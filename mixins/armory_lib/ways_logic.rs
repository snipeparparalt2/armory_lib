// Waystones and the recall charm, without the game: what the mods' files declare (`waystones`
// and `recall` in `data/<mod>/armory_lib.json`), a world's registry of waystones and its JSON
// file, the travel menu's order and the cooldown sums. Added to the game as
// `game::armory_lib_ways_logic`, which `ways.rs` runs in the game's hooks.
//
// It is also a test target of the repository's Cargo package: `cargo test` runs it alone.
use serde_json::{Value, json};
use std::sync::{Arc, RwLock};

/// The registry's file, in `<world>/armory_lib/`.
pub const REGISTRY_FILE: &str = "waystones.json";
/// The key in a recall item's `minecraft:custom_data` that holds the id of its waystone.
pub const RECALL_KEY: &str = "armory_lib:recall";
/// A server menu has at most 6 rows of 9.
pub const MENU_SLOTS: usize = 54;
/// The longest name a name tag gives a waystone (the anvil's limit).
pub const MAX_NAME: usize = 50;
/// A day, the longest cooldown a file can ask for.
pub const MAX_COOLDOWN_SECONDS: u32 = 86_400;
/// The most levels a trip can cost.
pub const MAX_COST_LEVELS: u32 = 100;

// ---- What the mods declare -------------------------------------------------------------

/// Blocks that are waystones, and what travelling from them costs.
#[derive(Clone, Debug, PartialEq)]
pub struct WaystoneDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub blocks: Vec<String>,
    /// Seconds a player waits between two trips (per player, whichever waystone).
    pub cooldown_seconds: u32,
    /// Experience levels a trip takes (none in creative).
    pub cost_levels: u32,
}

/// Items that take their user home, or to the waystone they are bound to.
#[derive(Clone, Debug, PartialEq)]
pub struct RecallDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub items: Vec<String>,
    pub cost_levels: u32,
    /// The item's cooldown after a use, as an ender pearl's (the hotbar greys it out).
    pub cooldown_seconds: u32,
}

/// Every mod's waystones and recall items together.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ways {
    pub waystones: Vec<WaystoneDef>,
    pub recall: Vec<RecallDef>,
}

impl Ways {
    /// The rule of the waystone block `block` (the first mod's that lists it).
    pub fn waystone(&self, block: &str) -> Option<&WaystoneDef> {
        self.waystones
            .iter()
            .find(|d| d.blocks.iter().any(|b| b == block))
    }

    /// The rule of the recall item `item`.
    pub fn recall(&self, item: &str) -> Option<&RecallDef> {
        self.recall
            .iter()
            .find(|d| d.items.iter().any(|i| i == item))
    }

    /// Puts what mod `mod_id` declares in place of what it declared before. `hooks.rs load()`
    /// reads the mods one at a time, so each mod's part is replaced on its own; the server and
    /// the client may read at the same time, and both read the same files.
    pub fn put(&mut self, mod_id: &str, waystones: Vec<WaystoneDef>, recall: Vec<RecallDef>) {
        self.waystones.retain(|d| d.mod_id != mod_id);
        self.recall.retain(|d| d.mod_id != mod_id);
        self.waystones.extend(waystones);
        self.recall.extend(recall);
    }
}

static WAYS: RwLock<Option<Arc<Ways>>> = RwLock::new(None);

