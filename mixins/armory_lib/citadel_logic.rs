// Sealed doors and structure finders, without the game: what the mods' files declare
// (`sealed_doors` and `structure_finders` in `data/<mod>/armory_lib.json`), which blocks make up
// one gate, and the words a finder says. Added to the game as `game::armory_lib_citadel_logic`,
// which `citadel.rs` runs in the game's hooks.
//
// It is also a test target of the repository's Cargo package: `cargo test` runs it alone.
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, RwLock};

/// The most blocks one key opens (a gate bigger than this opens this much of it).
pub const MAX_GATE: u32 = 256;
/// The farthest a finder searches, in placement cells of the structure's set (`/locate` uses 100).
pub const MAX_RADIUS: u32 = 100;

/// Blocks that open, all connected ones together, for a key that is used up.
#[derive(Clone, Debug, PartialEq)]
pub struct SealedDoor {
    pub mod_id: String,
    pub setting: Option<String>,
    pub blocks: Vec<String>,
    pub key: String,
    /// The most blocks one key opens.
    pub max_blocks: u32,
    /// A vanilla sound event played where the gate stood.
    pub sound: String,
    /// What a click with anything else says (on the action bar).
    pub locked: String,
}

/// Items that tell the way to the nearest start of a structure.
#[derive(Clone, Debug, PartialEq)]
pub struct Finder {
    pub mod_id: String,
    pub setting: Option<String>,
    pub items: Vec<String>,
    pub structure: String,
    /// How the answer names the place: "The citadel" lies about ...
    pub name: String,
    pub radius: u32,
    pub cooldown_seconds: u32,
    pub not_found: String,
}

/// Every mod's sealed doors and finders together.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Citadel {
    pub doors: Vec<SealedDoor>,
    pub finders: Vec<Finder>,
}

impl Citadel {
    pub fn door(&self, block: &str) -> Option<&SealedDoor> {
        self.doors.iter().find(|d| d.blocks.iter().any(|b| b == block))
    }

    pub fn finder(&self, item: &str) -> Option<&Finder> {
        self.finders.iter().find(|f| f.items.iter().any(|i| i == item))
    }

    /// Puts what mod `mod_id` declares in place of what it declared before.
    pub fn put(&mut self, mod_id: &str, doors: Vec<SealedDoor>, finders: Vec<Finder>) {
        self.doors.retain(|d| d.mod_id != mod_id);
        self.finders.retain(|f| f.mod_id != mod_id);
        self.doors.extend(doors);
        self.finders.extend(finders);
    }
}

static CITADEL: RwLock<Option<Arc<Citadel>>> = RwLock::new(None);

/// The sealed doors and finders in force.
pub fn citadel() -> Arc<Citadel> {
    CITADEL
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Reads the `sealed_doors` and `structure_finders` sections of mod `mod_id`'s file (the README
/// has the format). What could not be used, one line each.
pub fn read(mod_id: &str, file: &Value) -> (Vec<SealedDoor>, Vec<Finder>, Vec<String>) {
    let mut problems = Vec::new();
    let mut doors = Vec::new();
    for entry in entries(mod_id, file, "sealed_doors", &mut problems) {
        let blocks = strings(entry.get("blocks"));
        let Some(key) = text(entry, "key") else {
            problems.push(format!("{mod_id}: a sealed_doors entry has no key"));
            continue;
        };
        if blocks.is_empty() {
            problems.push(format!("{mod_id}: a sealed_doors entry has no blocks"));
            continue;
        }
        let max_blocks = whole(mod_id, "sealed_doors", entry, "max_blocks", 64, MAX_GATE, &mut problems).max(1);
        doors.push(SealedDoor {
            mod_id: mod_id.to_owned(),
            setting: text(entry, "setting"),
            blocks,
            key,
            max_blocks,
            sound: text(entry, "sound").unwrap_or_else(|| "minecraft:block.vault.open_shutter".to_owned()),
            locked: text(entry, "locked").unwrap_or_else(|| "It is sealed".to_owned()),
        });
    }
    let mut finders = Vec::new();
    for entry in entries(mod_id, file, "structure_finders", &mut problems) {
        let items = strings(entry.get("items"));
        let Some(structure) = text(entry, "structure") else {
            problems.push(format!("{mod_id}: a structure_finders entry has no structure"));
            continue;
        };
        if items.is_empty() {
            problems.push(format!("{mod_id}: a structure_finders entry has no items"));
            continue;
        }
        let radius = whole(mod_id, "structure_finders", entry, "radius", 32, MAX_RADIUS, &mut problems).max(1);
        let cooldown_seconds = whole(mod_id, "structure_finders", entry, "cooldown_seconds", 3, 3600, &mut problems);
        finders.push(Finder {
            mod_id: mod_id.to_owned(),
            setting: text(entry, "setting"),
            items,
            name: text(entry, "name").unwrap_or_else(|| structure.clone()),
            structure,
            radius,
            cooldown_seconds,
            not_found: text(entry, "not_found")
                .unwrap_or_else(|| "The markings lead nowhere you can reach".to_owned()),
        });
    }
    (doors, finders, problems)
}

/// Reads mod `mod_id`'s sections and puts them in force in place of its earlier ones. Returns
/// what could not be used.
pub fn read_and_keep(mod_id: &str, file: &Value) -> Vec<String> {
    let (doors, finders, problems) = read(mod_id, file);
    let mut c = CITADEL.write().unwrap_or_else(|e| e.into_inner());
    let mut now = c.as_deref().cloned().unwrap_or_default();
    now.put(mod_id, doors, finders);
    *c = Some(Arc::new(now));
    problems
}

fn entries<'a>(mod_id: &str, file: &'a Value, key: &str, problems: &mut Vec<String>) -> Vec<&'a Value> {
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
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// A whole number from 0 to `max`, `default` when left out; out of range is clamped and said.
fn whole(mod_id: &str, section: &str, entry: &Value, key: &str, default: u32, max: u32, problems: &mut Vec<String>) -> u32 {
    match entry.get(key) {
        None | Some(Value::Null) => default,
        Some(value) => match value.as_f64() {
            None => {
                problems.push(format!("{mod_id}: {section} {key} must be a number; using {default}"));
                default
            }
            Some(n) if !(0.0..=f64::from(max)).contains(&n) => {
                let kept = n.clamp(0.0, f64::from(max)).round() as u32;
                problems.push(format!("{mod_id}: {section} {key} {n} is outside 0 to {max}; using {kept}"));
                kept
            }
            Some(n) => n.round() as u32,
        },
    }
}

