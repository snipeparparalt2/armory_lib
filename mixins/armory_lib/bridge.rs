// Armory Lib's half inside the game's client library (added there as the public module
// `armory_lib_bridge`, so the `[api]` package reaches it as `horizon_client::armory_lib_bridge`
// and the mixin code in `gear.rs` as the same path). It holds what both sides share, with no
// game in it:
//
// - the entity types daylight must not set alight (`sunburn.rs` reads it);
// - the gear rules mods declare in their `data/<mod>/armory_lib.json`: armour set bonuses,
//   tool abilities, harvest sweeps and weapon effects, and the logic `gear.rs` runs them with;
// - the settings switches as the server's hooks last read them.
//
// It is also a test target of the repository's Cargo package: `cargo test` runs it alone.
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A vanilla id without its namespace: `minecraft:zombie` and `zombie` -> `zombie`.
pub fn vanilla(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

// ---------------------------------------------------------------------------------------
// Daylight
// ---------------------------------------------------------------------------------------

static NO_SUNBURN: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Replaces the list of entity types (`mymod:shade`) that daylight doesn't light.
pub fn set_no_sunburn(kinds: Vec<String>) {
    let mut list = NO_SUNBURN.write().unwrap_or_else(|e| e.into_inner());
    if *list != kinds {
        *list = kinds;
    }
}

/// Whether daylight leaves entity type `kind` unlit.
pub fn no_sunburn(kind: &str) -> bool {
    let list = NO_SUNBURN.read().unwrap_or_else(|e| e.into_inner());
    list.iter().any(|k| k == kind)
}

// ---------------------------------------------------------------------------------------
// Content mods
// ---------------------------------------------------------------------------------------

/// The ids of the content mods in `dir` (the instance's `horizon-mods/`), as the game finds
/// them: each `.hmod` and each mod folder.
#[cfg(not(test))]
pub fn content_mod_ids(dir: &std::path::Path) -> Vec<String> {
    let mut ids: Vec<String> = horizon_mods::find(dir)
        .iter()
        .filter_map(|path| horizon_mods::Package::read(path).ok())
        .map(|package| package.manifest.id)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

// ---------------------------------------------------------------------------------------
// Switches. A rule may name a switch of its mod's settings page; the page is drawn by the
// `[api]` package and kept by the game in one file per switch. The server's hooks read the
// files through here, at most once a second.
// ---------------------------------------------------------------------------------------

/// A stored switch: on unless it says `false`.
pub fn read_flag(stored: Option<String>) -> bool {
    stored.is_none_or(|v| v.trim() != "false")
}

const FRESH_MS: u64 = 1000;
const MAX_SWITCHES: usize = 512;

/// (mod, key, on, read at).
static SWITCHES: Mutex<Vec<(String, String, bool, u64)>> = Mutex::new(Vec::new());
/// The mods switched off in Options > Mods, as the last hook with a game saw them.
static MODS_OFF: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Milliseconds since the first call.
pub fn now_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// Whether switch `key` of mod `mod_id` is on at `now` (see [`now_ms`]): the kept answer
/// while it is under a second old, else what `read` finds in its file. No key is always on.
pub fn switch_on(
    mod_id: &str,
    key: Option<&str>,
    now: u64,
    read: impl FnOnce(&str, &str) -> Option<String>,
) -> bool {
    let Some(key) = key else {
        return true;
    };
    let mut all = lock(&SWITCHES);
    let at = all.iter().position(|(m, k, _, _)| m == mod_id && k == key);
    if let Some(i) = at
        && now.saturating_sub(all[i].3) < FRESH_MS
    {
        return all[i].2;
    }
    let on = read_flag(read(mod_id, key));
    match at {
        Some(i) => (all[i].2, all[i].3) = (on, now),
        None => {
            if all.len() >= MAX_SWITCHES {
                all.remove(0);
            }
            all.push((mod_id.to_owned(), key.to_owned(), on, now));
        }
    }
    on
}

/// The switch as it was last read, for the hooks with no game to read files with: on until
/// a hook with a game has read it.
pub fn switch_on_now(mod_id: &str, key: Option<&str>) -> bool {
    let Some(key) = key else {
        return true;
    };
    lock(&SWITCHES)
        .iter()
        .find(|(m, k, _, _)| m == mod_id && k == key)
        .is_none_or(|(_, _, on, _)| *on)
}

/// Keeps whether mod `mod_id` is on in Options > Mods, for the hooks with no game.
pub fn note_mod_on(mod_id: &str, on: bool) {
    let mut off = lock(&MODS_OFF);
    let at = off.iter().position(|m| m == mod_id);
    match (on, at) {
        (true, Some(i)) => {
            off.remove(i);
        }
        (false, None) => off.push(mod_id.to_owned()),
        _ => {}
    }
}

pub fn mod_on(mod_id: &str) -> bool {
    !lock(&MODS_OFF).iter().any(|m| m == mod_id)
}

// ---------------------------------------------------------------------------------------
// Gear rules, as mods declare them.
// ---------------------------------------------------------------------------------------

/// A full armour set and what wearing all of it does.
#[derive(Clone, Debug, PartialEq)]
pub struct SetDef {
    pub mod_id: String,
    pub setting: Option<String>,
    /// The item ids of its pieces; every one must be worn.
    pub pieces: Vec<String>,
    /// Effects kept on the wearer: id and amplifier (0 for level I).
    pub effects: Vec<(String, u8)>,
    pub no_fall_damage: bool,
    /// Health given back every `regen_every` player ticks; 0 for none.
    pub regen_every: u64,
    pub regen_health: f32,
    /// The share of a knockback that still pushes (1 for all of it).
    pub knockback_taken: f64,
    /// The share of an explosion's damage that still hurts.
    pub explosion_taken: f32,
    /// Experience picked up is worth this much more (0.25 for a quarter).
    pub experience_bonus: f32,
}

impl Default for SetDef {
    fn default() -> Self {
        Self {
            mod_id: String::new(),
            setting: None,
            pieces: Vec::new(),
            effects: Vec::new(),
            no_fall_damage: false,
            regen_every: 0,
            regen_health: 0.0,
            knockback_taken: 1.0,
            explosion_taken: 1.0,
            experience_bonus: 0.0,
        }
    }
}

/// What a tool does to the blocks around the one broken, while its holder sneaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ability {
    /// Breaks the ore blocks of the same kind connected to a broken ore.
    VeinMine,
    /// Breaks the 3 x 3 square around the broken block, facing the player.
    WideDig,
    /// Breaks the logs connected to and above a broken log.
    FellTree,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub items: Vec<String>,
    pub ability: Ability,
}

/// Hoes that harvest and replant the grown crops around the one clicked.
#[derive(Clone, Debug, PartialEq)]
pub struct HarvestDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub items: Vec<String>,
    /// 1 for the 3 x 3 square, 2 for 5 x 5.
    pub radius: i32,
}

