// The soul forge and its six upgrades in the game, added as `game::armory_lib_forge`. What the
// mods declare (`soul_forge` in `armory_lib.json`) and the sums are in `armory_lib_forge_logic`
// (forge_logic.rs).
//
// - The forge: a right click on a forge block opens, at the next server tick, a one-row server
//   menu whose slots 0 to 2 hold items as a chest's do (the item, the relic, the full soul jar or
//   the upgrade tome) and whose slot 4 shows the item as it would come out. A click there checks
//   the forge's rule, takes the levels, then changes the three slots in one step: the item gains
//   the upgrade, one relic goes, the jar comes back empty (or one tome goes). Whatever is in the
//   slots when the menu closes goes back to the player, as Horizon gives a menu's input back.
//   A world save writes what the open forges hold to `<world>/armory_lib/forge/<uuid>.dat`, and
//   the next save takes the file away once nothing is held there; a world opened after a crash
//   gives those items back to their players. So the file always matches the saved world.
// - The upgrades live in the item's custom data (`armory_lib:upgrades`, a list of names):
//   sharpened adds damage to the player's weapon hits, swift is an attribute modifier on the
//   player while the item is in the main hand, reinforced lowers the wear Horizon reports,
//   warded lowers the damage a player takes, soulbound stays in the inventory through death,
//   radiant lights the world around who holds or wears it, and the tooltip names them.
use super::armory_lib_forge_logic as f;
use super::armory_lib_store as store;
use crate::game::Game;
use crate::mod_api::Host;
use horizon_client::items::ItemStack;
use horizon_client::lan::data::component_nbt;
use horizon_client::mod_api::{NativeColoredLight, NativeLootDrops};
use horizon_client::motion::PlayerMotion;
use horizon_client::text::{self, Text};
use horizon_world::attributes::{Modifier, Operation};
use horizon_world::nbt::Tag;
use horizon_world::player_inventory::EquipmentSlot;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic in another hook must not take the forge down with it.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The menu's slots that hold items, the preview's slot and the row's size.
const INPUT: std::ops::Range<usize> = 0..3;
const PREVIEW: usize = 4;
const ROW: usize = 9;
const PANE: &str = "minecraft:gray_stained_glass_pane";
const HINT_PANE: &str = "minecraft:purple_stained_glass_pane";
const FORGED_SOUND: &str = "minecraft:block.smithing_table.use";
/// `NativeServerPlayer.game_mode` of creative and spectator.
const CREATIVE: i32 = 1;
const SPECTATOR: i32 = 3;
/// Swift's modifiers on the player while a swift item is in the main hand.
const SWIFT_ATTACK: (&str, &str) = ("minecraft:attack_speed", "armory_lib:forge/swift_attack");
const SWIFT_MINING: (&str, &str) = ("minecraft:block_break_speed", "armory_lib:forge/swift_mining");
/// Radiant light: glowstone's warm colour.
const RADIANT_RGB: u32 = 0xffd98c;
/// A player not ticked for this long gets nothing from the damage hook.
const FRESH: Duration = Duration::from_secs(2);

/// `hooks.rs load()`, for each content mod's file: its `soul_forge`.
pub(super) fn read(mod_id: &str, file: &serde_json::Value) -> Vec<String> {
    f::read(mod_id, file)
}

/// The first of `forges` in force (the library on, its mod on, its switch on): its numbers are
/// the upgrades'. None when no mod declares a forge or none is in force.
fn in_force<'a>(game: &Game<'_>, forges: &'a f::Forges) -> Option<&'a f::ForgeDef> {
    if forges.is_empty() || !game.native_on(super::armory_lib_logic::ID) {
        return None;
    }
    forges
        .all()
        .find(|d| super::armory_lib_gear::on(game, &d.mod_id, d.setting.as_deref()))
}

/// The upgrades `stack` has.
pub(super) fn upgrades(stack: &ItemStack) -> Vec<f::Upgrade> {
    match store::custom(stack, f::UPGRADES_KEY) {
        Some(Tag::List(list)) => {
            let names: Vec<String> = list
                .iter()
                .filter_map(|t| match t {
                    Tag::String(s) => Some(s.to_string_lossy()),
                    _ => None,
                })
                .collect();
            f::upgrades_of(names.iter().map(String::as_str))
        }
        _ => Vec::new(),
    }
}

