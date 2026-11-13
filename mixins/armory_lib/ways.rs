// Waystones and the recall charm, added to the game as `game::armory_lib_ways`. Which blocks are
// waystones and which items recall come from the mods' own files (`waystones` and `recall` in
// `armory_lib.json`), read by `armory_lib_ways_logic` (ways_logic.rs), which also holds the
// sums. Everything here runs on the server (single player's server thread, a LAN host, a
// dedicated server), inside the hooks named in each function.
//
// A world's waystones are kept in `<world>/armory_lib/waystones.json`: each one's id, place,
// name and the players who found it. The file is read the first time a hook needs it, and
// again when another world opens; it is written whole on every change.
use super::armory_lib_store as store;
use super::armory_lib_ways_logic as w;
use crate::game::Game;
use horizon_client::armory_lib_bridge as bridge;
use horizon_client::items::ItemStack;
use horizon_client::lan::data::component_nbt;
use horizon_client::particles::{ParticleOptions, ParticleType};
use horizon_client::text::Text;
use horizon_world::nbt::Tag;
use horizon_world::player_inventory::EquipmentSlot;
use horizon_world::registry::BlockStateId;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

const NAME_TAG: &str = "minecraft:name_tag";
const OVERWORLD: &str = "minecraft:overworld";
/// What an enderman's teleport sounds like, at both ends of a trip.
const TRAVEL_SOUND: &str = "minecraft:entity.enderman.teleport";
/// A lodestone compass locking on: a recall charm bound to a waystone.
const BIND_SOUND: &str = "minecraft:item.lodestone_compass.lock";
/// The `SoundSource` ordinal of players.
const PLAYERS: usize = 7;
/// `GameType.CREATIVE`'s id.
const CREATIVE: i32 = 1;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic in another hook must not take the waystones down with it.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn on(game: &Game<'_>, mod_id: &str, setting: Option<&str>) -> bool {
    super::armory_lib_gear::on(game, mod_id, setting)
}

/// `hooks.rs load()`, for each content mod's file: its `waystones` and `recall` sections, in
/// place of what it declared before. What it could not use, one line each.
pub(super) fn read(mod_id: &str, file: &serde_json::Value) -> Vec<String> {
    w::read_and_keep(mod_id, file)
}

// ---- The world's waystones --------------------------------------------------------------

/// The open world's waystones, and when each player last travelled between them (in
/// `bridge::now_ms`; the cooldown is the server's, so it doesn't outlive a restart).
struct World {
    file: PathBuf,
    registry: w::Registry,
    travelled: HashMap<u128, u64>,
}

static WORLD: Mutex<Option<World>> = Mutex::new(None);

/// Runs `f` with the open world's waystones, read from its file when the world is new to the
/// library. None with no world (a client).
fn with_world<T>(game: &Game<'_>, f: impl FnOnce(&mut World) -> T) -> Option<T> {
    let file = store::path(game, w::REGISTRY_FILE)?;
    let mut world = lock(&WORLD);
    if world.as_ref().is_none_or(|s| s.file != file) {
        *world = Some(World {
            registry: load(&file),
            file,
            travelled: HashMap::new(),
        });
    }
    world.as_mut().map(f)
}

/// The registry in `file`. One that doesn't parse is kept aside as `waystones.json.broken`
/// rather than written over, and the world starts with none.
fn load(file: &Path) -> w::Registry {
    if let Some(value) = store::read_json(file) {
        return w::Registry::from_json(&value);
    }
    if file.exists() {
        let aside = file.with_extension("json.broken");
        let kept = std::fs::copy(file, &aside).is_ok();
        eprintln!(
            "Armory Lib: {} is not readable JSON; {}",
            file.display(),
            if kept {
                "kept it as waystones.json.broken"
            } else {
                "it will be replaced"
            }
        );
    }
    w::Registry::default()
}

fn save(world: &World) {
    if let Err(e) = store::write_json(&world.file, &world.registry.to_json()) {
        eprintln!("Armory Lib: the waystones were not saved: {e}");
    }
}