/// The waystones and recall items in force.
pub fn ways() -> Arc<Ways> {
    WAYS.read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// What one mod's file declares, and what could not be used, one line each for the log.
#[derive(Debug, Default)]
pub struct Read {
    pub waystones: Vec<WaystoneDef>,
    pub recall: Vec<RecallDef>,
    pub problems: Vec<String>,
}

/// Reads the `waystones` and `recall` sections of mod `mod_id`'s file (the README has the
/// format). Values out of range are clamped, and said so.
pub fn read(mod_id: &str, file: &Value) -> Read {
    let mut out = Read::default();
    for entry in entries(mod_id, file, "waystones", &mut out.problems) {
        let blocks = strings(entry.get("blocks"));
        if blocks.is_empty() {
            out.problems
                .push(format!("{mod_id}: a waystones entry has no blocks"));
            continue;
        }
        let mut number = |key: &str, default: u32, max: u32| {
            whole(
                mod_id,
                "waystones",
                entry,
                key,
                default,
                max,
                &mut out.problems,
            )
        };
        let cooldown_seconds = number("cooldown_seconds", 60, MAX_COOLDOWN_SECONDS);
        let cost_levels = number("cost_levels", 0, MAX_COST_LEVELS);
        out.waystones.push(WaystoneDef {
            mod_id: mod_id.to_owned(),
            setting: text(entry, "setting"),
            blocks,
            cooldown_seconds,
            cost_levels,
        });
    }
    for entry in entries(mod_id, file, "recall", &mut out.problems) {
        let items = strings(entry.get("items"));
        if items.is_empty() {
            out.problems
                .push(format!("{mod_id}: a recall entry has no items"));
            continue;
        }
        let mut number = |key: &str, default: u32, max: u32| {
            whole(
                mod_id,
                "recall",
                entry,
                key,
                default,
                max,
                &mut out.problems,
            )
        };
        let cost_levels = number("cost_levels", 1, MAX_COST_LEVELS);
        let cooldown_seconds = number("cooldown_seconds", 120, MAX_COOLDOWN_SECONDS);
        out.recall.push(RecallDef {
            mod_id: mod_id.to_owned(),
            setting: text(entry, "setting"),
            items,
            cost_levels,
            cooldown_seconds,
        });
    }
    out
}

/// Reads mod `mod_id`'s sections and puts them in force in place of its earlier ones. Returns
/// what could not be used.
pub fn read_and_keep(mod_id: &str, file: &Value) -> Vec<String> {
    let found = read(mod_id, file);
    let mut ways = WAYS.write().unwrap_or_else(|e| e.into_inner());
    let mut now = ways.as_deref().cloned().unwrap_or_default();
    now.put(mod_id, found.waystones, found.recall);
    *ways = Some(Arc::new(now));
    found.problems
}

/// The objects of the list `key`; a list that isn't one, or an entry that isn't an object, is
/// a problem.
fn entries<'a>(
    mod_id: &str,
    file: &'a Value,
    key: &str,
    problems: &mut Vec<String>,
) -> Vec<&'a Value> {
    let Some(list) = file.get(key) else {
        return Vec::new();
    };
    let Some(list) = list.as_array() else {
        problems.push(format!("{mod_id}: {key} must be a list"));
        return Vec::new();
    };
    list.iter()
        .filter(|entry| {
            let object = entry.is_object();
            if !object {
                problems.push(format!("{mod_id}: a {key} entry is not an object"));
            }
            object
        })
        .collect()
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

/// A whole number from 0 to `max`, `default` when left out.
fn whole(
    mod_id: &str,
    section: &str,
    entry: &Value,
    key: &str,
    default: u32,
    max: u32,
    problems: &mut Vec<String>,
) -> u32 {
    match entry.get(key) {
        None | Some(Value::Null) => default,
        Some(value) => match value.as_f64() {
            None => {
                problems.push(format!(
                    "{mod_id}: {section} {key} must be a number; using {default}"
                ));
                default
            }
            Some(n) if !(0.0..=f64::from(max)).contains(&n) => {
                let kept = n.clamp(0.0, f64::from(max)).round() as u32;
                problems.push(format!(
                    "{mod_id}: {section} {key} {n} is outside 0 to {max}; using {kept}"
                ));
                kept
            }
            Some(n) => n.round() as u32,
        },
    }
}

// ---- A world's waystones ---------------------------------------------------------------

/// One waystone of the world: where it is, its name, and who found it.
#[derive(Clone, Debug, PartialEq)]
pub struct Waystone {
    pub id: String,
    pub dimension: String,
    pub pos: [i32; 3],
    pub name: String,
    /// The UUIDs of the players who found it, in the order they did.
    pub found_by: Vec<u128>,
}

