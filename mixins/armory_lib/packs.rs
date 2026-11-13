// Backpacks and soul jars in the game, added as `game::armory_lib_packs`. Which items are
// backpacks and jars comes from the mods' files, kept with the sums in
// `armory_lib_packs_logic` (packs_logic.rs).
//
// A backpack item carries only an id (`armory_lib:bag` in its custom data); what it holds is
// the world's, in `<world>/armory_lib/backpacks/<id>.dat`. A right click opens a server chest
// menu (Horizon's `native_open_menu_with_input`) whose every slot is an input slot, filled
// from that file, and its closing writes the file again. So dropping or throwing the bag
// while it is open moves no item, and copies of one bag (a creative player's) share what they
// hold. One bag id is open for one player at a time.
//
// Horizon gives the menu's input slots to `native_menu_input_closed` whenever it closes (the
// player's Escape, a death, leaving, another menu), wherever that happens, before they go back
// to the player: the file is written there, and only once it is written are the stacks taken
// out of the slots. A write that fails leaves them all to go back to the player, and the old
// file goes, so nothing is in two places.
//
// A soul jar takes a soul for each mob its carrier kills with a blow, in the first jar of
// their hotbar (then their off hand), and at its capacity becomes its full item. Its fill
// bar and tooltip line are drawn on the client from the same rules.
use super::armory_lib_packs_logic as p;
use super::armory_lib_store as store;
use crate::game::Game;
use horizon_client::items::ItemStack;
use horizon_client::particles::ParticleOptions;
use horizon_client::text::{self, Text};
use horizon_world::nbt::Tag;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// The custom data key of a backpack's id (text).
const BAG_KEY: &str = "armory_lib:bag";
/// The custom data key of the souls in a jar (int).
const SOULS_KEY: &str = "armory_lib:souls";
/// A backpack opening and closing: leather armour going on.
const BAG_SOUND: &str = "minecraft:item.armor.equip_leather";
/// A jar filling up: a respawn anchor taking a charge.
const FULL_SOUND: &str = "minecraft:block.respawn_anchor.charge";
const SOUL: &str = "minecraft:soul";
/// Soul-fire cyan.
const SOUL_RGB: u32 = 0x5e_e8_ff;
/// The `SoundSource` ordinal of players.
const PLAYERS: usize = 7;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic in another hook must not take the backpacks down with it.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A backpack a player has open.
struct Open {
    uuid: u128,
    window: i32,
    bag: String,
    /// Its file, kept from the opening: the close may come where no world is in reach.
    path: PathBuf,
    /// `native_menu_closed` was heard for it before its input slots were (as when the server
    /// closes it): they still come, in the same pass. Left over at a level's end, it is stale.
    closed: bool,
}

/// A right click on a backpack, waiting for the end of its player's level tick: a player's
/// item use runs with the server's menus out of reach, and a menu opens there.
struct Request {
    uuid: u128,
    bag: String,
    path: PathBuf,
    rows: u8,
    title: Text,
}

struct State {
    open: Vec<Open>,
    requests: Vec<Request>,
    /// Players to play the backpack sound at, at the end of their level's tick.
    sounds: Vec<u128>,
}

fn state() -> MutexGuard<'static, State> {
    static S: Mutex<State> = Mutex::new(State {
        open: Vec::new(),
        requests: Vec::new(),
        sounds: Vec::new(),
    });
    lock(&S)
}

/// Whether some player has bag `bag` open.
fn is_open(bag: &str) -> bool {
    state().open.iter().any(|o| o.bag == bag)
}

fn open_elsewhere() -> Text {
    Text::translate_with_fallback(
        "armory_lib.backpack.open_elsewhere",
        "This backpack is open somewhere else",
        &[],
    )
}