/// Whether the waystone still stands: false only when its block can be seen (the level in
/// place, or one the server keeps loaded, in a loaded chunk) and is no longer a waystone
/// block. Explosions, pistons and commands remove blocks without a player's break, so a trip
/// checks first.
fn still_there(game: &Game<'_>, ways: &w::Ways, stone: &w::Waystone) -> bool {
    let [x, y, z] = stone.pos;
    let world = if game.dim.id() == stone.dimension {
        Some(&game.world)
    } else {
        let dim = match stone.dimension.as_str() {
            "minecraft:overworld" => super::Dim::Overworld,
            "minecraft:the_nether" => super::Dim::Nether,
            "minecraft:the_end" => super::Dim::End,
            _ => return true,
        };
        game.levels.parked_world(dim)
    };
    let Some(world) = world.filter(|w| w.loaded(x, z)) else {
        return true;
    };
    let state = world.block(x, y, z);
    ways.waystone(&game.res.facts().get(state).name).is_some()
}

/// Takes the waystone `id` out of the registry (its block is gone).
fn forget(game: &Game<'_>, id: &str) {
    with_world(game, |world| {
        if world.registry.remove(id).is_some() {
            save(world);
        }
    });
}

/// An enderman's teleport sound and portal particles at `at`, when `dimension` is the level in
/// place (the only one the game's sound and particle helpers reach).
fn flash(game: &mut Game<'_>, dimension: &str, at: [f64; 3]) {
    if game.dim.id() != dimension {
        return;
    }
    game.native_play_sound_in_place(TRAVEL_SOUND, at, PLAYERS, 1.0, 1.0);
    let portal = ParticleOptions::simple(ParticleType::Portal);
    let middle = [at[0], at[1] + 1.0, at[2]];
    game.native_send_particles_in_place(&portal, None, middle, [0.4, 0.9, 0.4], 0.4, 64);
}

// ---- The travel menu --------------------------------------------------------------------

/// A travel menu, opened or waiting to open: what each slot shows and the waystone it leads
/// to, the waystone it was opened at, and that waystone's rule (its cooldown and cost).
struct Menu {
    window: i32,
    here: String,
    slots: Vec<Option<String>>,
    items: Vec<Option<ItemStack>>,
    def: w::WaystoneDef,
}

/// Menus waiting for the next server tick: a right click is a player's own turn, where the
/// other players' menus are out of the game's hands (`native_open_menu` reaches none).
static WAITING: Mutex<Vec<(u128, u8, String, Menu)>> = Mutex::new(Vec::new());
static ANY_WAITING: AtomicBool = AtomicBool::new(false);
/// The open travel menus, by player.
static MENUS: Mutex<Vec<(u128, Menu)>> = Mutex::new(Vec::new());

/// A text component for an item's name or lore: not italic (vanilla shows a custom name and
/// lore in italics), in `color`.
fn line(text: &str, color: &str) -> Tag {
    component_nbt(&json!({"text": text, "italic": false, "color": color}))
}

/// The menu's item for `stone`: its dimension's icon, its name, and where it is.
fn entry_item(stone: &w::Waystone, lore: Vec<Tag>, here: bool) -> ItemStack {
    let mut item = ItemStack::new(w::icon(&stone.dimension), 1);
    let extra = item.extra.get_or_insert_with(Default::default);
    let color = if here { "yellow" } else { "aqua" };
    extra.set_other("minecraft:custom_name", Some(line(&stone.name, color)));
    extra.set_other("minecraft:lore", Some(Tag::List(lore)));
    if here {
        extra.set_other("minecraft:enchantment_glint_override", Some(Tag::Byte(1)));
    }
    item
}