/// What a weapon does beyond its damage.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub items: Vec<String>,
    /// A hit from behind the target is multiplied by this (1 for no bonus).
    pub backstab: f32,
    /// A hit also hurts the other mobs this close to its target (blocks; 0 for none)...
    pub splash_radius: f64,
    /// ... for this share of the hit.
    pub splash_share: f32,
    /// The target burns for this many ticks (0 for none).
    pub fire_ticks: i32,
    /// Health its wielder gets back for a kill.
    pub heal_on_kill: f32,
    /// Entity types it hits harder, and by how much.
    pub bonus_against: Vec<String>,
    pub bonus_against_by: f32,
    /// A dimension it hits harder in, and by how much.
    pub bonus_in: Option<String>,
    pub bonus_in_by: f32,
    /// Its kills may drop the mob's head.
    pub heads: bool,
}

impl Default for WeaponDef {
    fn default() -> Self {
        Self {
            mod_id: String::new(),
            setting: None,
            items: Vec::new(),
            backstab: 1.0,
            splash_radius: 0.0,
            splash_share: 0.0,
            fire_ticks: 0,
            heal_on_kill: 0.0,
            bonus_against: Vec::new(),
            bonus_against_by: 1.0,
            bonus_in: None,
            bonus_in_by: 1.0,
            heads: false,
        }
    }
}

/// Every mod's gear rules together.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gear {
    pub sets: Vec<Arc<SetDef>>,
    pub tools: Vec<ToolDef>,
    pub harvests: Vec<HarvestDef>,
    pub weapons: Vec<Arc<WeaponDef>>,
    /// Blocks that count as ores though no ore tag lists them.
    pub ores: Vec<String>,
}

static GEAR: RwLock<Option<Arc<Gear>>> = RwLock::new(None);

/// Replaces the gear rules (the `[api]` package, after it read the mods' files).
pub fn set_gear(gear: Gear) {
    *GEAR.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(gear));
}

/// The gear rules in force.
pub fn gear() -> Arc<Gear> {
    GEAR.read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

impl Gear {
    /// The set whose every piece is among `worn` (head, chest, legs, feet), if one is whole.
    pub fn set_of(&self, worn: [Option<&str>; 4]) -> Option<Arc<SetDef>> {
        self.sets
            .iter()
            .find(|set| {
                !set.pieces.is_empty()
                    && set.pieces.iter().all(|p| worn.contains(&Some(p.as_str())))
            })
            .cloned()
    }
    pub fn tool(&self, item: &str) -> Option<&ToolDef> {
        self.tools.iter().find(|t| t.items.iter().any(|i| i == item))
    }
    pub fn harvest(&self, item: &str) -> Option<&HarvestDef> {
        self.harvests.iter().find(|h| h.items.iter().any(|i| i == item))
    }
    pub fn weapon(&self, item: &str) -> Option<Arc<WeaponDef>> {
        self.weapons.iter().find(|w| w.items.iter().any(|i| i == item)).cloned()
    }
    pub fn is_listed_ore(&self, block: &str) -> bool {
        self.ores.iter().any(|o| o == block)
    }
}

// ---- Reading a mod's file --------------------------------------------------------------

fn strings(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

fn number(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|v| v.as_f64())
}

fn entries<'a>(file: &'a serde_json::Value, key: &str) -> impl Iterator<Item = &'a serde_json::Value> {
    file.get(key).and_then(|v| v.as_array()).into_iter().flatten()
}

/// Adds the gear rules of mod `mod_id`'s file (`armor_sets`, `tools`, `ores`, `harvest`,
/// `weapons`; the README has the format) to `gear`. Returns what it could not use, one line
/// each, for the log.
pub fn read_gear(mod_id: &str, file: &serde_json::Value, gear: &mut Gear) -> Vec<String> {
    let mut problems = Vec::new();
    for set in entries(file, "armor_sets") {
        let pieces = strings(set.get("pieces"));
        if pieces.is_empty() {
            problems.push(format!("{mod_id}: an armor_sets entry has no pieces"));
            continue;
        }
        let effects = entries(set, "effects")
            .filter_map(|e| match e {
                serde_json::Value::String(id) => Some((id.clone(), 0)),
                other => Some((
                    text(other, "effect")?,
                    number(other, "amplifier").unwrap_or(0.0).clamp(0.0, 255.0) as u8,
                )),
            })
            .collect();
        let regen = set.get("regen");
        gear.sets.push(Arc::new(SetDef {
            mod_id: mod_id.to_owned(),
            setting: text(set, "setting"),
            pieces,
            effects,
            no_fall_damage: set.get("no_fall_damage").and_then(|v| v.as_bool()).unwrap_or(false),
            regen_every: regen
                .and_then(|r| number(r, "every_ticks"))
                .map_or(0, |t| t.max(0.0) as u64),
            regen_health: regen.and_then(|r| number(r, "health")).unwrap_or(1.0) as f32,
            knockback_taken: number(set, "knockback_taken").unwrap_or(1.0).clamp(0.0, 1.0),
            explosion_taken: number(set, "explosion_taken").unwrap_or(1.0).clamp(0.0, 1.0) as f32,
            experience_bonus: number(set, "experience_bonus").unwrap_or(0.0).clamp(0.0, 10.0) as f32,
        }));
    }
    for tool in entries(file, "tools") {
        let ability = match tool.get("ability").and_then(|v| v.as_str()) {
            Some("vein_mine") => Ability::VeinMine,
            Some("wide_dig") => Ability::WideDig,
            Some("fell_tree") => Ability::FellTree,
            other => {
                problems.push(format!("{mod_id}: unknown tool ability {other:?}"));
                continue;
            }
        };
        gear.tools.push(ToolDef {
            mod_id: mod_id.to_owned(),
            setting: text(tool, "setting"),
            items: strings(tool.get("items")),
            ability,
        });
    }
    gear.ores.extend(strings(file.get("ores")));
    for hoe in entries(file, "harvest") {
        gear.harvests.push(HarvestDef {
            mod_id: mod_id.to_owned(),
            setting: text(hoe, "setting"),
            items: strings(hoe.get("items")),
            radius: number(hoe, "radius").unwrap_or(1.0).clamp(1.0, 4.0) as i32,
        });
    }
    for weapon in entries(file, "weapons") {
        let splash = weapon.get("splash");
        let against = weapon.get("bonus_against");
        let within = weapon.get("bonus_in");
        gear.weapons.push(Arc::new(WeaponDef {
            mod_id: mod_id.to_owned(),
            setting: text(weapon, "setting"),
            items: strings(weapon.get("items")),
            backstab: number(weapon, "backstab").unwrap_or(1.0).clamp(0.0, 100.0) as f32,
            splash_radius: splash.and_then(|s| number(s, "radius")).unwrap_or(0.0).clamp(0.0, 8.0),
            splash_share: splash.and_then(|s| number(s, "share")).unwrap_or(0.5).clamp(0.0, 1.0) as f32,
            fire_ticks: number(weapon, "fire_ticks").unwrap_or(0.0).clamp(0.0, 12000.0) as i32,
            heal_on_kill: number(weapon, "heal_on_kill").unwrap_or(0.0).clamp(0.0, 1024.0) as f32,
            bonus_against: against.map(|a| strings(a.get("kinds"))).unwrap_or_default(),
            bonus_against_by: against.and_then(|a| number(a, "multiplier")).unwrap_or(1.0).clamp(0.0, 100.0) as f32,
            bonus_in: within.and_then(|w| text(w, "dimension")),
            bonus_in_by: within.and_then(|w| number(w, "multiplier")).unwrap_or(1.0).clamp(0.0, 100.0) as f32,
            heads: weapon.get("heads").and_then(|v| v.as_bool()).unwrap_or(false),
        }));
    }
    problems
}