// ---- One gate ----------------------------------------------------------------------------

/// The blocks of the gate clicked at `start`: every block `is_door` says yes to that joins it
/// face to face, nearest first, at most `max`.
pub fn gate(start: [i32; 3], max: u32, mut is_door: impl FnMut([i32; 3]) -> bool) -> Vec<[i32; 3]> {
    let mut seen = HashSet::from([start]);
    let mut todo = VecDeque::from([start]);
    let mut out = Vec::new();
    while let Some(p) = todo.pop_front() {
        if out.len() as u32 >= max {
            break;
        }
        if !is_door(p) {
            continue;
        }
        out.push(p);
        for d in [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] {
            let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            if seen.insert(n) {
                todo.push_back(n);
            }
        }
    }
    out
}

/// The middle of the gate's blocks, for its sound and smoke.
pub fn middle(blocks: &[[i32; 3]]) -> [f64; 3] {
    let n = blocks.len().max(1) as f64;
    let sum = blocks.iter().fold([0.0; 3], |s, b| [s[0] + f64::from(b[0]), s[1] + f64::from(b[1]), s[2] + f64::from(b[2])]);
    [sum[0] / n + 0.5, sum[1] / n + 0.5, sum[2] / n + 0.5]
}

// ---- What a finder says ------------------------------------------------------------------

/// The compass point from a step `dx` east and `dz` south: north, north-east, ... (eight).
pub fn direction(dx: f64, dz: f64) -> &'static str {
    const POINTS: [&str; 8] = ["east", "south-east", "south", "south-west", "west", "north-west", "north", "north-east"];
    let turn = dz.atan2(dx).to_degrees().rem_euclid(360.0);
    POINTS[((turn + 22.5) / 45.0) as usize % 8]
}