impl Waystone {
    pub fn found(&self, player: u128) -> bool {
        self.found_by.contains(&player)
    }

    /// Notes that `player` found it; true the first time.
    pub fn mark_found(&mut self, player: u128) -> bool {
        let first = !self.found(player);
        if first {
            self.found_by.push(player);
        }
        first
    }
}

/// Every waystone of one world, as `<world>/armory_lib/waystones.json` keeps them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Registry {
    pub stones: Vec<Waystone>,
}

impl Registry {
    /// The registry a file holds. Entries without an id, a dimension or a position are left
    /// out, as is a second one at the same place.
    pub fn from_json(value: &Value) -> Registry {
        let mut registry = Registry::default();
        let list = value.get("waystones").and_then(Value::as_array);
        for entry in list.into_iter().flatten() {
            let (Some(id), Some(dimension)) = (text(entry, "id"), text(entry, "dimension")) else {
                continue;
            };
            let Some(pos) = block_pos(entry.get("pos")) else {
                continue;
            };
            if registry.index_at(&dimension, pos).is_some() || registry.get(&id).is_some() {
                continue;
            }
            let found_by = entry
                .get("found_by")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|u| u.as_str().and_then(parse_uuid))
                .fold(Vec::new(), |mut all, u| {
                    if !all.contains(&u) {
                        all.push(u);
                    }
                    all
                });
            registry.stones.push(Waystone {
                name: text(entry, "name")
                    .and_then(|n| clean_name(&n))
                    .unwrap_or_else(|| default_name(None, pos)),
                id,
                dimension,
                pos,
                found_by,
            });
        }
        registry
    }

    pub fn to_json(&self) -> Value {
        let stones: Vec<Value> = self
            .stones
            .iter()
            .map(|s| {
                json!({
                    "id": s.id,
                    "dimension": s.dimension,
                    "pos": s.pos,
                    "name": s.name,
                    "found_by": s.found_by.iter().map(|u| uuid_text(*u)).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({ "schema": 1, "waystones": stones })
    }

    pub fn get(&self, id: &str) -> Option<&Waystone> {
        self.stones.iter().find(|s| s.id == id)
    }

    pub fn index_at(&self, dimension: &str, pos: [i32; 3]) -> Option<usize> {
        self.stones
            .iter()
            .position(|s| s.pos == pos && s.dimension == dimension)
    }

    /// Adds a waystone named `name`, or `name 2`, `name 3`... when another has that name.
    /// Its index.
    pub fn add(&mut self, id: String, dimension: &str, pos: [i32; 3], name: &str) -> usize {
        let name = self.unique_name(name);
        self.stones.push(Waystone {
            id,
            dimension: dimension.to_owned(),
            pos,
            name,
            found_by: Vec::new(),
        });
        self.stones.len() - 1
    }

    /// `base`, or `base 2`, `base 3`... the first no waystone has.
    pub fn unique_name(&self, base: &str) -> String {
        let taken = |name: &str| {
            self.stones
                .iter()
                .any(|s| s.name.eq_ignore_ascii_case(name))
        };
        if !taken(base) {
            return base.to_owned();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| base.to_owned())
    }

    pub fn remove(&mut self, id: &str) -> Option<Waystone> {
        let i = self.stones.iter().position(|s| s.id == id)?;
        Some(self.stones.remove(i))
    }

    pub fn remove_at(&mut self, dimension: &str, pos: [i32; 3]) -> Option<Waystone> {
        let i = self.index_at(dimension, pos)?;
        Some(self.stones.remove(i))
    }

    /// What the travel menu lists for `player`, standing at `at` in `dimension`: the waystones
    /// they found, those of this dimension first, nearest first, then the others by dimension
    /// (the Overworld, the Nether, the End, then the rest) and name. At most a menu's worth.
    pub fn menu(&self, player: u128, dimension: &str, at: [f64; 3]) -> Vec<&Waystone> {
        let mut found: Vec<&Waystone> = self.stones.iter().filter(|s| s.found(player)).collect();
        found.sort_by(|a, b| {
            let (here_a, here_b) = (a.dimension == dimension, b.dimension == dimension);
            here_b
                .cmp(&here_a)
                .then_with(|| {
                    if here_a && here_b {
                        distance(at, a.pos).total_cmp(&distance(at, b.pos))
                    } else {
                        dimension_rank(&a.dimension)
                            .cmp(&dimension_rank(&b.dimension))
                            .then_with(|| a.dimension.cmp(&b.dimension))
                    }
                })
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        found.truncate(MENU_SLOTS);
        found
    }
}

fn block_pos(value: Option<&Value>) -> Option<[i32; 3]> {
    let list = value?.as_array()?;
    let mut pos = [0; 3];
    if list.len() != 3 {
        return None;
    }
    for (slot, v) in pos.iter_mut().zip(list) {
        *slot = i32::try_from(v.as_i64()?).ok()?;
    }
    Some(pos)
}

fn dimension_rank(dimension: &str) -> u8 {
    match dimension {
        "minecraft:overworld" => 0,
        "minecraft:the_nether" => 1,
        "minecraft:the_end" => 2,
        _ => 3,
    }
}

/// A UUID as vanilla writes one: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`, lower case.
pub fn uuid_text(uuid: u128) -> String {
    let hex = format!("{uuid:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A UUID with or without its dashes.
pub fn parse_uuid(text: &str) -> Option<u128> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    u128::from_str_radix(&hex, 16).ok()
}

// ---- Places and names ------------------------------------------------------------------

/// Where a traveller lands: on top of the waystone's block, in its middle.
pub fn landing(pos: [i32; 3]) -> [f64; 3] {
    [
        f64::from(pos[0]) + 0.5,
        f64::from(pos[1]) + 1.0,
        f64::from(pos[2]) + 0.5,
    ]
}

/// Blocks from `from` to where a traveller lands at the waystone at `pos`.
pub fn distance(from: [f64; 3], pos: [i32; 3]) -> f64 {
    let to = landing(pos);
    (0..3)
        .map(|i| (to[i] - from[i]).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// The rows a menu of `entries` items needs, 1 to 6.
pub fn rows(entries: usize) -> u8 {
    entries.div_ceil(9).clamp(1, 6) as u8
}

/// `dark_forest` -> `Dark Forest`.
pub fn title_case(path: &str) -> String {
    path.split(['_', '/'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A new waystone's name: its biome's (`minecraft:dark_forest` -> `Dark Forest`) where the
/// game knows it, else `Waystone <x>, <z>`.
pub fn default_name(biome: Option<&str>, pos: [i32; 3]) -> String {
    biome
        .map(|b| title_case(b.rsplit(':').next().unwrap_or(b)))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("Waystone {}, {}", pos[0], pos[2]))
}

/// `minecraft:the_nether` -> `The Nether`.
pub fn dimension_name(dimension: &str) -> String {
    match dimension {
        "minecraft:overworld" => "Overworld".to_owned(),
        "minecraft:the_nether" => "The Nether".to_owned(),
        "minecraft:the_end" => "The End".to_owned(),
        other => title_case(other.rsplit(':').next().unwrap_or(other)),
    }
}

/// The menu's icon of a waystone in `dimension`.
pub fn icon(dimension: &str) -> &'static str {
    match dimension {
        "minecraft:the_nether" => "minecraft:crying_obsidian",
        "minecraft:the_end" => "minecraft:end_stone",
        _ => "minecraft:lodestone",
    }
}

/// A name for a waystone from a name tag: trimmed, without control characters, at most
/// [`MAX_NAME`] characters; None when nothing is left.
pub fn clean_name(raw: &str) -> Option<String> {
    let name: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_NAME)
        .collect();
    let name = name.trim_end().to_owned();
    (!name.is_empty()).then_some(name)
}

/// `1 level`, `3 levels`.
pub fn levels(n: u32) -> String {
    if n == 1 {
        "1 level".to_owned()
    } else {
        format!("{n} levels")
    }
}

// ---- Cooldowns -------------------------------------------------------------------------

/// Whole seconds left of a cooldown of `cooldown_seconds` that began at `last_ms` (None:
/// never), at `now_ms`. A part of a second counts as one, so it reads 0 only once it is over.
pub fn seconds_left(last_ms: Option<u64>, now_ms: u64, cooldown_seconds: u32) -> u32 {
    let Some(last) = last_ms else {
        return 0;
    };
    let until = last.saturating_add(u64::from(cooldown_seconds) * 1000);
    u32::try_from(until.saturating_sub(now_ms).div_ceil(1000)).unwrap_or(u32::MAX)
}

/// An item cooldown of `seconds`, in ticks.
pub fn cooldown_ticks(seconds: u32) -> i32 {
    i32::try_from(u64::from(seconds) * 20).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: u128 = 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef;
    const B: u128 = 0xffee_ddcc_bbaa_9988_7766_5544_3322_1100;

    fn stone(id: &str, dimension: &str, pos: [i32; 3], name: &str, found_by: &[u128]) -> Waystone {
        Waystone {
            id: id.to_owned(),
            dimension: dimension.to_owned(),
            pos,
            name: name.to_owned(),
            found_by: found_by.to_vec(),
        }
    }

    #[test]
    fn reads_both_sections_with_defaults() {
        let file = json!({
            "waystones": [{"blocks": ["m:waystone", "m:mossy_waystone"], "setting": "waystones"}],
            "recall": [{"items": ["m:recall_charm"]}]
        });
        let read = read("m", &file);
        assert!(read.problems.is_empty(), "{:?}", read.problems);
        assert_eq!(
            read.waystones,
            vec![WaystoneDef {
                mod_id: "m".into(),
                setting: Some("waystones".into()),
                blocks: vec!["m:waystone".into(), "m:mossy_waystone".into()],
                cooldown_seconds: 60,
                cost_levels: 0,
            }]
        );
        assert_eq!(
            read.recall,
            vec![RecallDef {
                mod_id: "m".into(),
                setting: None,
                items: vec!["m:recall_charm".into()],
                cost_levels: 1,
                cooldown_seconds: 120,
            }]
        );
    }

    #[test]
    fn clamps_values_and_reports_unusable_entries() {
        let file = json!({
            "waystones": [
                {"blocks": ["m:w"], "cooldown_seconds": 999999, "cost_levels": -3},
                {"blocks": []},
                "m:w",
                {"blocks": ["m:x"], "cooldown_seconds": "soon", "cost_levels": 2.4}
            ],
            "recall": {"items": ["m:r"]}
        });
        let read = read("m", &file);
        assert_eq!(read.waystones.len(), 2);
        assert_eq!(read.waystones[0].cooldown_seconds, MAX_COOLDOWN_SECONDS);
        assert_eq!(read.waystones[0].cost_levels, 0);
        assert_eq!(read.waystones[1].cooldown_seconds, 60);
        assert_eq!(read.waystones[1].cost_levels, 2);
        assert!(read.recall.is_empty());
        assert_eq!(read.problems.len(), 6, "{:?}", read.problems);
        assert!(read.problems.iter().all(|p| p.starts_with("m: ")));
    }

    #[test]
    fn a_mods_part_is_replaced_on_its_own() {
        let def = |mod_id: &str, block: &str| WaystoneDef {
            mod_id: mod_id.into(),
            setting: None,
            blocks: vec![block.into()],
            cooldown_seconds: 60,
            cost_levels: 0,
        };
        let mut ways = Ways::default();
        ways.put("a", vec![def("a", "a:w")], Vec::new());
        ways.put("b", vec![def("b", "b:w")], Vec::new());
        ways.put("a", vec![def("a", "a:v")], Vec::new());
        assert!(ways.waystone("a:w").is_none());
        assert_eq!(ways.waystone("a:v").map(|d| d.mod_id.as_str()), Some("a"));
        assert_eq!(ways.waystone("b:w").map(|d| d.mod_id.as_str()), Some("b"));
        assert!(ways.recall("a:v").is_none());
    }

    #[test]
    fn the_registry_survives_its_file() {
        let mut registry = Registry::default();
        let i = registry.add("1".into(), "minecraft:overworld", [10, 64, -20], "Plains");
        registry.stones[i].mark_found(A);
        registry.stones[i].mark_found(B);
        registry.add("2".into(), "minecraft:the_nether", [-5, 70, 3], "Hot Spot");
        let text = serde_json::to_string(&registry.to_json()).unwrap();
        let back = Registry::from_json(&serde_json::from_str(&text).unwrap());
        assert_eq!(back, registry);
        assert!(text.contains(&uuid_text(A)));
    }

    #[test]
    fn a_damaged_file_keeps_what_it_can() {
        let file = json!({"waystones": [
            {"id": "1", "dimension": "minecraft:overworld", "pos": [1, 2, 3], "name": "  ",
             "found_by": ["not a uuid", uuid_text(A), uuid_text(A)]},
            {"id": "2", "dimension": "minecraft:overworld", "pos": [1, 2]},
            {"id": "3", "dimension": "minecraft:overworld", "pos": [1, 2, 3]},
            {"dimension": "minecraft:overworld", "pos": [4, 5, 6]},
            {"id": "1", "dimension": "minecraft:the_end", "pos": [0, 60, 0]}
        ]});
        let registry = Registry::from_json(&file);
        assert_eq!(registry.stones.len(), 1);
        assert_eq!(registry.stones[0].name, "Waystone 1, 3");
        assert_eq!(registry.stones[0].found_by, vec![A]);
        assert_eq!(Registry::from_json(&json!("nonsense")), Registry::default());
    }

    #[test]
    fn uuids_read_back() {
        assert_eq!(uuid_text(A), "01234567-89ab-cdef-0123-456789abcdef");
        assert_eq!(parse_uuid(&uuid_text(B)), Some(B));
        assert_eq!(parse_uuid("0123456789abcdef0123456789abcdef"), Some(A));
        assert_eq!(parse_uuid("0123"), None);
    }

    #[test]
    fn the_menu_lists_this_dimension_nearest_first_then_the_others() {
        let registry = Registry {
            stones: vec![
                stone("far", "minecraft:overworld", [1000, 64, 0], "Far", &[A]),
                stone("end", "minecraft:the_end", [0, 60, 0], "Island", &[A]),
                stone(
                    "hidden",
                    "minecraft:overworld",
                    [5, 64, 0],
                    "Not Found",
                    &[B],
                ),
                stone("near", "minecraft:overworld", [20, 64, 0], "Near", &[A, B]),
                stone("neth_b", "minecraft:the_nether", [0, 70, 0], "Basalt", &[A]),
                stone("neth_a", "minecraft:the_nether", [9000, 70, 0], "Ash", &[A]),
                stone("here", "minecraft:overworld", [0, 64, 0], "Home", &[A]),
            ],
        };
        let ids: Vec<&str> = registry
            .menu(A, "minecraft:overworld", [0.5, 65.0, 0.5])
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ids, ["here", "near", "far", "neth_a", "neth_b", "end"]);
        let from_nether: Vec<&str> = registry
            .menu(A, "minecraft:the_nether", [8990.0, 70.0, 0.0])
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(
            from_nether,
            ["neth_a", "neth_b", "far", "here", "near", "end"]
        );
    }

    #[test]
    fn the_menu_holds_six_rows() {
        let mut registry = Registry::default();
        for x in 0..70 {
            let i = registry.add(format!("{x}"), "minecraft:overworld", [x, 64, 0], "W");
            registry.stones[i].mark_found(A);
        }
        let menu = registry.menu(A, "minecraft:overworld", [0.0, 65.0, 0.0]);
        assert_eq!(menu.len(), MENU_SLOTS);
        assert_eq!(menu[0].id, "0");
        assert_eq!(rows(menu.len()), 6);
        assert_eq!(rows(0), 1);
        assert_eq!(rows(9), 1);
        assert_eq!(rows(10), 2);
    }

    #[test]
    fn names_are_kept_apart() {
        let mut registry = Registry::default();
        registry.add("1".into(), "minecraft:overworld", [0, 64, 0], "Plains");
        let i = registry.add("2".into(), "minecraft:overworld", [50, 64, 0], "plains");
        assert_eq!(registry.stones[i].name, "plains 2");
        assert_eq!(registry.unique_name("Plains"), "Plains 3");
        assert_eq!(
            registry
                .remove_at("minecraft:overworld", [0, 64, 0])
                .map(|s| s.id),
            Some("1".into())
        );
        assert_eq!(registry.remove("2").map(|s| s.id), Some("2".into()));
        assert!(registry.stones.is_empty());
    }

    #[test]
    fn found_once() {
        let mut s = stone("1", "minecraft:overworld", [0, 0, 0], "W", &[]);
        assert!(s.mark_found(A));
        assert!(!s.mark_found(A));
        assert!(s.found(A) && !s.found(B));
    }

    #[test]
    fn cooldowns_count_down_in_whole_seconds() {
        assert_eq!(seconds_left(None, 5_000, 60), 0);
        assert_eq!(seconds_left(Some(1_000), 1_000, 60), 60);
        assert_eq!(seconds_left(Some(1_000), 1_001, 60), 60);
        assert_eq!(seconds_left(Some(1_000), 60_001, 60), 1);
        assert_eq!(seconds_left(Some(1_000), 61_000, 60), 0);
        assert_eq!(seconds_left(Some(1_000), 900_000, 60), 0);
        assert_eq!(seconds_left(Some(u64::MAX - 5), 0, 60), u32::MAX);
        assert_eq!(seconds_left(Some(1_000), 1_000, 0), 0);
        assert_eq!(cooldown_ticks(120), 2400);
        assert_eq!(cooldown_ticks(MAX_COOLDOWN_SECONDS), 1_728_000);
    }

    #[test]
    fn names_read_well() {
        assert_eq!(
            default_name(Some("minecraft:dark_forest"), [1, 2, 3]),
            "Dark Forest"
        );
        assert_eq!(
            default_name(Some("terralith:moonlight_grove"), [1, 2, 3]),
            "Moonlight Grove"
        );
        assert_eq!(default_name(None, [-120, 64, 33]), "Waystone -120, 33");
        assert_eq!(dimension_name("minecraft:the_nether"), "The Nether");
        assert_eq!(dimension_name("mymod:deep_dark"), "Deep Dark");
        assert_eq!(icon("minecraft:the_end"), "minecraft:end_stone");
        assert_eq!(icon("mymod:deep_dark"), "minecraft:lodestone");
        assert_eq!(clean_name("  Home\n "), Some("Home".into()));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(80)).map(|n| n.len()), Some(MAX_NAME));
        assert_eq!(levels(1), "1 level");
        assert_eq!(levels(3), "3 levels");
    }

    #[test]
    fn distance_is_to_the_landing_spot() {
        assert_eq!(landing([2, 64, -3]), [2.5, 65.0, -2.5]);
        assert_eq!(distance([2.5, 65.0, 1.5], [2, 64, -3]), 4.0);
    }

    /// For mod authors: `ARMORY_LIB_FILE=<path to armory_lib.json> cargo test checks_a_mods_file`
    /// fails with what the library could not use of the file's waystones and recall items.
    #[test]
    fn checks_a_mods_file() {
        let Ok(path) = std::env::var("ARMORY_LIB_FILE") else {
            return;
        };
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let file: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        let problems = read("mod", &file).problems;
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