// ---------------------------------------------------------------------------------------
// Armour set bonuses.
// ---------------------------------------------------------------------------------------

/// How often a set's effects are given again, in player ticks.
pub const EFFECT_REFRESH_TICKS: u64 = 20;
/// An effect's length each time: a little over a refresh, so it never runs out while the
/// set is worn and fades soon after it is taken off.
pub const EFFECT_TICKS: i32 = 60;
/// Night vision flickers in its last 10 seconds (200 ticks): keep it above that.
pub const NIGHT_VISION_TICKS: i32 = 260;

/// How long effect `id` is given for each time.
pub fn effect_ticks(id: &str) -> i32 {
    if vanilla(id) == "night_vision" {
        NIGHT_VISION_TICKS
    } else {
        EFFECT_TICKS
    }
}

/// What a set does to damage its wearer is about to take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DamageChange {
    Keep,
    Cancel,
    Scale(f32),
}

/// Damage types that count as falling (vanilla's `#minecraft:is_fall`).
const FALL: &[&str] = &["fall", "stalagmite", "ender_pearl"];
/// Damage types that count as explosions (vanilla's `#minecraft:is_explosion`).
const EXPLOSION: &[&str] = &[
    "explosion",
    "player_explosion",
    "bad_respawn_point",
    "fireworks",
    "fireball",
    "wither_skull",
];

pub fn incoming_damage(set: &SetDef, damage_type: &str) -> DamageChange {
    let kind = vanilla(damage_type);
    if set.no_fall_damage && FALL.contains(&kind) {
        DamageChange::Cancel
    } else if set.explosion_taken < 1.0 && EXPLOSION.contains(&kind) {
        DamageChange::Scale(set.explosion_taken)
    } else {
        DamageChange::Keep
    }
}

/// The extra experience points for `points` picked up with a bonus of `share`, carrying
/// the fractions in `carry` so the bonus is exact over time.
pub fn experience_bonus(points: i32, share: f32, carry: &mut f32) -> i32 {
    if points <= 0 || share <= 0.0 {
        return 0;
    }
    let total = points as f32 * share + *carry;
    let whole = total.floor();
    *carry = total - whole;
    whole as i32
}

/// A player as the server's player tick last saw them: what the hooks without a game (damage,
/// knockback) and the experience hook need to know about the wearer.
#[derive(Clone, Debug)]
pub struct Wearer {
    pub uuid: u128,
    pub entity: i32,
    pub set: Option<Arc<SetDef>>,
    /// Their player ticks counted since they were first seen.
    pub ticks: u64,
    pub xp_carry: f32,
    seen: std::time::Instant,
}

/// A wearer not seen for this long (left, died) is no longer trusted.
const WEARER_STALE: std::time::Duration = std::time::Duration::from_secs(2);
const MAX_WEARERS: usize = 256;

static WEARERS: Mutex<Vec<Wearer>> = Mutex::new(Vec::new());

/// The player tick of `uuid`: keeps their entity id and set, and counts their ticks.
/// Returns the set they had at their last tick and this tick's count.
pub fn note_wearer(
    uuid: u128,
    entity: i32,
    set: Option<Arc<SetDef>>,
) -> (Option<Arc<SetDef>>, u64) {
    let now = std::time::Instant::now();
    let mut all = lock(&WEARERS);
    all.retain(|w| w.uuid == uuid || now.duration_since(w.seen) < WEARER_STALE * 30);
    if let Some(w) = all.iter_mut().find(|w| w.uuid == uuid) {
        let before = if now.duration_since(w.seen) < WEARER_STALE {
            w.set.take()
        } else {
            None
        };
        w.entity = entity;
        w.set = set;
        w.ticks += 1;
        w.seen = now;
        return (before, w.ticks);
    }
    if all.len() >= MAX_WEARERS {
        all.remove(0);
    }
    all.push(Wearer {
        uuid,
        entity,
        set,
        ticks: 1,
        xp_carry: 0.0,
        seen: now,
    });
    (None, 1)
}