/// A distance in blocks rounded as a guess would be: to 10 under 200, to 50 under 1,000, to
/// 100 beyond, written with a thousands comma.
pub fn rough(distance: f64) -> String {
    let step = if distance < 200.0 {
        10.0
    } else if distance < 1000.0 {
        50.0
    } else {
        100.0
    };
    let n = ((distance / step).round() * step).max(step) as u64;
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What the finder says, from where the player stands to the structure's start: "The citadel
/// lies about 1,200 blocks north-east", or that it is right below when it is that close.
pub fn says(name: &str, from: [f64; 3], to: [i32; 3]) -> String {
    let dx = f64::from(to[0]) + 8.0 - from[0];
    let dz = f64::from(to[2]) + 8.0 - from[2];
    let distance = (dx * dx + dz * dz).sqrt();
    if distance < 32.0 {
        return format!("{name} lies right beneath your feet");
    }
    format!("{name} lies about {} blocks {}", rough(distance), direction(dx, dz))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_both_sections_with_defaults() {
        let file = json!({
            "sealed_doors": [{"blocks": ["m:sealed_door"], "key": "m:key"}],
            "structure_finders": [{"items": ["m:map"], "structure": "m:keep", "name": "The keep"}]
        });
        let (doors, finders, problems) = read("m", &file);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(doors[0].max_blocks, 64);
        assert_eq!(doors[0].sound, "minecraft:block.vault.open_shutter");
        assert_eq!(finders[0].radius, 32);
        assert_eq!(finders[0].cooldown_seconds, 3);
        assert_eq!(finders[0].not_found, "The markings lead nowhere you can reach");
        let mut c = Citadel::default();
        c.put("m", doors, finders);
        assert!(c.door("m:sealed_door").is_some() && c.door("m:other").is_none());
        assert_eq!(c.finder("m:map").map(|f| f.structure.as_str()), Some("m:keep"));
    }

    #[test]
    fn says_what_it_could_not_use() {
        let file = json!({
            "sealed_doors": [{"blocks": ["m:door"]}, {"key": "m:key"}, 3,
                             {"blocks": ["m:door"], "key": "m:key", "max_blocks": 5000}],
            "structure_finders": {"items": []}
        });
        let (doors, finders, problems) = read("m", &file);
        assert_eq!(doors.len(), 1);
        assert_eq!(doors[0].max_blocks, MAX_GATE);
        assert!(finders.is_empty());
        assert_eq!(problems.len(), 5, "{problems:?}");
    }

    #[test]
    fn a_mods_part_is_replaced_not_added() {
        let file = json!({"sealed_doors": [{"blocks": ["zz:door"], "key": "zz:key"}]});
        read_and_keep("zz_citadel_test", &file);
        read_and_keep("zz_citadel_test", &file);
        assert_eq!(citadel().doors.iter().filter(|d| d.mod_id == "zz_citadel_test").count(), 1);
        read_and_keep("zz_citadel_test", &json!({}));
        assert!(citadel().door("zz:door").is_none());
    }

    #[test]
    fn a_gate_is_every_door_block_joined_to_the_one_clicked() {
        // a 5 x 5 gate in the plane z = 2, and a door block elsewhere that is not part of it
        let doors: HashSet<[i32; 3]> = (8..13).flat_map(|x| (1..6).map(move |y| [x, y, 2])).chain([[20, 1, 2]]).collect();
        let gate_ = gate([10, 3, 2], 64, |p| doors.contains(&p));
        assert_eq!(gate_.len(), 25);
        assert_eq!(gate_[0], [10, 3, 2]);
        assert!(!gate_.contains(&[20, 1, 2]));
        assert_eq!(gate([10, 3, 2], 7, |p| doors.contains(&p)).len(), 7);
        assert!(gate([0, 0, 0], 64, |p| doors.contains(&p)).is_empty());
        assert_eq!(middle(&gate_), [10.5, 3.5, 2.5]);
    }

    #[test]
    fn compass_points() {
        assert_eq!(direction(0.0, -10.0), "north");
        assert_eq!(direction(10.0, -10.0), "north-east");
        assert_eq!(direction(10.0, 0.0), "east");
        assert_eq!(direction(10.0, 10.0), "south-east");
        assert_eq!(direction(0.0, 10.0), "south");
        assert_eq!(direction(-10.0, 10.0), "south-west");
        assert_eq!(direction(-10.0, 0.0), "west");
        assert_eq!(direction(-10.0, -10.0), "north-west");
        assert_eq!(direction(10.0, -3.0), "east");
    }

    #[test]
    fn rough_distances() {
        assert_eq!(rough(64.0), "60");
        assert_eq!(rough(3.0), "10");
        assert_eq!(rough(437.0), "450");
        assert_eq!(rough(1234.0), "1,200");
        assert_eq!(rough(12_345.0), "12,300");
        assert_eq!(rough(1_234_567.0), "1,234,600");
    }

    #[test]
    fn what_the_scrap_says() {
        let text = says("The citadel", [0.5, 70.0, 0.5], [840, 0, -856]);
        assert_eq!(text, "The citadel lies about 1,200 blocks north-east");
        assert_eq!(says("The citadel", [5.0, 20.0, 5.0], [0, 0, 0]), "The citadel lies right beneath your feet");
    }

    /// For mod authors: `ARMORY_LIB_FILE=<path to armory_lib.json> cargo test checks_a_mods_file`
    /// fails with what the library could not use of the file's sealed doors and finders.
    #[test]
    fn checks_a_mods_file() {
        let Ok(path) = std::env::var("ARMORY_LIB_FILE") else {
            return;
        };
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let file: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        let problems = read("mod", &file).2;
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
