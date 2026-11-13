// Trinket slots in the game, added as the module `game::armory_lib_trinkets`: a row of slots
// of the player's own for the items content mods declare as trinkets, like a second off hand.
// What the mods declare, and the sums, are in `trinkets_logic.rs`.
//
// - The client: the trinkets key (G unless a mod names another), pressed while playing or in
//   the player's own inventory, sends `/trinkets`.
// - The server: `/trinkets` opens a one-row chest titled "Trinkets" whose middle slots are the
//   player's trinket slots. They are kept in `<world>/armory_lib/trinkets/<uuid>.dat`, apart
//   from the player's own file, so a death leaves them where they are, as an ender chest's
//   items. While the menu is open its slots hold the items and the file is empty; when it
//   closes, for whatever reason, the trinkets go back into the file and the rest back to the
//   player, so an item is always in one place.
// - Each player tick: the trinkets' attribute modifiers on the player, as transient modifiers
//   (never saved: given again after a respawn or a rejoin), and their experience bonus for the
//   orbs they pick up.
use crate::game::Game;
use super::armory_lib_store as store;
use super::armory_lib_trinkets_logic as t;
use horizon_client::armory_lib_bridge as bridge;
use horizon_client::items::ItemStack;
use horizon_client::mod_api::{NativeCommand, NativeScreenView, NativeServerPlayer};
use horizon_client::motion::PlayerMotion;
use horizon_client::text::Text;
use horizon_world::attributes::{Modifier, Operation};
use horizon_world::nbt::Tag;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic in another hook must not take the trinkets down with it.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

const TITLE: &str = "Trinkets";
/// The panes that fill the row around the slots.
const PANE: &str = "minecraft:gray_stained_glass_pane";
const MAX_HEALTH: &str = "minecraft:max_health";
/// `NativeServerPlayer.game_mode` of a spectator, who opens no menus.
const SPECTATOR: i32 = 3;
/// A player not ticked for this long (gone, dead) gets no experience bonus.
const FRESH: Duration = Duration::from_secs(2);
/// Most players' slots kept in memory; past it, those with no menu open are read again.
const MAX_KEPT: usize = 4096;

/// Whether the slots are offered: Armory Lib is on, a mod declares them, and one that does is
/// in force (its mod on in Options > Mods, and its switch if it names one).
fn offered(game: &Game<'_>, all: &t::Trinkets) -> bool {
    all.slots > 0
        && game.native_on(super::armory_lib_logic::ID)
        && all
            .declared_by
            .iter()
            .any(|(mod_id, setting)| super::armory_lib_gear::on(game, mod_id, setting.as_deref()))
}

// ---- The client: the key ---------------------------------------------------------------

/// A press of the trinkets key in the player's inventory, waiting for the next client tick.
static PRESSED: AtomicBool = AtomicBool::new(false);

fn send(game: &mut Game<'_>) {
    if let Err(e) = game.native_send_chat(&format!("/{}", t::COMMAND)) {
        eprintln!("Armory Lib: trinkets: /{} not sent: {e}", t::COMMAND);
    }
}

/// `native_key` (the window's): the trinkets key pressed with no screen open sends
/// `/trinkets`. The hook hears every key on every screen (chat too), and releases; and the
/// prompts and server dialogs drawn over gameplay leave it "playing", so they are asked too.
pub(super) fn on_key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_playing() || game.prompt_open() {
        return;
    }
    let all = t::trinkets();
    if key == all.key && offered(game, &all) {
        send(game);
    }
}

/// `native_screen_key_pressed_pre` (the window's, `&self` only): the trinkets key in the
/// player's own inventory (the `inventory` screen with no menu: not a chest's, a station's or
/// a mount's) goes no further and is sent at the next client tick. Not while a search box
/// there has the focus: the key is a letter then.
pub(super) fn on_screen_key(
    game: &Game<'_>,
    screen: &NativeScreenView<'_>,
    key: &str,
    repeat: bool,
    canceled: &mut bool,
) {
    if *canceled || repeat || screen.id != "inventory" || screen.menu.is_some() {
        return;
    }
    if game.recipe_search_focused() || game.mod_browser_typing() {
        return;
    }
    let all = t::trinkets();
    if key == all.key && offered(game, &all) {
        *canceled = true;
        PRESSED.store(true, Ordering::Release);
    }
}