fn has(stack: &ItemStack, upgrade: f::Upgrade) -> bool {
    // Cheap first: most stacks have no custom data.
    stack.custom_data().is_some() && upgrades(stack).contains(&upgrade)
}

/// `stack` with `upgrade` added to its list.
fn with_upgrade(stack: &ItemStack, upgrade: f::Upgrade) -> ItemStack {
    let mut out = stack.clone();
    let mut list = upgrades(stack);
    list.push(upgrade);
    let names = list.iter().map(|u| Tag::String(u.name().into())).collect();
    store::set_custom(&mut out, f::UPGRADES_KEY, Some(Tag::List(names)));
    out
}

/// The kinds of `stack` for `def`: its lists, item tags read from the server's.
fn kinds_of(game: &Game<'_>, def: &f::ForgeDef, stack: &ItemStack) -> Vec<f::Kind> {
    def.kinds_of(&stack.item, |tag| {
        game.native_server_item_in_tag(stack, tag).unwrap_or(false)
    })
}

/// A text component for an item's name or lore: not italic, in `color`.
fn line(text: &str, color: &str) -> Tag {
    component_nbt(&json!({"text": text, "italic": false, "color": color}))
}

fn named(item: &str, name: Tag) -> ItemStack {
    let mut stack = ItemStack::new(item, 1);
    stack
        .data_mut()
        .set_other("minecraft:custom_name", Some(name));
    stack
}

// ---- The forge menu ----------------------------------------------------------------------

/// A forge menu open on the server, and what its preview was last made from (the three slots,
/// the player's levels, creative).
struct Open {
    uuid: u128,
    window: i32,
    mod_id: String,
    shown: Option<(Vec<Option<ItemStack>>, i32, bool)>,
}

/// Menus right clicks asked for, waiting for the next server tick (a player's own turn can't
/// reach the menus): the player, the forge's mod, the title.
static WAITING: Mutex<Vec<(u128, String, Text)>> = Mutex::new(Vec::new());
static OPEN: Mutex<Vec<Open>> = Mutex::new(Vec::new());

/// `native_right_click_block`, the player swapped in: a right click on a forge block takes the
/// click and asks for its menu. Sneaking with something in hand is left alone, so blocks can be
/// placed against it.
pub(super) fn on_right_click_block(game: &mut Game<'_>, pos: [i32; 3], canceled: &mut bool) {
    if *canceled || !game.native_authoritative() {
        return;
    }
    let block = game.native_block(pos);
    let forges = f::forges();
    let Some(def) = forges.by_block(&block) else {
        return;
    };
    if !game.native_on(super::armory_lib_logic::ID)
        || !super::armory_lib_gear::on(game, &def.mod_id, def.setting.as_deref())
    {
        return;
    }
    let Some(me) = game.native_acting_player() else {
        return;
    };
    let hands_full = game.native_equipment(EquipmentSlot::MainHand).is_some()
        || game.native_equipment(EquipmentSlot::OffHand).is_some();
    if me.sneaking && hands_full {
        return;
    }
    *canceled = true;
    if me.game_mode == SPECTATOR {
        return;
    }
    let (namespace, path) = block.split_once(':').unwrap_or(("minecraft", block.as_str()));
    let title = Text::translate_with_fallback(
        &format!("block.{namespace}.{path}"),
        "Soul Forge",
        &[],
    );
    let mut waiting = lock(&WAITING);
    waiting.retain(|(uuid, _, _)| *uuid != me.uuid);
    waiting.push((me.uuid, def.mod_id.clone(), title));
}

/// Whether the player's open menu is still the forge `window` (the hub in reach).
fn showing(game: &Game<'_>, uuid: u128, window: i32) -> bool {
    game.lan
        .as_ref()
        .and_then(|host| host.hub.guests.iter().find(|g| g.placed && g.uuid == uuid))
        .and_then(|g| g.menu.as_ref())
        .is_some_and(|m| m.window == window)
}