/// `native_use_item`, with the player swapped in: a right click with a backpack. It is given
/// an id the first time, and its menu opens at the end of the tick.
pub(super) fn on_use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if *claimed || !game.native_authoritative() {
        return;
    }
    let packs = p::packs();
    let Some(def) = packs.backpack(&stack.item) else {
        return;
    };
    if !super::armory_lib_gear::on(game, &def.mod_id, def.setting.as_deref()) {
        return;
    }
    let Some(me) = game.native_acting_player() else {
        return;
    };
    *claimed = true;
    let bag = match store::custom_text(stack, BAG_KEY).filter(|id| p::valid_bag_id(id)) {
        Some(id) => id,
        None => {
            // `stack` is a copy of the held one: the id goes on the held stack itself.
            let Some(held) = game.held_mut().as_mut().filter(|h| h.item == stack.item) else {
                return;
            };
            let id = store::new_id();
            store::set_custom(held, BAG_KEY, Some(Tag::String(id.as_str().into())));
            id
        }
    };
    let Some(path) = store::path(game, &p::bag_file(&bag)) else {
        return;
    };
    if is_open(&bag) {
        game.native_action_bar(me.uuid, open_elsewhere());
        return;
    }
    let request = Request {
        uuid: me.uuid,
        bag,
        path,
        rows: def.rows,
        title: title_of(stack),
    };
    let mut s = state();
    s.requests.retain(|r| r.uuid != me.uuid);
    s.requests.push(request);
}

/// The menu's title, as a renamed chest's: the stack's custom name, else its item name
/// component, else the item's own name, which the client translates (`item.mymod.small_pack`,
/// or the item's id made readable where it has no translation).
fn title_of(stack: &ItemStack) -> Text {
    let data = stack.data();
    let named = data
        .and_then(|d| d.custom_name.as_ref())
        .or_else(|| data.and_then(|d| d.other("minecraft:item_name")));
    if let Some(tag) = named {
        // The component goes to the client as the stack has it, formatting and all.
        return horizon_client::component::name_text(tag)
            .with_component(horizon_client::scoreboard::tag_to_json(tag));
    }
    Text::translate_with_fallback(
        &p::name_key(&stack.item),
        &p::name_fallback(&stack.item),
        &[],
    )
}

/// `native_level_tick_post`: the level `dimension` has ticked, with it in place and the
/// server's menus in reach. The backpacks its players asked to open open, and the sounds of
/// those opened and closed play where they stand.
pub(super) fn on_level_tick_post(game: &mut Game<'_>, dimension: &str) {
    {
        let mut s = state();
        // Stale: its menu closed and its input slots were never handed over (they come in the
        // same pass as the close when they come), as when the server drops a player's record
        // outright. Its file keeps what it held when it opened or the world last saved.
        s.open.retain(|o| {
            if o.closed {
                log::error!(
                    "Armory Lib: backpack {} closed without handing back its items; its file \
                     keeps what it held when opened or last saved",
                    o.bag
                );
            }
            !o.closed
        });
        if s.requests.is_empty() && s.sounds.is_empty() {
            return;
        }
    }
    if game.dim.id() != dimension || !game.native_authoritative() {
        return;
    }
    let players = game.native_server_players();
    // Where a player of this level stands, at chest height.
    let here = |uuid: u128| {
        players
            .iter()
            .find(|p| p.uuid == uuid && p.dimension == dimension)
            .map(|p| [p.position[0], p.position[1] + 1.0, p.position[2]])
    };
    let (requests, sounds) = {
        let mut s = state();
        // Those of players who left, or died before their bag opened, are dropped; those of
        // players in another level wait for it.
        let gone = |uuid: u128| {
            players
                .iter()
                .find(|p| p.uuid == uuid)
                .is_none_or(|p| p.health <= 0.0)
        };
        s.requests.retain(|r| !gone(r.uuid));
        s.sounds
            .retain(|&uuid| players.iter().any(|p| p.uuid == uuid));
        let (now, later): (Vec<Request>, Vec<Request>) = std::mem::take(&mut s.requests)
            .into_iter()
            .partition(|r| here(r.uuid).is_some());
        s.requests = later;
        let (now_sounds, later_sounds): (Vec<u128>, Vec<u128>) = std::mem::take(&mut s.sounds)
            .into_iter()
            .partition(|&u| here(u).is_some());
        s.sounds = later_sounds;
        (now, now_sounds)
    };
    for uuid in sounds {
        if let Some(at) = here(uuid) {
            game.native_play_sound_in_place(BAG_SOUND, at, PLAYERS, 1.0, 1.0);
        }
    }
    for request in requests {
        let at = here(request.uuid);
        if open_bag(game, request)
            && let Some(at) = at
        {
            game.native_play_sound_in_place(BAG_SOUND, at, PLAYERS, 1.0, 1.0);
        }
    }
}

