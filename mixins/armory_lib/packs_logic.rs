// Backpacks and soul jars, with nothing from the game in them: what content mods declare in
// their `data/<mod>/armory_lib.json` (`backpacks`, `soul_jars`; the README has the format)
// and the sums the game module `armory_lib_packs` (packs.rs) makes with them. Added to the game
// as `game::armory_lib_packs_logic`; it is also a test target of the repository's Cargo
// package, so `cargo test` runs it alone.
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// The most slots a backpack shows: a chest menu of 6 rows.
pub const MAX_SLOTS: usize = 54;
/// The rows of a backpack entry that names none: a single chest's.
pub const DEFAULT_ROWS: u8 = 3;
/// The souls a jar holds when its entry names no capacity.
pub const DEFAULT_CAPACITY: u32 = 50;
/// The most souls a jar can be made to hold.
pub const MAX_CAPACITY: u32 = 1_000_000;

/// A backpack rule: a right click with one of `items` opens `rows` rows of storage that the
/// world keeps for that bag.
#[derive(Clone, Debug, PartialEq)]
pub struct BackpackDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub items: Vec<String>,
    /// 1 to 6.
    pub rows: u8,
}

/// A soul jar rule: `item` takes a soul for each mob its carrier kills and at `capacity`
/// souls becomes one `full`.
#[derive(Clone, Debug, PartialEq)]
pub struct SoulJarDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub item: String,
    pub full: String,
    /// 1 to `MAX_CAPACITY`.
    pub capacity: u32,
}

/// What one mod's file declares.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModPacks {
    pub backpacks: Vec<BackpackDef>,
    pub jars: Vec<SoulJarDef>,
}

/// Every mod's backpacks and jars, by mod id. Where two mods name the same item, the mod whose
/// id sorts first wins.
#[derive(Debug, Default)]
pub struct Packs {
    by_mod: BTreeMap<String, ModPacks>,
}

impl Packs {
    /// The backpack rule of item `item`.
    pub fn backpack(&self, item: &str) -> Option<&BackpackDef> {
        self.by_mod
            .values()
            .flat_map(|m| &m.backpacks)
            .find(|b| b.items.iter().any(|i| i == item))
    }

    /// The jar rule whose jar (empty or filling) is item `item`.
    pub fn jar(&self, item: &str) -> Option<&SoulJarDef> {
        self.jars().find(|j| j.item == item)
    }

    /// The jar rule whose full jar is item `item`.
    pub fn full_jar(&self, item: &str) -> Option<&SoulJarDef> {
        self.jars().find(|j| j.full == item)
    }

    pub fn has_jars(&self) -> bool {
        self.jars().next().is_some()
    }

    fn jars(&self) -> impl Iterator<Item = &SoulJarDef> {
        self.by_mod.values().flat_map(|m| &m.jars)
    }
}

static PACKS: RwLock<Option<Arc<Packs>>> = RwLock::new(None);