/// The row: the three slots (kept as the hub has them), a pane, the preview, panes.
fn row(preview: ItemStack) -> Vec<Option<ItemStack>> {
    let pane = named(PANE, Tag::String(" ".into()));
    (0..ROW)
        .map(|k| match k {
            k if INPUT.contains(&k) => None,
            PREVIEW => Some(preview.clone()),
            _ => Some(pane.clone()),
        })
        .collect()
}

/// What lies in the slots, as the rule reads it.
struct Slots<'a> {
    item: Option<&'a ItemStack>,
    relic: Option<&'a ItemStack>,
    payment: Option<&'a ItemStack>,
}

fn slots_of(contents: &[Option<ItemStack>]) -> Slots<'_> {
    let at = |k: usize| contents.get(k).and_then(Option::as_ref).filter(|s| s.count > 0);
    Slots {
        item: at(0),
        relic: at(1),
        payment: at(2),
    }
}

fn plan(
    game: &Game<'_>,
    def: &f::ForgeDef,
    s: &Slots<'_>,
    levels: i32,
    creative: bool,
) -> Result<f::Plan, f::Refusal> {
    let (has, kinds) = match s.item {
        Some(item) => (upgrades(item), kinds_of(game, def, item)),
        None => (Vec::new(), Vec::new()),
    };
    f::plan(
        def,
        s.item.map(|i| i.count),
        &has,
        &kinds,
        s.relic.map(|r| &*r.item),
        s.payment.map(|p| (&*p.item, p.count)),
        levels,
        creative,
    )
}

/// The preview: the item as it would come out, with what it costs; or a pane saying what is
/// missing or wrong.
fn preview(game: &Game<'_>, def: &f::ForgeDef, contents: &[Option<ItemStack>], levels: i32, creative: bool) -> ItemStack {
    let s = slots_of(contents);
    match plan(game, def, &s, levels, creative) {
        Ok(p) => {
            let Some(item) = s.item else {
                return named(HINT_PANE, line("?", "gray"));
            };
            let mut out = with_upgrade(item, p.upgrade);
            let cost = if p.tome {
                "Uses the relic and the upgrade tome".to_owned()
            } else if p.levels > 0 {
                format!("Uses the relic, the soul jar and {} levels", p.levels)
            } else {
                "Uses the relic and the soul jar".to_owned()
            };
            let lore = vec![line("Click to forge", "green"), line(&cost, "gray")];
            out.data_mut().set_other("minecraft:lore", Some(Tag::List(lore)));
            out
        }
        Err(why) => {
            let color = if why.is_hint() { "gray" } else { "red" };
            named(HINT_PANE, line(&why.text(), color))
        }
    }
}

/// `native_server_tick`, the menus in reach: the forge menus asked for open, and each open
/// forge's preview follows what lies in its slots.
pub(super) fn on_server_tick(game: &mut Game<'_>) {
    let forges = f::forges();
    let waiting = std::mem::take(&mut *lock(&WAITING));
    for (uuid, mod_id, title) in waiting {
        let Some(def) = forges.get(&mod_id) else {
            continue;
        };
        let first = preview(game, def, &[], 0, false);
        // Opening closes the menu they had open (heard at once), so no lock is held here.
        let Some(window) = game.native_open_menu_with_input(uuid, 1, title, row(first), INPUT)
        else {
            continue;
        };
        let mut open = lock(&OPEN);
        open.retain(|o| o.uuid != uuid);
        open.push(Open { uuid, window, mod_id, shown: None });
    }
    let open: Vec<(u128, i32, String)> = lock(&OPEN)
        .iter()
        .map(|o| (o.uuid, o.window, o.mod_id.clone()))
        .collect();
    if open.is_empty() {
        return;
    }
    let players = game.native_server_players();
    for (uuid, window, mod_id) in open {
        let (Some(def), Some(player)) = (
            forges.get(&mod_id),
            players.iter().find(|p| p.uuid == uuid),
        ) else {
            continue;
        };
        if !showing(game, uuid, window) {
            continue;
        }
        let Some(contents) = game.native_menu_contents(uuid) else {
            continue;
        };
        let key = (
            contents.get(INPUT).map(<[_]>::to_vec).unwrap_or_default(),
            player.xp_level,
            player.game_mode == CREATIVE,
        );
        let changed = {
            let mut all = lock(&OPEN);
            let Some(o) = all.iter_mut().find(|o| o.uuid == uuid && o.window == window) else {
                continue;
            };
            let changed = o.shown.as_ref() != Some(&key);
            o.shown = Some(key.clone());
            changed
        };
        if changed {
            let shown = preview(game, def, &key.0, key.1, key.2);
            game.native_set_menu_items(uuid, row(shown));
        }
    }
}