/// `native_client_tick_pre`: sends the `/trinkets` a press in the inventory queued. The
/// server's menu then takes the inventory's place.
pub(super) fn on_client_tick(game: &mut Game<'_>) {
    if PRESSED.swap(false, Ordering::AcqRel) {
        send(game);
    }
}

// ---- The server: the command and its menu ----------------------------------------------

/// A trinkets menu open on the server: its window, the player's connection then, their file.
struct Open {
    uuid: u128,
    window: i32,
    conn: u32,
    path: PathBuf,
}

static OPEN: Mutex<Vec<Open>> = Mutex::new(Vec::new());

/// What each player's slots hold (position, item id), by file: what the file holds, or while
/// the menu is open what it held as it opened. Read from the file the first time it's needed.
static HELD: Mutex<BTreeMap<PathBuf, Arc<[(usize, String)]>>> = Mutex::new(BTreeMap::new());

fn file_of(game: &Game<'_>, uuid: u128) -> Option<PathBuf> {
    store::path(game, &t::file_name(uuid))
}

/// Notes what the slots at `path` hold now.
fn remember(path: &Path, stacks: &[Option<ItemStack>]) {
    let held: Arc<[(usize, String)]> = stacks
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.as_ref().filter(|s| s.count > 0).map(|s| (i, s.item.to_string())))
        .collect();
    let mut all = lock(&HELD);
    if all.len() >= MAX_KEPT {
        let open = lock(&OPEN);
        all.retain(|p, _| open.iter().any(|o| o.path == *p));
    }
    all.insert(path.to_owned(), held);
}

/// What the player's slots hold, read from their file the first time.
fn held(game: &Game<'_>, uuid: u128) -> Arc<[(usize, String)]> {
    let Some(path) = file_of(game, uuid) else {
        return Arc::from([]);
    };
    if let Some(held) = lock(&HELD).get(&path) {
        return held.clone();
    }
    remember(&path, &store::read_items(&path, t::ROW));
    lock(&HELD).get(&path).cloned().unwrap_or_else(|| Arc::from([]))
}

/// What the file at `path` keeps, by position; None when a file is there that can't be read,
/// which is then left as it is.
fn stored(path: &Path) -> Option<Vec<Option<ItemStack>>> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(vec![None; t::ROW]),
        Err(_) => return None,
        Ok(_) => {
            horizon_client::save::read_dat(path)?;
        }
    }
    Some(
        store::read_items(path, t::ROW)
            .into_iter()
            .map(|s| s.filter(|s| s.count > 0))
            .collect(),
    )
}

/// A pane with a blank name: the row's slots that aren't trinket slots.
fn pane() -> ItemStack {
    let mut pane = ItemStack::new(PANE, 1);
    pane.data_mut()
        .set_other("minecraft:custom_name", Some(Tag::String(" ".into())));
    pane
}

/// `native_register_commands`: `/trinkets`, for every player, when a mod declares the slots
/// (the files are read as the world opens, before its players come).
pub(super) fn on_register_commands(commands: &mut Vec<NativeCommand>) {
    let taken = commands
        .iter()
        .any(|c| c.usage.split(' ').next() == Some(t::COMMAND));
    if t::trinkets().slots > 0 && !taken {
        commands.push(NativeCommand::new(t::COMMAND));
    }
}