/// Opens the bag a request names, filled from its file; true when it opened.
fn open_bag(game: &mut Game<'_>, r: Request) -> bool {
    // Checked again: someone may have opened it since the click.
    if is_open(&r.bag) {
        game.native_action_bar(r.uuid, open_elsewhere());
        return false;
    }
    // A file that is there but doesn't read is left alone rather than written over.
    if r.path.exists() && horizon_client::save::read_dat(&r.path).is_none() {
        log::error!(
            "Armory Lib: backpack file {} doesn't read; not opening it",
            r.path.display()
        );
        game.native_action_bar(
            r.uuid,
            Text::translate_with_fallback(
                "armory_lib.backpack.unreadable",
                "This backpack's contents can't be read",
                &[],
            ),
        );
        return false;
    }
    let mut items = store::read_items(&r.path, p::MAX_SLOTS);
    let rows = p::rows_to_show(r.rows, items.iter().map(Option::is_some));
    let size = usize::from(rows) * 9;
    items.truncate(size);
    // No lock is held here: opening closes the player's open menu first, which may be
    // another bag of theirs, and its closing is heard at once.
    let Some(window) = game.native_open_menu_with_input(r.uuid, rows, r.title, items, 0..size)
    else {
        return false;
    };
    let mut s = state();
    s.open.retain(|o| !(o.uuid == r.uuid && o.window == window));
    s.open.push(Open {
        uuid: r.uuid,
        window,
        bag: r.bag,
        path: r.path,
        closed: false,
    });
    true
}

/// `native_menu_input_closed`: a menu with input slots closed, wherever that happened (no
/// game is used here). For a bag of ours, every stack but the backpacks in `slots` is written
/// to its file and then taken out of `slots`; the backpacks stay, and go back to the player.
pub(crate) fn on_menu_input_closed(uuid: u128, window: i32, slots: &mut [Option<ItemStack>]) {
    let open = {
        let mut s = state();
        let i = s
            .open
            .iter()
            .position(|o| o.uuid == uuid && o.window == window);
        i.map(|i| s.open.remove(i))
    };
    let Some(open) = open else {
        return;
    };
    let packs = p::packs();
    // Backpacks don't go inside backpacks.
    let refused = |s: &ItemStack| packs.backpack(&s.item).is_some();
    let kept = p::to_keep(slots, refused);
    match store::write_items(&open.path, &kept) {
        Ok(()) => p::take_kept(slots, refused),
        Err(e) => {
            // Everything goes back to the player. The file as it was when the bag opened (or
            // the world last saved) must not stay, or what it held would be both there and
            // with the player.
            log::error!(
                "Armory Lib: backpack {} not saved ({e}); its items go back to its player",
                open.bag
            );
            match std::fs::remove_file(&open.path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => log::error!(
                    "Armory Lib: backpack {}'s old file {} can't be removed either ({e}): what \
                     it held before may now also be with its player",
                    open.bag,
                    open.path.display()
                ),
            }
        }
    }
    state().sounds.push(uuid);
}

/// `native_world_saved`: the world and its players were just saved. Each open bag's file gets
/// what its menu holds now, so a crash before it closes finds the bag as it was when its
/// player's inventory was saved, not as it was when it opened. Its closing writes it again.
pub(super) fn on_world_saved(game: &mut Game<'_>) {
    let open: Vec<(u128, i32, String, PathBuf)> = state()
        .open
        .iter()
        .filter(|o| !o.closed)
        .map(|o| (o.uuid, o.window, o.bag.clone(), o.path.clone()))
        .collect();
    for (uuid, window, bag, path) in open {
        // Its menu is still the one the player has open (the server's menus in reach).
        let showing = game
            .lan
            .as_ref()
            .and_then(|host| host.hub.guests.iter().find(|g| g.placed && g.uuid == uuid))
            .and_then(|g| g.menu.as_ref())
            .is_some_and(|m| m.window == window);
        let Some(contents) = game.native_menu_contents(uuid).filter(|_| showing) else {
            continue;
        };
        if let Err(e) = store::write_items(&path, &contents) {
            log::warn!("Armory Lib: open backpack {bag} not saved with the world ({e})");
        }
    }
}

/// `native_menu_closed`: a menu a mod opened closed. A bag of ours is only marked, as its
/// input slots may still come (the server's closes are heard in this order).
pub(crate) fn on_menu_closed(uuid: u128, window: i32) {
    let mut s = state();
    if let Some(o) = s
        .open
        .iter_mut()
        .find(|o| o.uuid == uuid && o.window == window)
    {
        o.closed = true;
    }
}

// ---- Soul jars ---------------------------------------------------------------------------