/// `native_menu_clicked`, no player swapped in, the menus in reach: a click on the preview
/// forges, or says on the action bar why not.
pub(super) fn on_menu_clicked(game: &mut Game<'_>, uuid: u128, window: i32, slot: i32, mode: i32) {
    // `ClickType` PICKUP and QUICK_MOVE.
    if slot != PREVIEW as i32 || !matches!(mode, 0 | 1) || !game.native_authoritative() {
        return;
    }
    let Some(mod_id) = lock(&OPEN)
        .iter()
        .find(|o| o.uuid == uuid && o.window == window)
        .map(|o| o.mod_id.clone())
    else {
        return;
    };
    let forges = f::forges();
    let Some(def) = forges.get(&mod_id) else {
        return;
    };
    let Some(player) = game.native_server_players().into_iter().find(|p| p.uuid == uuid) else {
        return;
    };
    let Some(contents) = game.native_menu_contents(uuid).filter(|_| showing(game, uuid, window))
    else {
        return;
    };
    let Some(before) = contents.get(INPUT).map(<[_]>::to_vec) else {
        return;
    };
    let s = slots_of(&before);
    let creative = player.game_mode == CREATIVE;
    let p = match plan(game, def, &s, player.xp_level, creative) {
        Ok(p) => p,
        Err(why) => {
            game.native_action_bar(uuid, Text::literal(why.text()));
            return;
        }
    };
    let (Some(item), Some(relic), Some(payment)) = (s.item, s.relic, s.payment) else {
        return;
    };
    let mut after: Vec<Option<ItemStack>> = vec![
        Some(with_upgrade(item, p.upgrade)),
        Some(ItemStack { count: relic.count - 1, ..relic.clone() }).filter(|r| r.count > 0),
        if p.tome {
            Some(ItemStack { count: payment.count - 1, ..payment.clone() }).filter(|t| t.count > 0)
        } else {
            Some(ItemStack::new(&def.empty_jar, 1))
        },
    ];
    // The levels first: when they can't be taken nothing changes.
    if p.levels > 0 && !game.native_give_levels(uuid, -(p.levels as i32)) {
        game.native_action_bar(uuid, Text::literal("The forge can't take your levels now"));
        return;
    }
    if !put_inputs(game, uuid, window, &before, &mut after) {
        if p.levels > 0 {
            game.native_give_levels(uuid, p.levels as i32);
        }
        return;
    }
    game.native_play_sound_to(uuid, FORGED_SOUND, "block", None, 1.0, 1.0);
    let done = format!("{} forged", p.upgrade.label());
    game.native_action_bar(uuid, Text::literal(done).with_color(text::LIGHT_PURPLE));
}

/// Replaces the forge's three slots with `after`, in the hub's own record of the open menu (the
/// helpers change only the slots a mod shows), when they still hold `before`.
fn put_inputs(
    game: &mut Game<'_>,
    uuid: u128,
    window: i32,
    before: &[Option<ItemStack>],
    after: &mut Vec<Option<ItemStack>>,
) -> bool {
    let Some(host) = game.lan.as_mut() else {
        return false;
    };
    let Some(g) = host.hub.guests.iter_mut().find(|g| g.placed && g.uuid == uuid) else {
        return false;
    };
    if !g.menu.as_ref().is_some_and(|m| m.window == window)
        || g.native_menu.get(INPUT) != Some(before)
    {
        return false;
    }
    for (k, stack) in std::mem::take(after).into_iter().enumerate() {
        g.native_menu[k] = stack;
    }
    true
}