/// The travel menu of a player at `at` in `dimension`, standing at the waystone `here`:
/// `left` seconds of cooldown to go, `cost` levels a trip.
fn build_menu(
    entries: &[w::Waystone],
    here: &str,
    dimension: &str,
    at: [f64; 3],
    left: u32,
    cost: u32,
) -> (Vec<Option<String>>, Vec<Option<ItemStack>>) {
    let mut slots = Vec::with_capacity(entries.len());
    let mut items = Vec::with_capacity(entries.len());
    for stone in entries {
        let is_here = stone.id == here;
        let [x, y, z] = stone.pos;
        let mut lore = vec![
            line(&format!("{x}, {y}, {z}"), "gray"),
            line(&w::dimension_name(&stone.dimension), "gray"),
        ];
        if stone.dimension == dimension {
            let blocks = w::distance(at, stone.pos).round() as i64;
            let away = if blocks == 1 {
                "1 block away".to_owned()
            } else {
                format!("{blocks} blocks away")
            };
            lore.push(line(&away, "gray"));
        } else {
            lore.push(line("In another dimension", "gray"));
        }
        if is_here {
            lore.push(line("You are here", "yellow"));
        } else {
            if cost > 0 {
                lore.push(line(&format!("Costs {}", w::levels(cost)), "gray"));
            }
            if left > 0 {
                lore.push(line(&format!("Cooldown: {left}s"), "red"));
            } else {
                lore.push(line("Click to travel", "green"));
            }
        }
        slots.push(Some(stone.id.clone()));
        items.push(Some(entry_item(stone, lore, is_here)));
    }
    (slots, items)
}

/// `native_right_click_block`: a player right clicks the block at `pos`, with their state
/// swapped in. On a waystone the click is taken (the block's own use doesn't run), and the
/// waystone is registered the first time anyone clicks it. A name tag with a name renames it;
/// a recall item, while sneaking, is bound to it; anything else opens its travel menu.
/// Sneaking with another item in hand is left alone, as vanilla skips a block's own use then
/// (so a block can be placed against the waystone).
pub(super) fn on_right_click_block(game: &mut Game<'_>, pos: [i32; 3], canceled: &mut bool) {
    if *canceled || !game.native_authoritative() {
        return;
    }
    let ways = w::ways();
    let Some(def) = ways.waystone(&game.native_block(pos)).cloned() else {
        return;
    };
    if !on(game, &def.mod_id, def.setting.as_deref()) {
        return;
    }
    let Some(me) = game.native_acting_player() else {
        return;
    };
    let held = game.held().cloned();
    let rename = held
        .as_ref()
        .filter(|s| &*s.item == NAME_TAG)
        .and_then(ItemStack::custom_name)
        .and_then(|n| w::clean_name(&n));
    let bind = me.sneaking
        && held
            .as_ref()
            .and_then(|s| ways.recall(&s.item))
            .is_some_and(|r| on(game, &r.mod_id, r.setting.as_deref()));
    let hands_full = game.native_equipment(EquipmentSlot::MainHand).is_some()
        || game.native_equipment(EquipmentSlot::OffHand).is_some();
    if me.sneaking && hands_full && rename.is_none() && !bind {
        return;
    }

    let dimension = game.dim.id().to_owned();
    let name = w::default_name(game.native_biome(pos).as_deref(), pos);
    let now = bridge::now_ms();
    let found = with_world(game, |world| {
        let (index, new) = match world.registry.index_at(&dimension, pos) {
            Some(i) => (i, false),
            None => (
                world.registry.add(store::new_id(), &dimension, pos, &name),
                true,
            ),
        };
        let stone = &mut world.registry.stones[index];
        let first = stone.mark_found(me.uuid);
        if let Some(name) = &rename {
            stone.name = name.clone();
        }
        let stone = stone.clone();
        if new || first || rename.is_some() {
            save(world);
        }
        let entries: Vec<w::Waystone> = world
            .registry
            .menu(me.uuid, &dimension, me.position)
            .into_iter()
            .cloned()
            .collect();
        let last = world.travelled.get(&me.uuid).copied();
        (stone, first, entries, last)
    });
    let Some((stone, first, entries, last)) = found else {
        return;
    };
    *canceled = true;
    if first {
        game.native_tell(
            me.uuid,
            Text::literal(format!("Waystone found: {}", stone.name)),
        );
    }

    if let Some(name) = rename {
        game.consume_held();
        game.native_tell(me.uuid, Text::literal(format!("Waystone renamed: {name}")));
        return;
    }
    if bind {
        if let Some(charm) = game.held_mut().as_mut() {
            store::set_custom(
                charm,
                w::RECALL_KEY,
                Some(Tag::String(stone.id.as_str().into())),
            );
        }
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ];
        game.native_play_sound_in_place(BIND_SOUND, at, PLAYERS, 1.0, 1.0);
        let text = format!("Recall charm bound to {}", stone.name);
        game.native_tell(me.uuid, Text::literal(text));
        return;
    }

    let left = w::seconds_left(last, now, def.cooldown_seconds);
    let cost = if game.creative { 0 } else { def.cost_levels };
    let (slots, items) = build_menu(&entries, &stone.id, &dimension, me.position, left, cost);
    let menu = Menu {
        window: 0,
        here: stone.id.clone(),
        slots,
        items,
        def,
    };
    lock(&WAITING).push((me.uuid, w::rows(entries.len()), stone.name, menu));
    ANY_WAITING.store(true, Ordering::Release);
}