/// The set the player with entity id `entity` wears.
pub fn set_of_entity(entity: i32) -> Option<Arc<SetDef>> {
    if entity <= 0 {
        return None;
    }
    let now = std::time::Instant::now();
    lock(&WEARERS)
        .iter()
        .find(|w| w.entity == entity && now.duration_since(w.seen) < WEARER_STALE)
        .and_then(|w| w.set.clone())
}

/// Runs `f` on the player `uuid`'s record, if they were seen lately.
pub fn with_wearer<T>(uuid: u128, f: impl FnOnce(&mut Wearer) -> T) -> Option<T> {
    let now = std::time::Instant::now();
    lock(&WEARERS)
        .iter_mut()
        .find(|w| w.uuid == uuid && now.duration_since(w.seen) < WEARER_STALE)
        .map(f)
}

// ---------------------------------------------------------------------------------------
// Tool abilities.
// ---------------------------------------------------------------------------------------

/// Most extra blocks one break takes with each ability.
pub const VEIN_MAX: usize = 32;
pub const TREE_MAX: usize = 64;

/// The block tags that make a block an ore.
pub const ORE_TAGS: &[&str] = &[
    "c:ores",
    "minecraft:coal_ores",
    "minecraft:copper_ores",
    "minecraft:iron_ores",
    "minecraft:gold_ores",
    "minecraft:redstone_ores",
    "minecraft:lapis_ores",
    "minecraft:diamond_ores",
    "minecraft:emerald_ores",
];

/// An ore by its name, for when the tags can't be read.
pub fn looks_like_ore(block: &str) -> bool {
    block.ends_with("_ore")
}

/// A log by its name, for when the tags can't be read.
pub fn looks_like_log(block: &str) -> bool {
    ["_log", "_stem", "_wood", "_hyphae"]
        .iter()
        .any(|end| block.ends_with(end))
        && !block.contains("stripped_")
}

/// What makes two ores one vein: the same ore, deepslate or not (`minecraft:deepslate_iron_ore`
/// and `minecraft:iron_ore`; the mod's `aquiteore` and `deepslate_aquite_ore`).
pub fn vein_kind(block: &str) -> String {
    let (ns, path) = block.split_once(':').unwrap_or(("minecraft", block));
    let path = path.strip_prefix("deepslate_").unwrap_or(path);
    format!("{ns}:{}", path.replace('_', ""))
}

/// The 26 blocks around a block (faces, edges and corners).
pub fn around() -> impl Iterator<Item = [i32; 3]> {
    (-1..=1)
        .flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| [x, y, z])))
        .filter(|d| *d != [0, 0, 0])
}

/// The blocks connected to `origin` through its 26 neighbours that `accept` takes, nearest
/// first, at most `max`, without `origin` itself.
pub fn flood(
    origin: [i32; 3],
    max: usize,
    mut accept: impl FnMut([i32; 3]) -> bool,
) -> Vec<[i32; 3]> {
    let mut seen = std::collections::HashSet::from([origin]);
    let mut queue = std::collections::VecDeque::from([origin]);
    let mut found = Vec::new();
    while let Some(at) = queue.pop_front() {
        for d in around() {
            let p = [at[0] + d[0], at[1] + d[1], at[2] + d[2]];
            if !seen.insert(p) || !accept(p) {
                continue;
            }
            found.push(p);
            if found.len() >= max {
                return found;
            }
            queue.push_back(p);
        }
    }
    found
}

/// An axis of the world: the one a wide dig's square faces along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// The face of `block` that `point` (where the player's view meets it) lies on, as its axis.
pub fn hit_axis(block: [i32; 3], point: [f64; 3]) -> Option<Axis> {
    let mut best: Option<(f64, Axis)> = None;
    for (i, axis) in [Axis::X, Axis::Y, Axis::Z].into_iter().enumerate() {
        let f = point[i] - f64::from(block[i]);
        if !(-0.01..=1.01).contains(&f) {
            return None;
        }
        let off = f.abs().min((1.0 - f).abs());
        if best.is_none_or(|(b, _)| off < b) {
            best = Some((off, axis));
        }
    }
    best.filter(|(off, _)| *off < 0.01).map(|(_, axis)| axis)
}

/// The axis a player looks along (yaw 0 faces south, +Z; 90 west, -X): up or down past 45
/// degrees, else the nearer horizontal one.
pub fn look_axis(yaw: f32, pitch: f32) -> Axis {
    if pitch.abs() > 45.0 {
        return Axis::Y;
    }
    let r = yaw.to_radians();
    if r.sin().abs() > r.cos().abs() {
        Axis::X
    } else {
        Axis::Z
    }
}

/// The eight blocks of the 3 x 3 square around `origin`, across `axis`.
pub fn square(origin: [i32; 3], axis: Axis) -> Vec<[i32; 3]> {
    let mut out = Vec::with_capacity(8);
    for a in -1..=1 {
        for b in -1..=1 {
            if a == 0 && b == 0 {
                continue;
            }
            let [x, y, z] = origin;
            out.push(match axis {
                Axis::X => [x, y + a, z + b],
                Axis::Y => [x + a, y, z + b],
                Axis::Z => [x + a, y + b, z],
            });
        }
    }
    out
}

/// Whether a tool with `max` durability (0: it never wears), `damage` taken, can spend more
/// and still keep more than `keep` points.
pub fn spare_durability(max: u32, damage: u32, keep: u32) -> bool {
    max == 0 || max.saturating_sub(damage) > keep
}

/// While extra blocks are broken: their breaks call the break hook again, which must leave
/// them alone.
static BREAKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Marks extra blocks being broken until it is dropped; None when that is already so.
pub struct Breaking(());

impl Breaking {
    pub fn start() -> Option<Breaking> {
        (!BREAKING.swap(true, std::sync::atomic::Ordering::AcqRel)).then_some(Breaking(()))
    }
}