fn forget(uuid: u128, window: i32) {
    lock(&OPEN).retain(|o| !(o.uuid == uuid && o.window == window));
}

/// `native_menu_input_closed`: a forge closed; what is in its slots goes back to the player
/// (Horizon gives it back), so it is only forgotten here.
pub(super) fn on_menu_input_closed(uuid: u128, window: i32) {
    forget(uuid, window);
}

/// `native_menu_closed`: as above, whichever is heard first.
pub(super) fn on_menu_closed(uuid: u128, window: i32) {
    forget(uuid, window);
}

// ---- What open forges held at the last save --------------------------------------------

/// The open world's forge files: the folder, the items owed to each player (from files a world
/// opened with: left by a crash), and the players with a file.
struct Kept {
    dir: PathBuf,
    owed: BTreeMap<u128, Vec<ItemStack>>,
    files: BTreeSet<u128>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

fn file_of(dir: &std::path::Path, uuid: u128) -> PathBuf {
    dir.join(format!("{uuid:032x}.dat"))
}

/// Runs `f` with the open world's forge files, read when the world is new to the library.
fn with_kept<T>(game: &Game<'_>, f: impl FnOnce(&mut Kept) -> T) -> Option<T> {
    let dir = store::path(game, "forge")?;
    let mut kept = lock(&KEPT);
    if kept.as_ref().is_none_or(|k| k.dir != dir) {
        let mut k = Kept { dir: dir.clone(), owed: BTreeMap::new(), files: BTreeSet::new() };
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(uuid) = name
                .strip_suffix(".dat")
                .and_then(|hex| u128::from_str_radix(hex, 16).ok())
            else {
                continue;
            };
            let path = entry.path();
            if horizon_client::save::read_dat(&path).is_none() {
                log::error!("Armory Lib: forge file {} doesn't read; left as it is", path.display());
                continue;
            }
            let items: Vec<ItemStack> = store::read_items(&path, 64)
                .into_iter()
                .flatten()
                .filter(|s| s.count > 0)
                .collect();
            k.files.insert(uuid);
            if !items.is_empty() {
                k.owed.insert(uuid, items);
            }
        }
        *kept = Some(k);
    }
    kept.as_mut().map(f)
}

/// `native_world_saved`: each player's file gets what their open forge holds now and what they
/// are still owed, so a crash finds it with the world as it was saved; a file with nothing to
/// hold goes. Items owed are given back at the player's next tick.
pub(super) fn on_world_saved(game: &mut Game<'_>) {
    let open: Vec<(u128, i32)> = lock(&OPEN).iter().map(|o| (o.uuid, o.window)).collect();
    let mut held: BTreeMap<u128, Vec<ItemStack>> = BTreeMap::new();
    for (uuid, window) in open {
        if !showing(game, uuid, window) {
            continue;
        }
        if let Some(contents) = game.native_menu_contents(uuid) {
            let items = contents.get(INPUT).unwrap_or_default().iter().flatten();
            held.entry(uuid).or_default().extend(items.filter(|s| s.count > 0).cloned());
        }
    }
    with_kept(game, |k| {
        let uuids: BTreeSet<u128> = k
            .files
            .iter()
            .chain(k.owed.keys())
            .chain(held.keys())
            .copied()
            .collect();
        for uuid in uuids {
            let items: Vec<Option<ItemStack>> = k
                .owed
                .get(&uuid)
                .into_iter()
                .flatten()
                .chain(held.get(&uuid).into_iter().flatten())
                .cloned()
                .map(Some)
                .collect();
            let path = file_of(&k.dir, uuid);
            if items.is_empty() {
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        k.files.remove(&uuid);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        k.files.remove(&uuid);
                    }
                    Err(e) => log::error!("Armory Lib: forge file {} not removed ({e})", path.display()),
                }
            } else {
                match store::write_items(&path, &items) {
                    Ok(()) => {
                        k.files.insert(uuid);
                    }
                    Err(e) => log::error!("Armory Lib: open soul forge not saved with the world ({e})"),
                }
            }
        }
    });
}