/// `native_server_tick`, with the players' menus in reach: the travel menus right clicks
/// asked for since the last one are opened.
pub(super) fn on_server_tick(game: &mut Game<'_>) {
    if !ANY_WAITING.swap(false, Ordering::AcqRel) {
        return;
    }
    let waiting = std::mem::take(&mut *lock(&WAITING));
    for (uuid, rows, title, menu) in waiting {
        // Opening one closes the menu they had open (`native_menu_closed` hears it), so no
        // lock is held here.
        let opened = game.native_open_menu(uuid, rows, Text::literal(title), menu.items.clone());
        let Some(window) = opened else {
            continue;
        };
        let mut menus = lock(&MENUS);
        menus.retain(|(player, _)| *player != uuid);
        menus.push((uuid, Menu { window, ..menu }));
    }
}

/// `native_menu_closed`: the player's travel menu is gone (a menu of another mod may reuse its
/// window id).
pub(super) fn on_menu_closed(uuid: u128, window: i32) {
    lock(&MENUS).retain(|(player, menu)| !(*player == uuid && menu.window == window));
}

/// Takes the waystone in `slot` out of the player's open travel menu. The menu helpers reach
/// whatever server menu the player has open, so only one still showing this menu's items is
/// changed.
fn clear_slot(game: &mut Game<'_>, uuid: u128, window: i32, slot: usize) {
    let (before, after) = {
        let mut menus = lock(&MENUS);
        let Some((_, menu)) = menus
            .iter_mut()
            .find(|(p, m)| *p == uuid && m.window == window)
        else {
            return;
        };
        let before = menu.items.clone();
        if let (Some(s), Some(i)) = (menu.slots.get_mut(slot), menu.items.get_mut(slot)) {
            (*s, *i) = (None, None);
        }
        (before, menu.items.clone())
    };
    let ours = game.native_menu_contents(uuid).is_some_and(|shown| {
        shown.len() >= before.len()
            && shown[..before.len()] == before[..]
            && shown[before.len()..].iter().all(Option::is_none)
    });
    if ours {
        game.native_set_menu_items(uuid, after);
    }
}