impl Drop for Breaking {
    fn drop(&mut self) {
        BREAKING.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Whether extra blocks are being broken now.
pub fn breaking() -> bool {
    BREAKING.load(std::sync::atomic::Ordering::Acquire)
}

// ---------------------------------------------------------------------------------------
// Harvest sweep.
// ---------------------------------------------------------------------------------------

/// A crop a hoe harvests: its block, its grown `age` and what is planted again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crop {
    pub block: &'static str,
    pub grown: u8,
    pub seed: &'static str,
}

const CROPS: &[Crop] = &[
    Crop {
        block: "minecraft:wheat",
        grown: 7,
        seed: "minecraft:wheat_seeds",
    },
    Crop {
        block: "minecraft:carrots",
        grown: 7,
        seed: "minecraft:carrot",
    },
    Crop {
        block: "minecraft:potatoes",
        grown: 7,
        seed: "minecraft:potato",
    },
    Crop {
        block: "minecraft:beetroots",
        grown: 3,
        seed: "minecraft:beetroot_seeds",
    },
    Crop {
        block: "minecraft:nether_wart",
        grown: 3,
        seed: "minecraft:nether_wart",
    },
];

pub fn crop(block: &str) -> Option<&'static Crop> {
    CROPS.iter().find(|c| c.block == block)
}

/// Whether a crop with the `age` property `age` is fully grown.
pub fn grown(crop: &Crop, age: Option<&str>) -> bool {
    age.and_then(|a| a.trim().parse::<u8>().ok())
        .is_some_and(|a| a >= crop.grown)
}

/// The blocks of the square of `radius` around `center` on its level, `center` first.
pub fn field(center: [i32; 3], radius: i32) -> Vec<[i32; 3]> {
    let mut out = vec![center];
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if dx != 0 || dz != 0 {
                out.push([center[0] + dx, center[1], center[2] + dz]);
            }
        }
    }
    out
}