/// `native_perform_command`, with the player swapped in: `/trinkets` opens their trinket
/// slots. While the slots aren't offered it isn't run, and answers as an unknown command.
pub(super) fn on_perform_command(
    game: &mut Game<'_>,
    player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    if *ran || command.trim() != t::COMMAND || !game.native_authoritative() {
        return;
    }
    let all = t::trinkets();
    if !offered(game, &all) {
        return;
    }
    *ran = true;
    if let Err(why) = open(game, player, &all) {
        // As a vanilla command's failure reads: in red.
        game.native_feedback(Text::literal(why).with_color(horizon_client::text::RED));
    }
}

/// Opens the player's trinkets: their file's items in the middle of the row, panes around.
fn open(game: &mut Game<'_>, player: &NativeServerPlayer, all: &t::Trinkets) -> Result<(), &'static str> {
    let uuid = player.uuid;
    if player.health <= 0.0 || player.game_mode == SPECTATOR {
        return Err("You can't open your trinkets now.");
    }
    let path = file_of(game, uuid).ok_or("Your trinkets need a world to be kept in.")?;
    let conn = game.native_conn(uuid).ok_or("You can't open your trinkets now.")?;
    {
        let mut open = lock(&OPEN);
        if let Some(i) = open.iter().position(|o| o.uuid == uuid) {
            // Its items reach the file only when the hook hears it closed.
            if open[i].conn == conn {
                return Err("Your trinkets are already open.");
            }
            // Left from an earlier connection, whose menu went when it did.
            open.remove(i);
        }
    }
    let Some(stacks) = stored(&path) else {
        eprintln!("Armory Lib: trinkets: {} can't be read; it is left as it is", path.display());
        return Err("Your trinkets can't be read.");
    };
    let reach = stacks.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
    let input = t::input_slots(all.slots, reach);
    // From here until the menu closes its slots hold the items, so the file is emptied first:
    // whatever closes the menu (the server stopping too) gives them to the close hook.
    if let Err(e) = store::write_items(&path, &[]) {
        eprintln!("Armory Lib: trinkets: {e}");
        return Err("Your trinkets can't be opened: their file can't be written.");
    }
    // The slots keep working while the menu is open: what they held as it opened.
    remember(&path, &stacks);
    let items: Vec<Option<ItemStack>> = (0..t::ROW)
        .map(|k| match k.checked_sub(input.start) {
            Some(i) if input.contains(&k) => stacks.get(i).cloned().flatten(),
            _ => Some(pane()),
        })
        .collect();
    match game.native_open_menu_with_input(uuid, 1, Text::literal(TITLE), items, input) {
        Some(window) => {
            lock(&OPEN).push(Open { uuid, window, conn, path });
            Ok(())
        }
        None => {
            put_back(game, player, &path, stacks);
            Err("You can't open your trinkets now.")
        }
    }
}

/// The menu didn't open: the items go back into the file, or, if it can't be written, to the
/// player (at their feet what doesn't fit).
fn put_back(game: &mut Game<'_>, player: &NativeServerPlayer, path: &Path, stacks: Vec<Option<ItemStack>>) {
    let Err(e) = store::write_items(path, &stacks) else {
        return;
    };
    eprintln!("Armory Lib: trinkets: {e}; they are given back");
    remember(path, &[]);
    let at = player.position.map(|c| c.floor() as i32);
    for stack in stacks.into_iter().flatten() {
        if let Some(left) = game.native_give_stack(player.uuid, stack) {
            game.pop_resource(at, left);
        }
    }
}

/// `native_menu_input_closed` (no player swapped in, the hub maybe out of reach: only `slots`
/// answers). A trinkets menu closed, whatever closed it: Escape, death, leaving, another menu,
/// the server stopping. One of each trinket in its slots is kept (one of a stack; no more
/// than there are trinket slots) and written to the player's file before it leaves `slots`;
/// what stays there goes back to the player. When the file can't be written nothing is kept.
/// It runs whether or not the library is on, so a menu open as it is turned off loses nothing.
pub(super) fn on_menu_closed(uuid: u128, window: i32, slots: &mut [Option<ItemStack>]) {
    let open = {
        let mut open = lock(&OPEN);
        let Some(i) = open.iter().position(|o| o.uuid == uuid && o.window == window) else {
            return;
        };
        open.remove(i)
    };
    let all = t::trinkets();
    let items: Vec<Option<&str>> = slots
        .iter()
        .map(|s| s.as_ref().filter(|s| s.count > 0).map(|s| &*s.item))
        .collect();
    let plan = t::keep_plan(&items, all.slots, |item| all.is_trinket(item));
    if plan.is_empty() {
        // The file was emptied as the menu opened.
        remember(&open.path, &[]);
        return;
    }
    let mut kept: Vec<Option<ItemStack>> = vec![None; all.slots];
    for &(from, to) in &plan {
        kept[to] = slots[from].clone().map(|s| ItemStack { count: 1, ..s });
    }
    if let Err(e) = store::write_items(&open.path, &kept) {
        eprintln!("Armory Lib: trinkets: {e}; they go back to the player");
        remember(&open.path, &[]);
        note(uuid, "Your trinkets couldn't be saved, so they went back to your inventory.");
        return;
    }
    for &(from, _) in &plan {
        match slots[from].as_mut() {
            Some(s) if s.count > 1 => s.count -= 1,
            _ => slots[from] = None,
        }
    }
    remember(&open.path, &kept);
}

/// Lines for players, told at their next tick (the close hook can't reach them).
static NOTES: Mutex<Vec<(u128, &'static str)>> = Mutex::new(Vec::new());

fn note(uuid: u128, line: &'static str) {
    let mut notes = lock(&NOTES);
    if notes.len() < 64 {
        notes.push((uuid, line));
    }
}

fn take_notes(uuid: u128) -> Vec<&'static str> {
    let mut notes = lock(&NOTES);
    if notes.is_empty() {
        return Vec::new();
    }
    let mine = notes.iter().filter(|(u, _)| *u == uuid).map(|(_, l)| *l).collect();
    notes.retain(|(u, _)| *u != uuid);
    mine
}

// ---- What trinkets do ------------------------------------------------------------------

/// A player the trinkets work on: the modifiers put on them (attribute and id), their
/// experience bonus and its fraction carried over, and when their tick last ran.
struct Worn {
    added: Vec<(String, String)>,
    bonus: f32,
    carry: f32,
    seen: Instant,
}

static WORN: Mutex<BTreeMap<u128, Worn>> = Mutex::new(BTreeMap::new());

/// `native_player_tick`, with the player swapped in: the modifiers of the trinkets in their
/// slots (a copy counting once, none for one their off hand holds too) are put on them, those
/// that no longer apply taken off; and their experience bonus is kept for the orbs.
pub(super) fn on_player_tick(game: &mut Game<'_>) {
    if !game.native_authoritative() {
        return;
    }
    let Some(uuid) = game.lan_acting_uuid else {
        return;
    };
    for line in take_notes(uuid) {
        let _ = game.native_tell(uuid, Text::literal(line));
    }
    let all = t::trinkets();
    let (applied, bonus) = if !all.defs.is_empty() && offered(game, &all) {
        let held = held(game, uuid);
        let slots: Vec<&str> = held
            .iter()
            .filter(|(at, _)| *at < all.slots)
            .map(|(_, item)| item.as_str())
            .collect();
        let off_hand = game.offhand.as_ref().filter(|s| s.count > 0).map(|s| &*s.item);
        let on = |def: &t::TrinketDef| super::armory_lib_gear::on(game, &def.mod_id, def.setting.as_deref());
        (all.applied(&slots, off_hand, &on), all.experience_bonus(&slots, off_hand, &on))
    } else {
        (Vec::new(), 0.0)
    };
    let before = {
        let mut worn = lock(&WORN);
        if worn.len() > 256 {
            worn.retain(|u, w| *u == uuid || w.seen.elapsed() < FRESH * 30);
        }
        let w = worn.entry(uuid).or_insert_with(|| Worn {
            added: Vec::new(),
            bonus: 0.0,
            carry: 0.0,
            seen: Instant::now(),
        });
        w.bonus = bonus;
        w.seen = Instant::now();
        if applied.is_empty() && w.added.is_empty() {
            return;
        }
        std::mem::take(&mut w.added)
    };
    let Some(body) = game.player.as_mut() else {
        if let Some(w) = lock(&WORN).get_mut(&uuid) {
            w.added = before;
        }
        return;
    };
    let mut health = false;
    let mut added = Vec::new();
    for a in &applied {
        match give(body, a) {
            Some(changed) => {
                health |= changed && a.attribute == MAX_HEALTH;
                added.push((a.attribute.clone(), a.id.clone()));
            }
            None => unknown(&a.attribute),
        }
    }
    for (attribute, id) in &before {
        let still = added.iter().any(|(a, i)| a == attribute && i == id);
        if !still && take(body, attribute, id) {
            health |= attribute == MAX_HEALTH;
        }
    }
    // `LivingEntity.onAttributeUpdated`: a lower maximum caps the health.
    if health {
        let _ = body.clamp_health_to_max();
    }
    if let Some(w) = lock(&WORN).get_mut(&uuid) {
        w.added = added;
    }
}

fn operation(op: t::Operation) -> Operation {
    match op {
        t::Operation::AddValue => Operation::AddValue,
        t::Operation::AddMultipliedBase => Operation::AddMultipliedBase,
        t::Operation::AddMultipliedTotal => Operation::AddMultipliedTotal,
    }
}

/// Puts modifier `a` on the body as a transient one, as `/attribute .. modifier add` edits it,
/// so the change is synced to the player's client: whether it changed anything (it is left
/// alone while it is already there as it should be). None for an attribute players don't
/// have.
fn give(body: &mut PlayerMotion, a: &t::Applied) -> Option<bool> {
    let key = body.attributes.key(&a.attribute)?;
    let handle = body.attributes.get_instance(Some(key)).ok().flatten()?;
    let op = operation(a.operation);
    let there = body.attributes.instance(&handle).ok()?.modifier(&a.id).cloned();
    if there.is_some_and(|m| m.amount() == a.amount && m.operation() == op) {
        return Some(false);
    }
    let modifier = Modifier::new(&a.id, a.amount, op).ok()?;
    body.attributes
        .edit(&handle, |value, observer| {
            // Removed first: vanilla's add-or-update keeps an old operation's entry.
            value.remove(modifier.id(), observer)?;
            value.add_transient(modifier, observer)
        })
        .ok()?;
    Some(true)
}

/// Takes the library's modifier `id` off the body's `attribute`: whether it was there.
fn take(body: &mut PlayerMotion, attribute: &str, id: &str) -> bool {
    let Some(key) = body.attributes.key(attribute) else {
        return false;
    };
    let Some(handle) = body.attributes.lookup(Some(&key)) else {
        return false;
    };
    body.attributes
        .edit(&handle, |value, observer| value.remove(id, observer))
        .unwrap_or(false)
}

/// Logs an attribute players don't have, once.
fn unknown(attribute: &str) {
    static TOLD: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut told = lock(&TOLD);
    if !told.iter().any(|a| a == attribute) {
        eprintln!("Armory Lib: trinkets: players have no attribute {attribute}");
        told.push(attribute.to_owned());
    }
}

/// `native_player_exp_change` (the player not swapped in): an orb's points are worth more by
/// the experience bonus of the player's trinkets, as their last tick found it.
pub(super) fn on_experience(player: u128, amount: &mut i32) {
    if *amount <= 0 {
        return;
    }
    let mut worn = lock(&WORN);
    let Some(w) = worn.get_mut(&player).filter(|w| w.seen.elapsed() < FRESH) else {
        return;
    };
    let bonus = bridge::experience_bonus(*amount, w.bonus, &mut w.carry);
    *amount = amount.saturating_add(bonus);
}