/// `native_menu_clicked`, with no player's state swapped in: a click (a plain or shift click)
/// on a waystone of a player's travel menu takes them there, unless their cooldown runs or
/// they lack the levels. A waystone whose block is gone is forgotten instead.
pub(super) fn on_menu_clicked(game: &mut Game<'_>, uuid: u128, window: i32, slot: i32, mode: i32) {
    // `ClickType` PICKUP and QUICK_MOVE; a double click's PICKUP_ALL, a drag or a throw is not
    // a choice.
    if !matches!(mode, 0 | 1) || !game.native_authoritative() {
        return;
    }
    let Ok(slot) = usize::try_from(slot) else {
        return;
    };
    let picked = lock(&MENUS)
        .iter()
        .find(|(p, m)| *p == uuid && m.window == window)
        .and_then(|(_, m)| Some((m.slots.get(slot)?.clone()?, m.here.clone(), m.def.clone())));
    let Some((id, here, def)) = picked else {
        return;
    };
    // The waystone they stand at is shown, and does nothing.
    if id == here || !on(game, &def.mod_id, def.setting.as_deref()) {
        return;
    }
    let Some(player) = game
        .native_server_players()
        .into_iter()
        .find(|p| p.uuid == uuid)
    else {
        return;
    };
    let now = bridge::now_ms();
    let Some((stone, last)) = with_world(game, |world| {
        (
            world.registry.get(&id).cloned(),
            world.travelled.get(&uuid).copied(),
        )
    }) else {
        return;
    };
    let Some(stone) = stone else {
        game.native_tell(uuid, Text::literal("That waystone is gone"));
        clear_slot(game, uuid, window, slot);
        return;
    };
    let left = w::seconds_left(last, now, def.cooldown_seconds);
    if left > 0 {
        game.native_action_bar(
            uuid,
            Text::literal(format!("You can travel again in {left}s")),
        );
        return;
    }
    let cost = if player.game_mode == CREATIVE {
        0
    } else {
        def.cost_levels
    };
    if cost > 0 && i64::from(player.xp_level) < i64::from(cost) {
        game.native_action_bar(uuid, Text::literal(format!("You need {}", w::levels(cost))));
        return;
    }
    if !still_there(game, &w::ways(), &stone) {
        forget(game, &stone.id);
        let text = format!("{} is no longer there, so it was forgotten", stone.name);
        game.native_tell(uuid, Text::literal(text));
        clear_slot(game, uuid, window, slot);
        return;
    }
    game.native_close_menu(uuid);
    let to = w::landing(stone.pos);
    if !game.native_teleport_player(uuid, &stone.dimension, to, player.yaw, player.pitch) {
        game.native_action_bar(uuid, Text::literal("You can't travel there now"));
        return;
    }
    if cost > 0 {
        game.native_give_levels(uuid, -(cost as i32));
    }
    with_world(game, |world| world.travelled.insert(uuid, now));
    flash(game, &player.dimension, player.position);
    flash(game, &stone.dimension, to);
}

/// `native_destroy_block`: a player breaks a waystone, which leaves the registry (and the
/// menus of everyone who found it).
pub(super) fn on_destroy_block(
    game: &mut Game<'_>,
    pos: [i32; 3],
    state: BlockStateId,
    canceled: &mut bool,
) {
    if *canceled || !game.native_authoritative() {
        return;
    }
    let Some(block) = game.native_state_block(state).map(str::to_owned) else {
        return;
    };
    if w::ways().waystone(&block).is_none() {
        return;
    }
    let dimension = game.dim.id().to_owned();
    let gone = with_world(game, |world| {
        let gone = world.registry.remove_at(&dimension, pos);
        if gone.is_some() {
            save(world);
        }
        gone
    })
    .flatten();
    if let (Some(stone), Some(me)) = (gone, game.native_acting_player()) {
        game.native_tell(
            me.uuid,
            Text::literal(format!("Waystone destroyed: {}", stone.name)),
        );
    }
}

// ---- The recall charm -------------------------------------------------------------------