// ---- The upgrades at work ----------------------------------------------------------------

/// What a player's upgrades do where no game is at hand (the damage hook): their entity id,
/// the share of damage that reaches them, and when their tick last ran. Also the swift
/// modifiers put on them.
struct Worn {
    entity: i32,
    taken: f32,
    seen: Instant,
    swift: (bool, bool),
}

static WORN: Mutex<BTreeMap<u128, Worn>> = Mutex::new(BTreeMap::new());

/// `native_player_tick`, the player swapped in: items owed from a crash are given back; swift's
/// modifiers follow the main hand; warded pieces are noted for the damage hook.
pub(super) fn on_player_tick(game: &mut Game<'_>) {
    if !game.native_authoritative() {
        return;
    }
    let Some(uuid) = game.lan_acting_uuid else {
        return;
    };
    give_owed(game, uuid);
    let forges = f::forges();
    let def = in_force(game, &forges);
    let main = game.native_equipment(EquipmentSlot::MainHand);
    let mut taken = 1.0;
    let mut swift = (false, false);
    if let Some(def) = def {
        let worn = [EquipmentSlot::Head, EquipmentSlot::Chest, EquipmentSlot::Legs, EquipmentSlot::Feet]
            .into_iter()
            .filter_map(|slot| game.native_equipment(slot))
            .filter(|s| has(s, f::Upgrade::Warded))
            .count();
        taken = f::warded_share(worn, def.amount(f::Upgrade::Warded));
        if let Some(main) = &main
            && has(main, f::Upgrade::Swift)
        {
            let kinds = kinds_of(game, def, main);
            swift = (kinds.contains(&f::Kind::Weapons), kinds.contains(&f::Kind::Tools));
        }
    }
    // The id the damage hook names them by as a victim.
    let entity = game.native_acting_player().map_or(0, |p| p.entity_id);
    let before = {
        let mut all = lock(&WORN);
        if all.len() > 256 {
            all.retain(|u, w| *u == uuid || w.seen.elapsed() < FRESH * 30);
        }
        let w = all.entry(uuid).or_insert(Worn {
            entity,
            taken: 1.0,
            seen: Instant::now(),
            swift: (false, false),
        });
        (w.entity, w.taken, w.seen) = (entity, taken, Instant::now());
        std::mem::replace(&mut w.swift, swift)
    };
    if swift == (false, false) && before == swift {
        return;
    }
    let amount = f64::from(def.map_or(0.0, |d| d.amount(f::Upgrade::Swift)));
    let Some(body) = game.player.as_mut() else {
        return;
    };
    // Given again each tick it applies (a respawn's new body has none), taken once it doesn't.
    for ((attribute, id), on, was, op) in [
        (SWIFT_ATTACK, swift.0, before.0, Operation::AddMultipliedTotal),
        (SWIFT_MINING, swift.1, before.1, Operation::AddMultipliedBase),
    ] {
        if on {
            give(body, attribute, id, amount, op);
        } else if was {
            take(body, attribute, id);
        }
    }
}

/// Puts a transient modifier on the player's `attribute`, as `/attribute .. modifier add` does,
/// so their client hears it; left alone while it is there as it should be. (Also the gear's
/// set knockback.)
pub(super) fn give(body: &mut PlayerMotion, attribute: &str, id: &str, amount: f64, op: Operation) {
    let Some(key) = body.attributes.key(attribute) else {
        return;
    };
    let Some(handle) = body.attributes.get_instance(Some(key)).ok().flatten() else {
        return;
    };
    let there = body
        .attributes
        .instance(&handle)
        .ok()
        .and_then(|i| i.modifier(id).cloned());
    if there.is_some_and(|m| m.amount() == amount && m.operation() == op) {
        return;
    }
    let Ok(modifier) = Modifier::new(id, amount, op) else {
        return;
    };
    let _ = body.attributes.edit(&handle, |value, observer| {
        value.remove(modifier.id(), observer)?;
        value.add_transient(modifier, observer)
    });
}