/// The backpacks and jars in force.
pub fn packs() -> Arc<Packs> {
    PACKS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Reads mod `mod_id`'s `backpacks` and `soul_jars` from its file and puts them in force in
/// place of what that mod's file declared before (`hooks.rs` calls it for each mod each time
/// the files are read). Returns what it could not use, one line each, for the log.
pub fn read(mod_id: &str, file: &serde_json::Value) -> Vec<String> {
    let (declared, problems) = parse(mod_id, file);
    let mut slot = PACKS.write().unwrap_or_else(|e| e.into_inner());
    let mut by_mod = slot.as_ref().map(|p| p.by_mod.clone()).unwrap_or_default();
    if declared == ModPacks::default() {
        by_mod.remove(mod_id);
    } else {
        by_mod.insert(mod_id.to_owned(), declared);
    }
    *slot = Some(Arc::new(Packs { by_mod }));
    problems
}

/// What mod `mod_id`'s file declares, with defaults filled in and values clamped, and a line
/// for each entry or value it could not use as written.
pub fn parse(mod_id: &str, file: &serde_json::Value) -> (ModPacks, Vec<String>) {
    let mut out = ModPacks::default();
    let mut problems = Vec::new();
    for entry in entries(mod_id, file, "backpacks", &mut problems) {
        let items = strings(entry.get("items"));
        if items.is_empty() {
            problems.push(format!("{mod_id}: a backpacks entry has no items"));
            continue;
        }
        let rows = match entry.get("rows") {
            None => DEFAULT_ROWS,
            Some(v) => match v.as_f64() {
                Some(n) if (1.0..=6.0).contains(&n) && n.fract() == 0.0 => n as u8,
                Some(n) => {
                    let rows = n.round().clamp(1.0, 6.0) as u8;
                    problems.push(format!(
                        "{mod_id}: backpack rows {n} is not 1 to 6; using {rows}"
                    ));
                    rows
                }
                None => {
                    problems.push(format!(
                        "{mod_id}: backpack rows {v} is not a number; using {DEFAULT_ROWS}"
                    ));
                    DEFAULT_ROWS
                }
            },
        };
        out.backpacks.push(BackpackDef {
            mod_id: mod_id.to_owned(),
            setting: text(entry, "setting"),
            items,
            rows,
        });
    }
    for entry in entries(mod_id, file, "soul_jars", &mut problems) {
        let (Some(item), Some(full)) = (text(entry, "item"), text(entry, "full")) else {
            problems.push(format!("{mod_id}: a soul_jars entry needs item and full"));
            continue;
        };
        if item == full {
            problems.push(format!(
                "{mod_id}: soul jar {item} can't be its own full jar"
            ));
            continue;
        }
        let capacity = match entry.get("capacity") {
            None => DEFAULT_CAPACITY,
            Some(v) => match v.as_f64() {
                Some(n) if (1.0..=f64::from(MAX_CAPACITY)).contains(&n) && n.fract() == 0.0 => {
                    n as u32
                }
                Some(n) => {
                    let capacity = n.round().clamp(1.0, f64::from(MAX_CAPACITY)) as u32;
                    problems.push(format!(
                        "{mod_id}: soul jar capacity {n} is not 1 to {MAX_CAPACITY}; using {capacity}"
                    ));
                    capacity
                }
                None => {
                    problems.push(format!(
                        "{mod_id}: soul jar capacity {v} is not a number; using {DEFAULT_CAPACITY}"
                    ));
                    DEFAULT_CAPACITY
                }
            },
        };
        out.jars.push(SoulJarDef {
            mod_id: mod_id.to_owned(),
            setting: text(entry, "setting"),
            item,
            full,
            capacity,
        });
    }
    (out, problems)
}

/// The objects of the list `key`; a line for a list that isn't one and each entry that isn't
/// an object.
fn entries<'a>(
    mod_id: &str,
    file: &'a serde_json::Value,
    key: &str,
    problems: &mut Vec<String>,
) -> Vec<&'a serde_json::Value> {
    let Some(value) = file.get(key) else {
        return Vec::new();
    };
    let Some(list) = value.as_array() else {
        problems.push(format!("{mod_id}: {key} is not a list"));
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

fn strings(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

// ---------------------------------------------------------------------------------------
// Backpacks
// ---------------------------------------------------------------------------------------

/// Whether `id` can be a bag's id: what `store::new_id` makes (hex digits) or close to it. It
/// becomes a file name, so anything that could leave the folder (`../level`) is refused and
/// the bag gets a new id.
pub fn valid_bag_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The bag's file under `<world>/armory_lib/`.
pub fn bag_file(id: &str) -> String {
    format!("backpacks/{id}.dat")
}

/// The rows to show a bag of `rows` rows whose slots (as its file has them) are `filled`: its
/// own, or more when its file holds items past them (its rule had more rows when they went
/// in), so that nothing in the file is out of reach or lost when it is written again.
pub fn rows_to_show(rows: u8, filled: impl IntoIterator<Item = bool>) -> u8 {
    let last = filled
        .into_iter()
        .enumerate()
        .filter(|(_, filled)| *filled)
        .map(|(i, _)| i)
        .last();
    let needed = last.map_or(0, |i| i / 9 + 1).min(6) as u8;
    rows.clamp(1, 6).max(needed)
}

/// What a closing bag keeps of its `slots`, in their places: every stack but those `refused`
/// (backpacks, which go back to the player).
pub fn to_keep<T: Clone>(slots: &[Option<T>], refused: impl Fn(&T) -> bool) -> Vec<Option<T>> {
    slots
        .iter()
        .map(|s| s.as_ref().filter(|s| !refused(s)).cloned())
        .collect()
}

/// Takes out of `slots` what `to_keep` kept (once it is written), leaving the refused stacks
/// to go back to the player.
pub fn take_kept<T>(slots: &mut [Option<T>], refused: impl Fn(&T) -> bool) {
    for slot in slots {
        if slot.as_ref().is_some_and(|s| !refused(s)) {
            *slot = None;
        }
    }
}

/// The translation key of an item's name, as `Item.getDescriptionId` makes it:
/// `mymod:small_pack` -> `item.mymod.small_pack`.
pub fn name_key(item: &str) -> String {
    let (namespace, path) = item.split_once(':').unwrap_or(("minecraft", item));
    format!("item.{namespace}.{}", path.replace('/', "."))
}

/// A name for an item a client has no translation of: `mymod:small_pack` -> `Small Pack`.
pub fn name_fallback(item: &str) -> String {
    let path = item.rsplit([':', '/']).next().unwrap_or(item);
    path.split('_')
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

// ---------------------------------------------------------------------------------------
// Soul jars
// ---------------------------------------------------------------------------------------

/// The slot whose jar takes the next soul: the first of `stacks` (slot, (item, count)), in
/// the order given (the hotbar left to right, then the off hand), that holds one jar.
/// A stack of several jars is passed over, as they would all fill from one kill.
pub fn first_jar<'a>(
    stacks: impl IntoIterator<Item = (usize, Option<(&'a str, u32)>)>,
    is_jar: impl Fn(&str) -> bool,
) -> Option<usize> {
    stacks
        .into_iter()
        .find(|(_, stack)| stack.is_some_and(|(item, count)| count == 1 && is_jar(item)))
        .map(|(slot, _)| slot)
}

/// A jar holding `souls` takes one more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Soul {
    /// It now holds this many.
    Count(u32),
    /// It reached its capacity and becomes its full jar.
    Full,
}

pub fn add_soul(souls: u32, capacity: u32) -> Soul {
    let souls = souls.saturating_add(1);
    if souls >= capacity.max(1) {
        Soul::Full
    } else {
        Soul::Count(souls)
    }
}

/// The souls a jar's custom data says it holds: none for a missing or negative count.
pub fn souls_of(stored: Option<i32>) -> u32 {
    stored.map_or(0, |n| n.max(0) as u32)
}

/// The width (out of 13) of the fill bar of a jar holding `souls`: `souls * 13 / capacity`,
/// and at least one pixel once it holds any, as vanilla's bundle shows its first item.
pub fn bar_width(souls: u32, capacity: u32) -> i32 {
    if souls == 0 {
        return 0;
    }
    let width = u64::from(souls) * 13 / u64::from(capacity.max(1));
    width.clamp(1, 13) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn backpacks_with_defaults_and_clamped_rows() {
        let file = json!({"backpacks": [
            {"items": ["m:small_pack"], "rows": 1, "setting": "backpacks"},
            {"items": ["m:pack", "m:pack_red"]},
            {"items": ["m:huge"], "rows": 9},
            {"items": ["m:none"], "rows": 0},
            {"items": ["m:odd"], "rows": "two"},
            {"items": []},
            {"rows": 2},
            "m:loose"
        ]});
        let (packs, problems) = parse("m", &file);
        let rows: Vec<(&str, u8)> = packs
            .backpacks
            .iter()
            .map(|b| (b.items[0].as_str(), b.rows))
            .collect();
        assert_eq!(
            rows,
            [
                ("m:small_pack", 1),
                ("m:pack", 3),
                ("m:huge", 6),
                ("m:none", 1),
                ("m:odd", 3)
            ]
        );
        assert_eq!(packs.backpacks[0].setting.as_deref(), Some("backpacks"));
        assert_eq!(packs.backpacks[1].setting, None);
        assert_eq!(packs.backpacks[1].items, ["m:pack", "m:pack_red"]);
        // Three clamped values, two entries with no items, one that isn't an object.
        assert_eq!(problems.len(), 6, "{problems:?}");
        assert!(problems.iter().all(|p| p.starts_with("m: ")));
    }

    #[test]
    fn soul_jars_with_defaults_and_clamped_capacity() {
        let file = json!({"soul_jars": [
            {"item": "m:soul_jar", "full": "m:soul_jar_full", "capacity": 50, "setting": "soul_jar"},
            {"item": "m:jar2", "full": "m:jar2_full"},
            {"item": "m:jar3", "full": "m:jar3_full", "capacity": 0},
            {"item": "m:jar4", "full": "m:jar4_full", "capacity": 2.5},
            {"item": "m:jar5"},
            {"item": "m:same", "full": "m:same"}
        ]});
        let (packs, problems) = parse("m", &file);
        let jars: Vec<(&str, &str, u32)> = packs
            .jars
            .iter()
            .map(|j| (j.item.as_str(), j.full.as_str(), j.capacity))
            .collect();
        assert_eq!(
            jars,
            [
                ("m:soul_jar", "m:soul_jar_full", 50),
                ("m:jar2", "m:jar2_full", DEFAULT_CAPACITY),
                ("m:jar3", "m:jar3_full", 1),
                ("m:jar4", "m:jar4_full", 3),
            ]
        );
        assert_eq!(packs.jars[0].setting.as_deref(), Some("soul_jar"));
        assert_eq!(problems.len(), 4, "{problems:?}");
    }

    #[test]
    fn a_section_that_is_not_a_list_is_a_problem() {
        let (packs, problems) = parse("m", &json!({"backpacks": {"items": ["m:p"]}}));
        assert_eq!(packs, ModPacks::default());
        assert_eq!(problems, ["m: backpacks is not a list"]);
        let (packs, problems) = parse("m", &json!({"gear": []}));
        assert_eq!(packs, ModPacks::default());
        assert!(problems.is_empty());
    }

    #[test]
    fn each_mod_replaces_its_own_rules() {
        // The one test that touches the rules in force.
        read(
            "a_mod",
            &json!({"backpacks": [{"items": ["a:pack"]}],
            "soul_jars": [{"item": "a:jar", "full": "a:jar_full"}]}),
        );
        read(
            "b_mod",
            &json!({"backpacks": [{"items": ["b:pack", "a:pack"], "rows": 6}]}),
        );
        let p = packs();
        assert_eq!(
            p.backpack("a:pack").map(|b| b.mod_id.as_str()),
            Some("a_mod")
        );
        assert_eq!(p.backpack("b:pack").map(|b| b.rows), Some(6));
        assert_eq!(p.jar("a:jar").map(|j| j.capacity), Some(DEFAULT_CAPACITY));
        assert_eq!(
            p.full_jar("a:jar_full").map(|j| j.item.as_str()),
            Some("a:jar")
        );
        assert!(p.jar("a:jar_full").is_none() && p.backpack("a:jar").is_none());
        assert!(p.has_jars());
        // a_mod's file loses its sections: its rules go, b_mod's stay.
        read("a_mod", &json!({}));
        let p = packs();
        assert_eq!(
            p.backpack("a:pack").map(|b| b.mod_id.as_str()),
            Some("b_mod")
        );
        assert!(!p.has_jars());
    }

    #[test]
    fn bag_ids_never_leave_the_folder() {
        assert!(valid_bag_id("0f3a9c2b11d4e5f6"));
        assert!(valid_bag_id("my-bag_2"));
        for bad in [
            "",
            "../level",
            "..",
            "a/b",
            "a\\b",
            "c:",
            "bag.dat",
            &"a".repeat(65),
        ] {
            assert!(!valid_bag_id(bad), "{bad}");
        }
        assert_eq!(bag_file("0f3a"), "backpacks/0f3a.dat");
    }

    #[test]
    fn a_bag_shows_every_slot_its_file_fills() {
        fn filled(slots: &[usize]) -> impl Iterator<Item = bool> + '_ {
            (0..MAX_SLOTS).map(|i| slots.contains(&i))
        }
        assert_eq!(rows_to_show(1, filled(&[])), 1);
        assert_eq!(rows_to_show(1, filled(&[0, 8])), 1);
        assert_eq!(rows_to_show(1, filled(&[9])), 2);
        assert_eq!(rows_to_show(2, filled(&[26])), 3);
        assert_eq!(rows_to_show(6, filled(&[3])), 6);
        assert_eq!(rows_to_show(3, filled(&[53])), 6);
        assert_eq!(rows_to_show(0, filled(&[])), 1);
        assert_eq!(rows_to_show(9, filled(&[])), 6);
    }

    #[test]
    fn closing_keeps_all_but_backpacks_and_makes_nothing() {
        let original = vec![
            Some("dirt"),
            None,
            Some("m:pack"),
            Some("sword"),
            Some("m:pack"),
        ];
        let refused = |s: &&str| *s == "m:pack";
        let kept = to_keep(&original, refused);
        assert_eq!(kept, [Some("dirt"), None, None, Some("sword"), None]);
        // Not yet taken: a failed write leaves every stack to go back.
        let mut slots = original.clone();
        // Written: what was kept leaves the slots, the backpacks stay to go back.
        take_kept(&mut slots, refused);
        assert_eq!(slots, [None, None, Some("m:pack"), None, Some("m:pack")]);
        // Every stack is in exactly one place.
        for (i, stack) in original.iter().enumerate() {
            let places = usize::from(kept[i].is_some()) + usize::from(slots[i].is_some());
            assert_eq!(places, usize::from(stack.is_some()), "slot {i}");
            assert_eq!(kept[i].or(slots[i]), *stack);
        }
    }

    #[test]
    fn names_for_the_title() {
        assert_eq!(name_key("mymod:small_pack"), "item.mymod.small_pack");
        assert_eq!(name_key("mymod:packs/big"), "item.mymod.packs.big");
        assert_eq!(name_key("bundle"), "item.minecraft.bundle");
        assert_eq!(name_fallback("mymod:small_pack"), "Small Pack");
        assert_eq!(name_fallback("mymod:packs/big__bag"), "Big Bag");
    }

    #[test]
    fn souls_go_to_the_first_single_jar() {
        let is_jar = |item: &str| item == "m:jar";
        let hotbar = |stacks: Vec<Option<(&'static str, u32)>>| {
            stacks
                .into_iter()
                .enumerate()
                .chain([(40, Some(("m:jar", 1)))])
        };
        assert_eq!(
            first_jar(hotbar(vec![None, Some(("m:jar", 1))]), is_jar),
            Some(1)
        );
        assert_eq!(
            first_jar(
                hotbar(vec![
                    Some(("m:jar", 3)),
                    Some(("dirt", 1)),
                    Some(("m:jar", 1))
                ]),
                is_jar
            ),
            Some(2)
        );
        // None in the hotbar: the off hand's.
        assert_eq!(first_jar(hotbar(vec![None; 9]), is_jar), Some(40));
        assert_eq!(first_jar([(0, Some(("dirt", 1)))], is_jar), None);
    }

    #[test]
    fn a_jar_fills_to_its_capacity() {
        assert_eq!(add_soul(0, 50), Soul::Count(1));
        assert_eq!(add_soul(48, 50), Soul::Count(49));
        assert_eq!(add_soul(49, 50), Soul::Full);
        // A capacity lowered since: full at once.
        assert_eq!(add_soul(70, 50), Soul::Full);
        assert_eq!(add_soul(0, 1), Soul::Full);
        assert_eq!(souls_of(None), 0);
        assert_eq!(souls_of(Some(-4)), 0);
        assert_eq!(souls_of(Some(12)), 12);
    }

    #[test]
    fn the_fill_bar() {
        assert_eq!(bar_width(0, 50), 0);
        assert_eq!(bar_width(1, 50), 1);
        assert_eq!(bar_width(12, 50), 3);
        assert_eq!(bar_width(25, 50), 6);
        assert_eq!(bar_width(49, 50), 12);
        assert_eq!(bar_width(50, 50), 13);
        assert_eq!(bar_width(900, 50), 13);
        assert_eq!(bar_width(MAX_CAPACITY, MAX_CAPACITY), 13);
    }
}