/// `native_use_item`: a player uses `stack` (a right click), with their state swapped in. A
/// recall item takes them to the waystone it is bound to, else to their bed or respawn
/// anchor, else to the world spawn, for its levels (none in creative). It then cools down as
/// an ender pearl does, so the hotbar greys it out; the game uses no item on cooldown.
pub(super) fn on_use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if *claimed || !game.native_authoritative() {
        return;
    }
    let ways = w::ways();
    let Some(def) = ways.recall(&stack.item).cloned() else {
        return;
    };
    if !on(game, &def.mod_id, def.setting.as_deref()) {
        return;
    }
    let Some(me) = game.native_acting_player() else {
        return;
    };
    // Taken whatever happens, so the item's own use never runs.
    *claimed = true;
    let cost = if game.creative { 0 } else { def.cost_levels };
    if cost > 0 && i64::from(me.xp_level) < i64::from(cost) {
        game.native_action_bar(
            me.uuid,
            Text::literal(format!("You need {}", w::levels(cost))),
        );
        return;
    }
    let Some((dimension, to, yaw)) = recall_to(game, &ways, stack, me.uuid) else {
        game.native_action_bar(me.uuid, Text::literal("There is nowhere to recall to"));
        return;
    };
    let yaw = yaw.unwrap_or(me.yaw);
    if !game.native_teleport_player(me.uuid, &dimension, to, yaw, me.pitch) {
        game.native_action_bar(me.uuid, Text::literal("You can't be recalled now"));
        return;
    }
    if cost > 0 {
        game.native_give_levels(me.uuid, -(cost as i32));
    }
    if def.cooldown_seconds > 0 {
        let group = horizon_client::cooldowns::group(stack, None);
        game.add_cooldown(group, w::cooldown_ticks(def.cooldown_seconds));
    }
    flash(game, &me.dimension, me.position);
    flash(game, &dimension, to);
}

/// Where a recall item takes the acting player: the waystone it is bound to while that stands,
/// else [`home`]. The dimension, the position and the yaw to face (None: their own). A charm
/// whose waystone is gone loses its binding (the hand that uses it holds it), so the player is
/// told once.
fn recall_to(
    game: &mut Game<'_>,
    ways: &w::Ways,
    stack: &ItemStack,
    uuid: u128,
) -> Option<(String, [f64; 3], Option<f32>)> {
    if let Some(id) = store::custom_text(stack, w::RECALL_KEY) {
        let stone = with_world(game, |world| world.registry.get(&id).cloned()).flatten();
        let name = match stone {
            Some(stone) if still_there(game, ways, &stone) => {
                return Some((stone.dimension.clone(), w::landing(stone.pos), None));
            }
            Some(stone) => {
                forget(game, &stone.id);
                stone.name
            }
            None => "Its waystone".to_owned(),
        };
        if let Some(charm) = game.held_mut().as_mut()
            && charm.item == stack.item
        {
            store::set_custom(charm, w::RECALL_KEY, None);
        }
        let text = format!("{name} is gone: the charm takes you home now");
        game.native_tell(uuid, Text::literal(text));
    }
    home(game)
}

/// The acting player's respawn point, as `ServerPlayer.findRespawnAndUseSpawnBlock` finds it
/// without using an anchor's charge: beside their bed or anchor when its level is in place;
/// on top of its block when it is in another level; the world spawn when the bed or anchor is
/// gone or blocked, or they have none.
fn home(game: &mut Game<'_>) -> Option<(String, [f64; 3], Option<f32>)> {
    if let Some(point) = game.respawn_config {
        let [x, y, z] = point.pos;
        let dimension = point.dim.id().to_owned();
        if point.dim != game.dim || !game.world.loaded(x, z) {
            let top = [f64::from(x) + 0.5, f64::from(y) + 1.0, f64::from(z) + 0.5];
            return Some((dimension, top, Some(point.yaw)));
        }
        if let Ok(Some((at, yaw))) = game.find_respawn_and_use(point, false) {
            return Some((dimension, [at.x, at.y, at.z], Some(yaw)));
        }
    } else if let Some(fallback) = game.spawn {
        // The point they fall back to with no bed or anchor (one `spawnpoint` set, or the
        // world spawn as it stood), in the Overworld.
        return Some((
            OVERWORLD.to_owned(),
            [fallback.x, fallback.y, fallback.z],
            None,
        ));
    }
    let [x, y, z] = game.level.world_spawn?;
    let at = [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5];
    Some((
        OVERWORLD.to_owned(),
        at,
        Some(game.level.world_spawn_angle[0]),
    ))
}