/// `native_mob_killed`, with the player swapped in: their blow killed mob `entity`. Their first
/// jar that isn't full (the hotbar left to right, then the off hand) takes its soul.
pub(super) fn on_mob_killed(game: &mut Game<'_>, player: u128, entity: u32) {
    if !game.native_authoritative() {
        return;
    }
    let packs = p::packs();
    if !packs.has_jars() {
        return;
    }
    let Some(me) = game.native_acting_player().filter(|me| me.uuid == player) else {
        return;
    };
    let slots = (0..9).chain([super::shield::OFFHAND_SLOT]);
    let found = p::first_jar(
        slots.map(|slot| (slot, game.stack_in(slot).map(|s| (&*s.item, s.count)))),
        |item| {
            packs
                .jar(item)
                .is_some_and(|j| super::armory_lib_gear::on(game, &j.mod_id, j.setting.as_deref()))
        },
    );
    let Some(slot) = found else {
        return;
    };
    let Some(jar) = game
        .stack_in(slot)
        .and_then(|s| packs.jar(&s.item))
        .cloned()
    else {
        return;
    };
    let souls = p::souls_of(
        game.stack_in(slot)
            .and_then(|s| store::custom_int(s, SOULS_KEY)),
    );
    // A puff where the mob fell, if it is still there to say where.
    let fell = game.mobs.iter().find(|mob| mob.id == entity).map(|mob| {
        [
            mob.position.x,
            mob.position.y + f64::from(mob.height()) * 0.5,
            mob.position.z,
        ]
    });
    if let Some(at) = fell {
        game.native_send_particles_in_place(
            &ParticleOptions::named(SOUL),
            None,
            at,
            [0.2, 0.2, 0.2],
            0.02,
            4,
        );
    }
    match p::add_soul(souls, jar.capacity) {
        p::Soul::Count(n) => {
            if let Some(stack) = game.stack_in_mut(slot).as_mut() {
                let n = i32::try_from(n).unwrap_or(i32::MAX);
                store::set_custom(stack, SOULS_KEY, Some(Tag::Int(n)));
            }
        }
        p::Soul::Full => {
            *game.stack_in_mut(slot) = Some(ItemStack::new(&jar.full, 1));
            let at = me.position;
            game.native_play_sound_in_place(FULL_SOUND, at, PLAYERS, 1.0, 1.0);
            game.native_send_particles_in_place(
                &ParticleOptions::named(SOUL),
                None,
                [at[0], at[1] + 1.0, at[2]],
                [0.4, 0.5, 0.4],
                0.03,
                12,
            );
        }
    }
}

/// The jar rule of `stack` and the souls it holds: a filling jar's from its custom data, a
/// full jar's its capacity. None for anything else, or a rule not in force.
fn jar_souls(game: &Game<'_>, stack: &ItemStack) -> Option<(u32, u32)> {
    let packs = p::packs();
    let (jar, souls) = match packs.jar(&stack.item) {
        Some(jar) => (jar, p::souls_of(store::custom_int(stack, SOULS_KEY))),
        None => {
            let jar = packs.full_jar(&stack.item)?;
            (jar, jar.capacity)
        }
    };
    super::armory_lib_gear::on(game, &jar.mod_id, jar.setting.as_deref())
        .then_some((souls, jar.capacity))
}

/// `native_item_bar` (client): a jar with souls shows how full it is.
pub(super) fn on_item_bar(game: &Game<'_>, stack: &ItemStack, bar: &mut Option<(i32, u32)>) {
    if p::packs().jar(&stack.item).is_none() {
        return;
    }
    if let Some((souls, capacity)) = jar_souls(game, stack)
        && souls > 0
    {
        *bar = Some((p::bar_width(souls, capacity), SOUL_RGB));
    }
}

/// `native_tooltip` (client): a jar's line `Souls: 12 / 50`, under its name.
pub(super) fn on_tooltip(game: &Game<'_>, stack: &ItemStack, lines: &mut Vec<(Text, u32)>) {
    let Some((souls, capacity)) = jar_souls(game, stack) else {
        return;
    };
    let line = Text::translate_with_fallback(
        "armory_lib.soul_jar.souls",
        "Souls: %s / %s",
        &[
            Text::literal(souls.to_string()),
            Text::literal(capacity.to_string()),
        ],
    )
    .with_color(text::GRAY);
    lines.insert(lines.len().min(1), (line, u32::MAX));
}