pub(super) fn take(body: &mut PlayerMotion, attribute: &str, id: &str) {
    let Some(key) = body.attributes.key(attribute) else {
        return;
    };
    if let Some(handle) = body.attributes.lookup(Some(&key)) {
        let _ = body.attributes.edit(&handle, |value, observer| value.remove(id, observer));
    }
}

/// Gives the acting player what a crash left in their forge file. What doesn't fit waits for
/// room; the file follows at the next world save.
fn give_owed(game: &mut Game<'_>, uuid: u128) {
    let owed = with_kept(game, |k| k.owed.remove(&uuid)).flatten();
    let Some(owed) = owed else {
        return;
    };
    let mut left = Vec::new();
    for stack in owed {
        left.extend(game.native_give_stack(uuid, stack));
    }
    if !left.is_empty() {
        with_kept(game, |k| k.owed.insert(uuid, left));
    }
}

/// `native_living_incoming_damage` (no game; it runs without a trait mod, unlike `_v3`): a
/// player (`entity`, by entity id) wearing warded pieces takes less. A sharpened weapon's extra
/// damage is the attack's (`on_attack_entity`), which the gear's part of this hook adds.
pub(super) fn on_incoming_damage(
    entity: i32,
    kind: &str,
    damage_type: &str,
    amount: &mut f32,
    cancel: &mut bool,
) {
    // Like Protection, warding doesn't hold off the void or /kill.
    if *cancel
        || *amount <= 0.0
        || kind != "minecraft:player"
        || matches!(damage_type, "minecraft:out_of_world" | "minecraft:generic_kill")
    {
        return;
    }
    let all = lock(&WORN);
    if let Some(w) = all.values().find(|w| w.entity == entity && w.seen.elapsed() < FRESH) {
        *amount *= w.taken;
    }
}

/// `native_attack_entity`, the player swapped in: their blow starts. With a sharpened weapon in
/// the main hand each of its hits (the target, a sweep's others, a player) does more.
pub(super) fn on_attack_entity(game: &Game<'_>) {
    let Some(attacker) = game.player.as_ref().map(|p| p.entity_id) else {
        return;
    };
    if !game
        .native_equipment(EquipmentSlot::MainHand)
        .is_some_and(|w| has(&w, f::Upgrade::Sharpened))
    {
        return;
    }
    if let Some(def) = in_force(game, &f::forges()) {
        horizon_client::armory_lib_bridge::add_attack_bonus(attacker, def.amount(f::Upgrade::Sharpened));
    }
}

/// `native_item_damaged`, the player swapped in: a reinforced item takes part of its wear. Only
/// the item's id is told, so the stack is the first of that item in the main hand, the armour
/// and the off hand.
pub(super) fn on_item_damaged(game: &mut Game<'_>, item: &str, amount: &mut i32, cancel: &mut bool) {
    if *cancel || *amount <= 0 {
        return;
    }
    let worn = [
        EquipmentSlot::MainHand,
        EquipmentSlot::Head,
        EquipmentSlot::Chest,
        EquipmentSlot::Legs,
        EquipmentSlot::Feet,
        EquipmentSlot::OffHand,
    ]
    .into_iter()
    .filter_map(|slot| game.native_equipment(slot))
    .find(|s| &*s.item == item);
    if !worn.is_some_and(|s| has(&s, f::Upgrade::Reinforced)) {
        return;
    }
    if let Some(def) = in_force(game, &f::forges()) {
        *amount = f::kept_wear(*amount, def.amount(f::Upgrade::Reinforced), roll());
    }
}

/// A number from 0 to 1, different each call.
fn roll() -> f32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    (h.finish() >> 40) as f32 / (1u64 << 24) as f32
}