/// Takes one `seed` out of a harvest's drops (item, count), for replanting: whether there was
/// one.
pub fn take_one<'a>(drops: impl IntoIterator<Item = (&'a str, &'a mut u32)>, seed: &str) -> bool {
    for (item, count) in drops {
        if item == seed && *count > 0 {
            *count -= 1;
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------------------
// Weapon effects.
// ---------------------------------------------------------------------------------------

/// Behind means within this many degrees of straight behind the target's facing.
pub const BACKSTAB_DEGREES: f64 = 60.0;

/// Whether the attacker at `attacker` stands behind a target at `target` that faces `view`:
/// within `BACKSTAB_DEGREES` of straight behind it, on the ground plane.
pub fn behind(target: [f64; 3], view: [f64; 3], attacker: [f64; 3]) -> bool {
    let (fx, fz) = (view[0], view[2]);
    let (ax, az) = (attacker[0] - target[0], attacker[2] - target[2]);
    let (lf, la) = ((fx * fx + fz * fz).sqrt(), (ax * ax + az * az).sqrt());
    if lf < 1e-6 || la < 1e-6 {
        return false;
    }
    // The angle between "behind the target" (-facing) and the way to the attacker.
    let cos = -(fx * ax + fz * az) / (lf * la);
    cos >= BACKSTAB_DEGREES.to_radians().cos() - 1e-9
}

/// What a hit by `weapon` on a mob of type `kind` in `dimension` is multiplied by.
pub fn hit_multiplier(weapon: &WeaponDef, kind: &str, dimension: &str, from_behind: bool) -> f32 {
    let mut by = 1.0;
    if from_behind {
        by *= weapon.backstab;
    }
    if weapon.bonus_against.iter().any(|k| vanilla(k) == vanilla(kind)) {
        by *= weapon.bonus_against_by;
    }
    if weapon.bonus_in.as_deref() == Some(dimension) {
        by *= weapon.bonus_in_by;
    }
    by
}

/// The head a kill by a weapon with `heads` may drop, and the chance.
pub fn head_of(kind: &str) -> Option<(&'static str, f32)> {
    match vanilla(kind) {
        "zombie" => Some(("minecraft:zombie_head", 0.10)),
        "skeleton" => Some(("minecraft:skeleton_skull", 0.10)),
        "creeper" => Some(("minecraft:creeper_head", 0.10)),
        "wither_skeleton" => Some(("minecraft:wither_skeleton_skull", 0.05)),
        _ => None,
    }
}

/// A greatsword's hit that landed: its splash is dealt in the attacker's next player tick,
/// once the hit is over.
#[derive(Clone, Debug, PartialEq)]
pub struct Splash {
    pub attacker: u128,
    pub victim: u32,
    pub at: [f64; 3],
    pub amount: f32,
    /// Blocks around the victim.
    pub radius: f64,
}

/// A few of each are ever waiting; more means something never picked them up.
const MAX_WAITING: usize = 64;

/// A greatsword hit being dealt: its target, its damage and where the target stands.
static HITS: Mutex<Vec<(u32, f32, [f64; 3])>> = Mutex::new(Vec::new());
static SPLASHES: Mutex<Vec<Splash>> = Mutex::new(Vec::new());
/// Mobs a reaper scythe killed whose head drops with their loot.
static HEADS: Mutex<Vec<(u32, &'static str)>> = Mutex::new(Vec::new());

fn push_capped<T>(list: &mut Vec<T>, item: T) {
    if list.len() >= MAX_WAITING {
        list.remove(0);
    }
    list.push(item);
}

/// The damage hook: a greatsword is about to hit `victim` at `at` for `amount`.
pub fn note_hit(victim: u32, amount: f32, at: [f64; 3]) {
    let mut hits = lock(&HITS);
    hits.retain(|(v, _, _)| *v != victim);
    push_capped(&mut hits, (victim, amount, at));
}

/// The hit on `victim` landed (the weapon's hurt-enemy hook): its damage and where.
pub fn take_hit(victim: u32) -> Option<(f32, [f64; 3])> {
    let mut hits = lock(&HITS);
    let i = hits.iter().position(|(v, _, _)| *v == victim)?;
    let (_, amount, at) = hits.remove(i);
    Some((amount, at))
}

pub fn queue_splash(splash: Splash) {
    push_capped(&mut lock(&SPLASHES), splash);
}

/// The splashes waiting for `attacker`'s player tick.
pub fn take_splashes(attacker: u128) -> Vec<Splash> {
    let mut all = lock(&SPLASHES);
    let (mine, rest) = std::mem::take(&mut *all)
        .into_iter()
        .partition(|s| s.attacker == attacker);
    *all = rest;
    mine
}

/// Whether a mob standing at `at` is caught by a splash of `radius` blocks around `center`.
pub fn in_splash(center: [f64; 3], radius: f64, at: [f64; 3]) -> bool {
    let d2: f64 = (0..3).map(|i| (at[i] - center[i]).powi(2)).sum();
    d2 <= radius * radius
}

/// A reaper scythe killed mob `entity`: `head` drops with its loot.
pub fn note_head(entity: u32, head: &'static str) {
    push_capped(&mut lock(&HEADS), (entity, head));
}

/// The head mob `entity`'s loot gets, once.
pub fn take_head(entity: u32) -> Option<&'static str> {
    let mut heads = lock(&HEADS);
    let i = heads.iter().position(|(e, _)| *e == entity)?;
    Some(heads.remove(i).1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r##"{
        "armor_sets": [
            {"pieces": ["m:helm", "m:chest", "m:legs", "m:boots"], "setting": "set_bonuses",
             "effects": ["minecraft:night_vision", {"effect": "minecraft:speed", "amplifier": 1}]},
            {"pieces": ["m:u1", "m:u2", "m:u3", "m:u4"], "no_fall_damage": true,
             "regen": {"every_ticks": 80, "health": 1.0}},
            {"pieces": ["m:o1", "m:o2", "m:o3", "m:o4"], "knockback_taken": 0.5,
             "explosion_taken": 0.5, "experience_bonus": 0.25},
            {"pieces": []}
        ],
        "tools": [
            {"items": ["m:pick"], "ability": "vein_mine", "setting": "vein_mining"},
            {"items": ["m:shovel", "m:big_pick"], "ability": "wide_dig"},
            {"items": ["m:axe"], "ability": "fell_tree"},
            {"items": ["m:wand"], "ability": "fly"}
        ],
        "ores": ["m:odd_ore_block"],
        "harvest": [{"items": ["m:hoe"], "radius": 1}, {"items": ["m:scythe"], "radius": 2}],
        "weapons": [
            {"items": ["m:dagger"], "backstab": 2.0},
            {"items": ["m:greatsword"], "splash": {"radius": 1.5, "share": 0.5}},
            {"items": ["m:magma"], "fire_ticks": 80},
            {"items": ["m:eater"], "heal_on_kill": 2.0},
            {"items": ["m:pig"], "bonus_against": {"kinds": ["minecraft:piglin", "zombified_piglin"], "multiplier": 1.5}},
            {"items": ["m:legacy"], "bonus_in": {"dimension": "minecraft:the_nether", "multiplier": 1.5}},
            {"items": ["m:scythe"], "heads": true, "setting": "weapon_effects"}
        ]
    }"##;

    fn gear_of_file() -> (Gear, Vec<String>) {
        let mut gear = Gear::default();
        let problems = read_gear("m", &serde_json::from_str(FILE).unwrap(), &mut gear);
        (gear, problems)
    }

    #[test]
    fn a_file_becomes_rules_and_says_what_it_could_not_use() {
        let (gear, problems) = gear_of_file();
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert_eq!((gear.sets.len(), gear.tools.len(), gear.harvests.len(), gear.weapons.len()), (3, 3, 2, 7));
        assert!(gear.sets.iter().all(|s| s.mod_id == "m"));
        assert_eq!(gear.sets[0].effects, vec![("minecraft:night_vision".to_owned(), 0), ("minecraft:speed".to_owned(), 1)]);
        assert_eq!(gear.sets[0].setting.as_deref(), Some("set_bonuses"));
        assert_eq!((gear.sets[1].regen_every, gear.sets[1].regen_health), (80, 1.0));
        assert!(gear.is_listed_ore("m:odd_ore_block") && !gear.is_listed_ore("minecraft:stone"));
        // Nothing declared is no rules, and a file of another shape is no rules either.
        let mut empty = Gear::default();
        assert!(read_gear("m", &serde_json::json!({"tools": 3, "weapons": {}}), &mut empty).is_empty());
        assert_eq!(empty, Gear::default());
    }

    #[test]
    fn whole_sets_only() {
        let (gear, _) = gear_of_file();
        let worn = |ids: [&'static str; 4]| gear.set_of(ids.map(Some));
        assert_eq!(worn(["m:helm", "m:chest", "m:legs", "m:boots"]), Some(gear.sets[0].clone()));
        // The slots don't matter, the pieces do.
        assert_eq!(worn(["m:boots", "m:legs", "m:chest", "m:helm"]), Some(gear.sets[0].clone()));
        assert_eq!(worn(["m:helm", "m:u2", "m:legs", "m:boots"]), None);
        assert_eq!(gear.set_of([Some("m:helm"), None, None, None]), None);
        assert_eq!(gear.set_of([None; 4]), None);
    }

    #[test]
    fn set_effects_and_damage() {
        let (gear, _) = gear_of_file();
        assert!(effect_ticks("minecraft:night_vision") > 200);
        assert!(effect_ticks("minecraft:speed") as u64 > EFFECT_REFRESH_TICKS);
        let (plain, ultimate, obsidian) = (&gear.sets[0], &gear.sets[1], &gear.sets[2]);
        assert_eq!(incoming_damage(ultimate, "minecraft:fall"), DamageChange::Cancel);
        assert_eq!(incoming_damage(ultimate, "minecraft:explosion"), DamageChange::Keep);
        assert_eq!(incoming_damage(obsidian, "minecraft:player_explosion"), DamageChange::Scale(0.5));
        assert_eq!(incoming_damage(obsidian, "minecraft:fall"), DamageChange::Keep);
        assert_eq!(incoming_damage(plain, "minecraft:fall"), DamageChange::Keep);
        assert_eq!(obsidian.knockback_taken, 0.5);
        assert_eq!(plain.knockback_taken, 1.0);
    }

    #[test]
    fn experience_bonus_is_exact_over_time() {
        let mut carry = 0.0;
        let total: i32 = (0..100).map(|_| experience_bonus(1, 0.25, &mut carry)).sum();
        assert_eq!(total, 25);
        assert_eq!(experience_bonus(8, 0.25, &mut 0.0), 2);
        assert_eq!(experience_bonus(0, 0.25, &mut carry), 0);
        assert_eq!(experience_bonus(-5, 0.25, &mut carry), 0);
        assert_eq!(experience_bonus(8, 0.0, &mut carry), 0);
    }

    #[test]
    fn wearers_are_found_by_entity_and_uuid() {
        let (gear, _) = gear_of_file();
        let set = gear.sets[2].clone();
        let uuid = 0xfeed_u128;
        assert_eq!(note_wearer(uuid, 9001, Some(set.clone())), (None, 1));
        assert_eq!(set_of_entity(9001), Some(set.clone()));
        assert_eq!(note_wearer(uuid, 9001, None), (Some(set), 2));
        assert_eq!(set_of_entity(9001), None);
        assert_eq!(with_wearer(uuid, |w| w.ticks), Some(2));
        assert_eq!(with_wearer(0xbeef, |w| w.ticks), None);
    }

    #[test]
    fn switches_are_read_once_a_second_and_default_on() {
        assert!(read_flag(None) && read_flag(Some("true".into())) && !read_flag(Some(" false\n".into())));
        assert!(switch_on("sw_test", None, 0, |_, _| panic!("no key, no read")));
        assert!(switch_on_now("sw_test", Some("dig")));
        assert!(!switch_on("sw_test", Some("dig"), 5_000, |m, k| {
            assert_eq!((m, k), ("sw_test", "dig"));
            Some("false".into())
        }));
        assert!(!switch_on("sw_test", Some("dig"), 5_500, |_, _| panic!("read again too soon")));
        assert!(!switch_on_now("sw_test", Some("dig")));
        // A second on, the page's change is picked up.
        assert!(switch_on("sw_test", Some("dig"), 6_000, |_, _| None));
        assert!(switch_on_now("sw_test", Some("dig")));
        assert!(mod_on("sw_test"));
        note_mod_on("sw_test", false);
        assert!(!mod_on("sw_test"));
        note_mod_on("sw_test", true);
        assert!(mod_on("sw_test"));
    }

    #[test]
    fn daylight_list() {
        assert!(!no_sunburn("m:shade"));
        set_no_sunburn(vec!["m:shade".into()]);
        assert!(no_sunburn("m:shade") && !no_sunburn("minecraft:zombie"));
        set_no_sunburn(Vec::new());
        assert!(!no_sunburn("m:shade"));
    }

    #[test]
    fn tools_and_their_abilities() {
        let (gear, _) = gear_of_file();
        assert_eq!(gear.tool("m:pick").map(|t| t.ability), Some(Ability::VeinMine));
        assert_eq!(gear.tool("m:big_pick").map(|t| t.ability), Some(Ability::WideDig));
        assert_eq!(gear.tool("m:axe").map(|t| t.ability), Some(Ability::FellTree));
        assert_eq!(gear.tool("m:pick").and_then(|t| t.setting.as_deref()), Some("vein_mining"));
        assert!(gear.tool("m:wand").is_none() && gear.tool("minecraft:diamond_pickaxe").is_none());
    }

    #[test]
    fn ores_logs_and_veins() {
        assert!(looks_like_ore("minecraft:iron_ore") && !looks_like_ore("minecraft:stone"));
        assert!(looks_like_log("minecraft:oak_log") && looks_like_log("minecraft:crimson_stem"));
        assert!(!looks_like_log("minecraft:stripped_oak_log") && !looks_like_log("minecraft:oak_planks"));
        assert_eq!(vein_kind("minecraft:deepslate_iron_ore"), vein_kind("minecraft:iron_ore"));
        assert_eq!(vein_kind("useful_armory:deepslate_aquite_ore"), vein_kind("useful_armory:aquiteore"));
        assert_ne!(vein_kind("minecraft:gold_ore"), vein_kind("minecraft:nether_gold_ore"));
        assert_ne!(vein_kind("minecraft:iron_ore"), vein_kind("minecraft:copper_ore"));
    }

    #[test]
    fn flood_follows_26_neighbours_and_stops_at_the_cap() {
        assert_eq!(around().count(), 26);
        // A diagonal line is connected through corners.
        let line: Vec<[i32; 3]> = (1..=5).map(|i| [i, i, i]).collect();
        let found = flood([0, 0, 0], 32, |p| line.contains(&p));
        assert_eq!(found, line);
        // A solid 5x5x5 cube is cut at the cap, nearest first.
        let cube = |p: [i32; 3]| p.iter().all(|c| (-2..=2).contains(c));
        let found = flood([0, 0, 0], 32, cube);
        assert_eq!(found.len(), 32);
        assert!(found[..26].iter().all(|p| p.iter().all(|c| c.abs() <= 1)));
        assert!(!found.contains(&[0, 0, 0]));
        let mut unique = found.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), found.len());
        // Logs above only.
        let origin = [0, 64, 0];
        let trunk = |p: [i32; 3]| p[0] == 0 && p[2] == 0 && (60..70).contains(&p[1]) && p[1] >= origin[1];
        assert_eq!(flood(origin, TREE_MAX, trunk).len(), 5);
        assert!(flood(origin, 32, |_| false).is_empty());
    }

    #[test]
    fn wide_dig_square_faces_the_player() {
        assert_eq!(hit_axis([3, 64, 5], [3.5, 65.0, 5.2]), Some(Axis::Y));
        assert_eq!(hit_axis([3, 64, 5], [4.0, 64.5, 5.5]), Some(Axis::X));
        assert_eq!(hit_axis([-3, 64, -5], [-2.3, 64.5, -5.0]), Some(Axis::Z));
        assert_eq!(hit_axis([3, 64, 5], [3.5, 64.5, 5.5]), None);
        assert_eq!(hit_axis([3, 64, 5], [9.0, 64.0, 5.5]), None);
        assert_eq!(look_axis(0.0, 10.0), Axis::Z);
        assert_eq!(look_axis(90.0, -20.0), Axis::X);
        assert_eq!(look_axis(-135.0, 0.0), Axis::Z);
        assert_eq!(look_axis(30.0, 80.0), Axis::Y);
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let sq = square([0, 64, 0], axis);
            assert_eq!(sq.len(), 8);
            assert!(!sq.contains(&[0, 64, 0]));
            let fixed = match axis {
                Axis::X => 0,
                Axis::Y => 1,
                Axis::Z => 2,
            };
            assert!(sq.iter().all(|p| p[fixed] == [0, 64, 0][fixed]));
        }
    }

    #[test]
    fn durability_is_kept() {
        assert!(spare_durability(0, 0, 2));
        assert!(spare_durability(100, 97, 2));
        assert!(!spare_durability(100, 98, 2));
        assert!(!spare_durability(100, 120, 1));
    }

    #[test]
    fn breaking_guard_is_not_reentrant() {
        let first = Breaking::start();
        assert!(first.is_some() && breaking());
        assert!(Breaking::start().is_none());
        drop(first);
        assert!(!breaking());
        assert!(Breaking::start().is_some());
    }

    #[test]
    fn backstab_is_within_60_degrees_of_behind() {
        // The target at the origin faces south (+Z): behind it is north (-Z).
        let (t, view) = ([0.0, 64.0, 0.0], [0.0, -0.3, 1.0]);
        assert!(behind(t, view, [0.0, 64.0, -2.0]));
        let at = |deg: f64| {
            let r = deg.to_radians();
            [2.0 * r.sin(), 64.0, -2.0 * r.cos()]
        };
        assert!(behind(t, view, at(59.0)) && behind(t, view, at(-59.0)));
        assert!(!behind(t, view, at(61.0)) && !behind(t, view, at(-61.0)));
        assert!(!behind(t, view, [0.0, 64.0, 2.0]));
        assert!(!behind(t, view, [2.0, 64.0, 0.0]));
        assert!(!behind(t, [0.0, 1.0, 0.0], [0.0, 64.0, -2.0]));
        assert!(!behind(t, view, t));
    }

    #[test]
    fn hoes_harvest_grown_crops() {
        let (gear, _) = gear_of_file();
        assert_eq!(gear.harvest("m:hoe").map(|h| h.radius), Some(1));
        assert_eq!(gear.harvest("m:scythe").map(|h| h.radius), Some(2));
        assert!(gear.harvest("minecraft:iron_hoe").is_none());
        let wheat = crop("minecraft:wheat").unwrap();
        assert!(grown(wheat, Some("7")) && !grown(wheat, Some("6")) && !grown(wheat, None));
        assert!(grown(crop("minecraft:beetroots").unwrap(), Some("3")));
        assert!(crop("minecraft:pumpkin_stem").is_none());
        assert_eq!(field([0, 64, 0], 1).len(), 9);
        assert_eq!(field([0, 64, 0], 2).len(), 25);
        assert_eq!(field([4, 64, 4], 2)[0], [4, 64, 4]);
        let mut drops = vec![("minecraft:wheat".to_owned(), 1u32), ("minecraft:wheat_seeds".to_owned(), 2)];
        assert!(take_one(drops.iter_mut().map(|(i, c)| (i.as_str(), c)), "minecraft:wheat_seeds"));
        assert_eq!(drops[1].1, 1);
        assert!(!take_one(drops.iter_mut().map(|(i, c)| (i.as_str(), c)), "minecraft:carrot"));
    }

    #[test]
    fn weapons_and_their_effects() {
        let (gear, _) = gear_of_file();
        let w = |item: &str| gear.weapon(item).unwrap();
        let nether = "minecraft:the_nether";
        let over = "minecraft:overworld";
        assert!(gear.weapon("m:sword").is_none() && gear.weapon("minecraft:iron_sword").is_none());
        assert_eq!(hit_multiplier(&w("m:dagger"), "minecraft:zombie", over, true), 2.0);
        assert_eq!(hit_multiplier(&w("m:dagger"), "minecraft:zombie", over, false), 1.0);
        assert_eq!(hit_multiplier(&w("m:pig"), "minecraft:zombified_piglin", over, false), 1.5);
        assert_eq!(hit_multiplier(&w("m:pig"), "piglin", over, true), 1.5);
        assert_eq!(hit_multiplier(&w("m:pig"), "minecraft:zombie", nether, false), 1.0);
        assert_eq!(hit_multiplier(&w("m:legacy"), "minecraft:zombie", nether, false), 1.5);
        assert_eq!(hit_multiplier(&w("m:legacy"), "minecraft:zombie", over, true), 1.0);
        assert_eq!((w("m:greatsword").splash_radius, w("m:greatsword").splash_share), (1.5, 0.5));
        assert_eq!(w("m:magma").fire_ticks, 80);
        assert_eq!(w("m:eater").heal_on_kill, 2.0);
        assert!(w("m:scythe").heads && !w("m:dagger").heads);
        assert_eq!(head_of("minecraft:wither_skeleton"), Some(("minecraft:wither_skeleton_skull", 0.05)));
        assert_eq!(head_of("minecraft:spider"), None);
    }

    #[test]
    fn splashes_and_heads_wait_for_their_turn() {
        note_hit(71, 8.0, [1.0, 64.0, 1.0]);
        note_hit(71, 9.0, [1.0, 64.0, 1.0]);
        assert_eq!(take_hit(71), Some((9.0, [1.0, 64.0, 1.0])));
        assert_eq!(take_hit(71), None);
        let s = |attacker| Splash {
            attacker,
            victim: 72,
            at: [0.0; 3],
            amount: 4.0,
            radius: 1.5,
        };
        queue_splash(s(1));
        queue_splash(s(2));
        queue_splash(s(1));
        assert_eq!(take_splashes(1).len(), 2);
        assert_eq!(take_splashes(1).len(), 0);
        assert_eq!(take_splashes(2), vec![s(2)]);
        assert!(in_splash([0.0, 64.0, 0.0], 1.5, [1.0, 64.0, 1.0]));
        assert!(!in_splash([0.0, 64.0, 0.0], 1.5, [1.5, 64.0, 1.0]));
        assert!(!in_splash([0.0, 64.0, 0.0], 0.0, [0.5, 64.0, 0.0]));
        note_head(73, "minecraft:zombie_head");
        assert_eq!(take_head(73), Some("minecraft:zombie_head"));
        assert_eq!(take_head(73), None);
        for i in 0..200 {
            note_head(1000 + i, "minecraft:creeper_head");
        }
        assert!(lock(&HEADS).len() <= MAX_WAITING);
    }

    /// For mod authors: `ARMORY_LIB_FILE=<path to armory_lib.json> cargo test checks_a_mods_file`
    /// fails with what the library could not use of the file's gear rules.
    #[test]
    fn checks_a_mods_file() {
        let Ok(path) = std::env::var("ARMORY_LIB_FILE") else {
            return;
        };
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let file: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        let problems = read_gear("mod", &file, &mut Gear::default());
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