/// `native_loot_drops`: a player died and `drops` are what they held. Soulbound stacks go back
/// into the dying player's emptied inventory instead (their state is swapped in), so they stay
/// with them through the respawn, as with `keep_inventory`, and are saved with the player.
pub(super) fn on_loot_drops(game: &mut Game<'_>, context: &NativeLootDrops<'_>, drops: &mut Vec<ItemStack>) {
    let dying_player = context.entity.is_some_and(|(_, kind)| kind == "minecraft:player")
        && context.block.is_none()
        && context.player.is_some()
        && context.player == game.lan_acting_uuid;
    if !dying_player || !drops.iter().any(|s| has(s, f::Upgrade::Soulbound)) || in_force(game, &f::forges()).is_none() {
        return;
    }
    let mut i = 0;
    while i < drops.len() {
        if has(&drops[i], f::Upgrade::Soulbound)
            && let Some(free) = game.inventory.iter_mut().find(|s| s.is_none())
        {
            *free = Some(drops.remove(i));
        } else {
            i += 1;
        }
    }
}

// ---- The client: light and tooltips -----------------------------------------------------

/// `native_visuals` (the client's frame): a radiant item held or worn lights the world around
/// the player and around the creatures and other players that hold or wear one. Lights share
/// the library's budget of 8 with the gear light; the nearest are kept.
pub(super) fn on_visuals(game: &Game<'_>, lights: &mut Vec<NativeColoredLight>) {
    let forges = f::forges();
    let Some(def) = in_force(game, &forges) else {
        return;
    };
    let host: &dyn Host = game;
    let Some(state) = host.client_state() else {
        return;
    };
    let eye = match (state.eye_position, host.player()) {
        (Some(eye), _) => eye,
        (None, Some(p)) => [p.x, p.y + 1.6, p.z],
        (None, None) => return,
    };
    use crate::mod_api::EquipmentSlot as Slot;
    const SLOTS: [Slot; 6] = [Slot::MainHand, Slot::OffHand, Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet];
    let level = def.amount(f::Upgrade::Radiant);
    let glow = |at: [f64; 3]| NativeColoredLight {
        at,
        rgb: RADIANT_RGB,
        radius: level.clamp(0.25, 16.0),
        intensity: (level / 15.0).clamp(0.0, 1.0),
    };
    let before = lights.len();
    if SLOTS.iter().any(|&slot| host.equipment(slot).is_some_and(|s| has(&s, f::Upgrade::Radiant))) {
        lights.push(glow([eye[0], eye[1] - 0.4, eye[2]]));
    }
    let partial = state.world_partial_tick;
    let reach = super::armory_lib_logic::REACH;
    for e in host.client_entities() {
        let at = e.position_at(partial);
        let far = (0..3).map(|i| (at[i] - eye[i]).powi(2)).sum::<f64>();
        if !e.alive || e.invisible || e.spectator || far > reach * reach {
            continue;
        }
        let Some(living) = host.entity(e.id).and_then(|r| host.living(r)) else {
            continue;
        };
        let lit = SLOTS.iter().any(|&slot| {
            host.living_equipment(living, slot).is_some_and(|s| has(&s, f::Upgrade::Radiant))
        });
        if lit {
            lights.push(glow([at[0], at[1] + f64::from(e.height) * 0.6, at[2]]));
        }
    }
    if lights.len() > before && lights.len() > super::armory_lib_logic::MAX_LIGHTS {
        let far = |l: &NativeColoredLight| (0..3).map(|i| (l.at[i] - eye[i]).powi(2)).sum::<f64>();
        lights.sort_by(|a, b| far(a).total_cmp(&far(b)));
        lights.truncate(super::armory_lib_logic::MAX_LIGHTS);
    }
}

/// `native_tooltip` (client): each upgrade under the item's name, in light purple.
pub(super) fn on_tooltip(game: &Game<'_>, stack: &ItemStack, lines: &mut Vec<(Text, u32)>) {
    if stack.custom_data().is_none() {
        return;
    }
    let list = upgrades(stack);
    if list.is_empty() || in_force(game, &f::forges()).is_none() {
        return;
    }
    let at = lines.len().min(1);
    for (k, upgrade) in list.into_iter().enumerate() {
        let text = Text::translate_with_fallback(
            &format!("armory_lib.upgrade.{}", upgrade.name()),
            upgrade.label(),
            &[],
        )
        .with_color(text::LIGHT_PURPLE);
        lines.insert(at + k, (text, u32::MAX));
    }
}
