// Trinket slots: what the content mods declare under `trinket_slots` and `trinkets` in their
// `data/<mod>/armory_lib.json`, and the sums the game runs them with (which modifiers apply,
// the slots' place in the menu's row, what a closing menu keeps). Added to the game as the
// module `game::armory_lib_trinkets_logic`, which `trinkets.rs` (the game's side) and `hooks.rs`
// (reading the files) reach. It is also a test target of the repository's Cargo package
// (`cargo test --test trinkets`), so nothing from the game is in it.
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The command the trinkets key sends, and the server runs.
pub const COMMAND: &str = "trinkets";
/// The slots of the menu's one row (a `generic_9x1` chest).
pub const ROW: usize = 9;
/// Fewest and most trinket slots, and how many when a file names no number.
pub const MIN_SLOTS: usize = 1;
pub const MAX_SLOTS: usize = 7;
pub const DEFAULT_SLOTS: usize = 3;
/// The key that opens them when no file names one.
pub const DEFAULT_KEY: &str = "key.keyboard.g";
/// Largest modifier amount either way (`max_health` itself stops at 1024).
pub const MAX_AMOUNT: f64 = 1024.0;
/// Largest experience bonus, as an armour set's: ten times the points.
pub const MAX_EXPERIENCE_BONUS: f64 = 10.0;

// ---------------------------------------------------------------------------------------
// What a mod declares
// ---------------------------------------------------------------------------------------

/// How a modifier changes its attribute, as vanilla's `AttributeModifier.Operation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Adds the amount to the base.
    AddValue,
    /// Adds the amount times the base (with the added values).
    AddMultipliedBase,
    /// Multiplies the total by one plus the amount.
    AddMultipliedTotal,
}

impl Operation {
    /// Vanilla's name for it in data (`add_value`...).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "add_value" => Some(Self::AddValue),
            "add_multiplied_base" => Some(Self::AddMultipliedBase),
            "add_multiplied_total" => Some(Self::AddMultipliedTotal),
            _ => None,
        }
    }
}

/// One attribute modifier a trinket gives.
#[derive(Clone, Debug, PartialEq)]
pub struct ModifierDef {
    /// `minecraft:max_health`.
    pub attribute: String,
    pub amount: f64,
    pub operation: Operation,
}

/// A `trinkets` entry: items that work in a trinket slot, and what they do there.
#[derive(Clone, Debug, PartialEq)]
pub struct TrinketDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub items: Vec<String>,
    pub modifiers: Vec<ModifierDef>,
    /// Extra experience from orbs: 0.25 is a quarter more.
    pub experience_bonus: f32,
}

/// A `trinket_slots` declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotsDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub slots: usize,
    pub key: Option<String>,
}

/// What one mod's file declares for the trinkets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub slots: Option<SlotsDef>,
    pub trinkets: Vec<TrinketDef>,
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

/// `id` with `minecraft:` in front when it has no namespace.
fn namespaced(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

/// Whether `id` is a resource location vanilla accepts (`namespace:path`, lower case).
fn valid_id(id: &str) -> bool {
    let Some((namespace, path)) = id.split_once(':') else {
        return false;
    };
    let ok = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c);
    !namespace.is_empty()
        && !path.is_empty()
        && namespace.bytes().all(ok)
        && path.bytes().all(|c| ok(c) || c == b'/')
}

/// The modifier `m` of mod `mod_id`'s trinket `item`, or why it can't be used.
fn read_modifier(mod_id: &str, item: &str, m: &serde_json::Value) -> Result<ModifierDef, String> {
    let Some(attribute) = m.get("attribute").and_then(|v| v.as_str()).map(namespaced) else {
        return Err(format!("{mod_id}: a modifier of trinket {item} needs an attribute"));
    };
    if !valid_id(&attribute) {
        return Err(format!("{mod_id}: trinket {item}'s attribute {attribute:?} is not an id"));
    }
    let Some(amount) = m.get("amount").and_then(|v| v.as_f64()).filter(|a| a.is_finite()) else {
        return Err(format!("{mod_id}: trinket {item}'s {attribute} modifier needs an amount"));
    };
    let operation = match m.get("operation") {
        None => Operation::AddValue,
        Some(v) => match v.as_str().and_then(Operation::parse) {
            Some(op) => op,
            None => {
                return Err(format!(
                    "{mod_id}: trinket {item}'s {attribute} modifier has operation {v}, not \
                     add_value, add_multiplied_base or add_multiplied_total"
                ));
            }
        },
    };
    Ok(ModifierDef { attribute, amount, operation })
}

/// `value` within `min..=max`, with a problem line when it was not.
fn clamped(value: f64, min: f64, max: f64, what: impl FnOnce() -> String, problems: &mut Vec<String>) -> f64 {
    let c = value.clamp(min, max);
    if c != value {
        problems.push(format!("{} is {value}, outside {min} to {max}: {c} is used", what()));
    }
    c
}

/// Mod `mod_id`'s `trinket_slots` and `trinkets` (the README has the format), and what it could
/// not use, one line each, for the log.
pub fn read(mod_id: &str, file: &serde_json::Value) -> (Section, Vec<String>) {
    let mut problems = Vec::new();
    let mut section = Section::default();
    if let Some(decl) = file.get("trinket_slots") {
        if decl.is_object() {
            let slots = match decl.get("slots") {
                None => DEFAULT_SLOTS,
                Some(v) => match v.as_f64().filter(|n| n.is_finite()) {
                    Some(n) => {
                        let whole = clamped(
                            n.round(),
                            MIN_SLOTS as f64,
                            MAX_SLOTS as f64,
                            || format!("{mod_id}: trinket_slots slots"),
                            &mut problems,
                        );
                        if n.fract() != 0.0 {
                            problems.push(format!(
                                "{mod_id}: trinket_slots slots {n} is not a whole number: {whole} is used"
                            ));
                        }
                        whole as usize
                    }
                    None => {
                        problems.push(format!(
                            "{mod_id}: trinket_slots slots must be a number: {DEFAULT_SLOTS} is used"
                        ));
                        DEFAULT_SLOTS
                    }
                },
            };
            let key = match decl.get("key") {
                None => None,
                Some(v) => match v.as_str().filter(|k| k.starts_with("key.") && k.len() > 4) {
                    Some(key) => Some(key.to_owned()),
                    None => {
                        problems.push(format!(
                            "{mod_id}: trinket_slots key {v} is not a key name such as {DEFAULT_KEY:?}"
                        ));
                        None
                    }
                },
            };
            section.slots = Some(SlotsDef {
                mod_id: mod_id.to_owned(),
                setting: text(decl, "setting"),
                slots,
                key,
            });
        } else {
            problems.push(format!("{mod_id}: trinket_slots must be an object"));
        }
    }
    let Some(list) = file.get("trinkets") else {
        return (section, problems);
    };
    let Some(list) = list.as_array() else {
        problems.push(format!("{mod_id}: trinkets must be a list"));
        return (section, problems);
    };
    for entry in list {
        let items: Vec<String> = entry
            .get("items")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|i| i.as_str())
            .filter(|i| !i.is_empty())
            .map(namespaced)
            .collect();
        let Some(first) = items.first().cloned() else {
            problems.push(format!("{mod_id}: a trinkets entry has no items"));
            continue;
        };
        let mut modifiers = Vec::new();
        for m in entry.get("modifiers").and_then(|v| v.as_array()).into_iter().flatten() {
            match read_modifier(mod_id, &first, m) {
                Ok(mut m) => {
                    m.amount = clamped(
                        m.amount,
                        -MAX_AMOUNT,
                        MAX_AMOUNT,
                        || format!("{mod_id}: trinket {first}'s {} amount", m.attribute),
                        &mut problems,
                    );
                    modifiers.push(m);
                }
                Err(problem) => problems.push(problem),
            }
        }
        let experience_bonus = match entry.get("experience_bonus") {
            None => 0.0,
            Some(v) => match v.as_f64().filter(|b| b.is_finite()) {
                Some(b) => clamped(
                    b,
                    0.0,
                    MAX_EXPERIENCE_BONUS,
                    || format!("{mod_id}: trinket {first}'s experience_bonus"),
                    &mut problems,
                ),
                None => {
                    problems.push(format!("{mod_id}: trinket {first}'s experience_bonus must be a number"));
                    0.0
                }
            },
        };
        section.trinkets.push(TrinketDef {
            mod_id: mod_id.to_owned(),
            setting: text(entry, "setting"),
            items,
            modifiers,
            experience_bonus: experience_bonus as f32,
        });
    }
    (section, problems)
}

// ---------------------------------------------------------------------------------------
// Every mod's together
// ---------------------------------------------------------------------------------------

/// A modifier as the library puts it on a player: its attribute, and its own id
/// (`armory_lib:trinket/<n>`, one per trinket item and modifier).
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub id: String,
    pub attribute: String,
    pub amount: f64,
    pub operation: Operation,
}

/// The trinkets of every mod, as one set of slots.
#[derive(Clone, Debug, Default)]
pub struct Trinkets {
    /// How many slots: the most any file declares; 0 when none declares `trinket_slots`.
    pub slots: usize,
    /// The key that opens them: the first a file names (by mod id), else G.
    pub key: String,
    /// The mods that declare the slots, with the switch each names: the slots are offered
    /// while one of these is in force.
    pub declared_by: Vec<(String, Option<String>)>,
    pub defs: Vec<TrinketDef>,
    /// Each trinket item: the entry it belongs to (the first that lists it) and its modifiers.
    by_item: HashMap<String, (usize, Vec<Applied>)>,
}

impl Trinkets {
    /// The mods' sections, in mod id order, put together. Also the problems, each with the
    /// mod whose entry lost: an item two entries list is the first one's.
    pub fn combine(sections: &BTreeMap<String, Section>) -> (Self, Vec<(String, String)>) {
        let mut all = Trinkets {
            key: DEFAULT_KEY.to_owned(),
            ..Default::default()
        };
        let mut problems = Vec::new();
        let mut key = None;
        let mut n = 0;
        for section in sections.values() {
            if let Some(s) = &section.slots {
                all.slots = all.slots.max(s.slots);
                key = key.or_else(|| s.key.clone());
                all.declared_by.push((s.mod_id.clone(), s.setting.clone()));
            }
            for def in &section.trinkets {
                let index = all.defs.len();
                for item in &def.items {
                    if let Some((other, _)) = all.by_item.get(item) {
                        let other = &all.defs[*other].mod_id;
                        problems.push((
                            def.mod_id.clone(),
                            format!("{}: {item} is already a trinket of {other}; that entry is used", def.mod_id),
                        ));
                        continue;
                    }
                    let applied = def
                        .modifiers
                        .iter()
                        .map(|m| {
                            n += 1;
                            Applied {
                                id: format!("armory_lib:trinket/{n}"),
                                attribute: m.attribute.clone(),
                                amount: m.amount,
                                operation: m.operation,
                            }
                        })
                        .collect();
                    all.by_item.insert(item.clone(), (index, applied));
                }
                all.defs.push(def.clone());
            }
        }
        if let Some(key) = key {
            all.key = key;
        }
        (all, problems)
    }

    /// Whether `item` goes in a trinket slot.
    pub fn is_trinket(&self, item: &str) -> bool {
        self.by_item.contains_key(item)
    }

    /// The trinkets of `slots` (item ids, a copy counting once) whose rule is `on`, as long
    /// as `off_hand` doesn't hold the same item too.
    fn worn<'a>(
        &'a self,
        slots: &[&'a str],
        on: &impl Fn(&TrinketDef) -> bool,
    ) -> Vec<(&'a str, &'a TrinketDef, &'a [Applied])> {
        let mut found: Vec<(&str, &TrinketDef, &[Applied])> = Vec::new();
        for &item in slots {
            if found.iter().any(|(i, _, _)| *i == item) {
                continue;
            }
            if let Some((def, applied)) = self.by_item.get(item)
                && on(&self.defs[*def])
            {
                found.push((item, &self.defs[*def], applied));
            }
        }
        found
    }

    /// The modifiers the trinkets in `slots` give, a copy counting once. One the off hand
    /// holds too gives none here: its item has off-hand modifiers of its own, as vanilla gives
    /// them, and two would count it twice.
    pub fn applied(
        &self,
        slots: &[&str],
        off_hand: Option<&str>,
        on: &impl Fn(&TrinketDef) -> bool,
    ) -> Vec<Applied> {
        self.worn(slots, on)
            .into_iter()
            .filter(|(item, _, _)| Some(*item) != off_hand)
            .flat_map(|(_, _, applied)| applied.iter().cloned())
            .collect()
    }

    /// The experience bonus of the trinkets in `slots` and the one in the off hand, added
    /// together, each item once.
    pub fn experience_bonus(
        &self,
        slots: &[&str],
        off_hand: Option<&str>,
        on: &impl Fn(&TrinketDef) -> bool,
    ) -> f32 {
        let mut items = slots.to_vec();
        items.extend(off_hand);
        self.worn(&items, on).iter().map(|(_, def, _)| def.experience_bonus).sum()
    }
}

/// Each mod's section, by mod id, as its file was last read.
static SECTIONS: Mutex<BTreeMap<String, Section>> = Mutex::new(BTreeMap::new());
/// All of them together.
static TRINKETS: RwLock<Option<Arc<Trinkets>>> = RwLock::new(None);

/// Reads mod `mod_id`'s file (`hooks.rs`, as it reads the mods' files) and puts its trinkets
/// in force in place of what its file declared before. Returns what it could not use.
pub fn load_file(mod_id: &str, file: &serde_json::Value) -> Vec<String> {
    let (section, mut problems) = read(mod_id, file);
    let mut sections = lock(&SECTIONS);
    if section == Section::default() {
        sections.remove(mod_id);
    } else {
        sections.insert(mod_id.to_owned(), section);
    }
    let (all, lost) = Trinkets::combine(&sections);
    problems.extend(lost.into_iter().filter(|(m, _)| m == mod_id).map(|(_, p)| p));
    *TRINKETS.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(all));
    problems
}

/// The trinkets in force.
pub fn trinkets() -> Arc<Trinkets> {
    TRINKETS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| {
            Arc::new(Trinkets {
                key: DEFAULT_KEY.to_owned(),
                ..Default::default()
            })
        })
}

// ---------------------------------------------------------------------------------------
// The menu
// ---------------------------------------------------------------------------------------

/// The menu's input slots for `slots` trinket slots, in the middle of its row (3: slots 3, 4
/// and 5). `stored` is how far the player's file reaches (the last kept position plus one):
/// when there are fewer slots than it holds (a mod declares fewer now), the row widens to show
/// them all, so nothing stays hidden in the file.
pub fn input_slots(slots: usize, stored: usize) -> Range<usize> {
    let count = slots.max(stored).clamp(MIN_SLOTS, ROW);
    let start = (ROW - count) / 2;
    start..start + count
}

/// What a closing menu keeps, from its input slots `items` (each an item id, or None for an
/// empty slot): one of each trinket, as (the slot it is taken from, its position among the
/// `slots` slots), in slot order and no more than `slots` of them. A trinket keeps its own
/// position; one past the slots (a row widened by `input_slots`) takes the first free one.
pub fn keep_plan(
    items: &[Option<&str>],
    slots: usize,
    is_trinket: impl Fn(&str) -> bool,
) -> Vec<(usize, usize)> {
    let picked: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.is_some_and(&is_trinket))
        .map(|(i, _)| i)
        .take(slots)
        .collect();
    let mut free = (0..slots).filter(|p| !picked.contains(p));
    picked
        .iter()
        .map(|&i| (i, if i < slots { Some(i) } else { free.next() }))
        .filter_map(|(i, to)| Some((i, to?)))
        .collect()
}

/// A UUID as vanilla writes it in file names: `8-4-4-4-12` lower-case hex.
pub fn uuid_text(uuid: u128) -> String {
    let h = format!("{uuid:032x}");
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// The player's file, in the library's folder of the world.
pub fn file_name(uuid: u128) -> String {
    format!("trinkets/{}.dat", uuid_text(uuid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn section(mod_id: &str, file: serde_json::Value) -> Section {
        let (section, problems) = read(mod_id, &file);
        assert!(problems.is_empty(), "{problems:?}");
        section
    }

    fn ring_file() -> serde_json::Value {
        json!({
            "trinket_slots": {"slots": 3, "key": "key.keyboard.g", "setting": "trinkets"},
            "trinkets": [
                {"items": ["mymod:ring"],
                 "modifiers": [{"attribute": "minecraft:max_health", "amount": 4.0, "operation": "add_value"}],
                 "experience_bonus": 0.25, "setting": "trinkets"},
                {"items": ["mymod:charm", "mymod:charm_gold"],
                 "modifiers": [{"attribute": "movement_speed", "amount": 0.1, "operation": "add_multiplied_base"},
                               {"attribute": "minecraft:armor", "amount": 2}]}
            ]
        })
    }

    fn all_of(files: &[(&str, serde_json::Value)]) -> Trinkets {
        let sections = files
            .iter()
            .map(|(id, f)| (id.to_string(), section(id, f.clone())))
            .collect();
        Trinkets::combine(&sections).0
    }

    #[test]
    fn reads_the_format() {
        let s = section("mymod", ring_file());
        assert_eq!(
            s.slots,
            Some(SlotsDef {
                mod_id: "mymod".into(),
                setting: Some("trinkets".into()),
                slots: 3,
                key: Some("key.keyboard.g".into()),
            })
        );
        assert_eq!(s.trinkets.len(), 2);
        let ring = &s.trinkets[0];
        assert_eq!(ring.items, ["mymod:ring"]);
        assert_eq!(ring.setting.as_deref(), Some("trinkets"));
        assert_eq!(ring.experience_bonus, 0.25);
        assert_eq!(
            ring.modifiers,
            [ModifierDef {
                attribute: "minecraft:max_health".into(),
                amount: 4.0,
                operation: Operation::AddValue,
            }]
        );
        // An attribute without its namespace is vanilla's; no operation is add_value.
        let charm = &s.trinkets[1];
        assert_eq!(charm.modifiers[0].attribute, "minecraft:movement_speed");
        assert_eq!(charm.modifiers[0].operation, Operation::AddMultipliedBase);
        assert_eq!(charm.modifiers[1].operation, Operation::AddValue);
        assert_eq!(charm.experience_bonus, 0.0);
        assert_eq!(charm.setting, None);
    }

    #[test]
    fn defaults_and_nothing_declared() {
        let (s, problems) = read("m", &json!({"trinket_slots": {}}));
        assert!(problems.is_empty());
        let slots = s.slots.unwrap();
        assert_eq!((slots.slots, slots.key, slots.setting), (DEFAULT_SLOTS, None, None));
        let (s, problems) = read("m", &json!({"gear_lights": []}));
        assert_eq!((s, problems.len()), (Section::default(), 0));
    }

    #[test]
    fn clamps_and_reports() {
        let (s, problems) = read(
            "m",
            &json!({
                "trinket_slots": {"slots": 12, "key": "g"},
                "trinkets": [
                    {"items": []},
                    {"items": ["m:a"], "experience_bonus": 50,
                     "modifiers": [
                        {"attribute": "minecraft:max_health", "amount": 5000},
                        {"attribute": "minecraft:max_health"},
                        {"amount": 1},
                        {"attribute": "Bad Name", "amount": 1},
                        {"attribute": "minecraft:armor", "amount": 1, "operation": "multiply"}
                     ]}
                ]
            }),
        );
        let slots = s.slots.unwrap();
        assert_eq!(slots.slots, MAX_SLOTS);
        assert_eq!(slots.key, None);
        assert_eq!(s.trinkets.len(), 1);
        let a = &s.trinkets[0];
        assert_eq!(a.experience_bonus, 10.0);
        assert_eq!(a.modifiers.len(), 1);
        assert_eq!(a.modifiers[0].amount, MAX_AMOUNT);
        // Slots, key, the empty entry, the bonus, and five modifiers (one clamped, four unusable).
        assert_eq!(problems.len(), 9, "{problems:#?}");
        assert!(problems.iter().all(|p| p.starts_with("m: ")));

        let (s, problems) = read("m", &json!({"trinket_slots": {"slots": 0.4}, "trinkets": {}}));
        assert_eq!(s.slots.unwrap().slots, MIN_SLOTS);
        assert_eq!(problems.len(), 3, "{problems:#?}");
        let (s, problems) = read("m", &json!({"trinket_slots": {"slots": "four"}}));
        assert_eq!(s.slots.unwrap().slots, DEFAULT_SLOTS);
        assert_eq!(problems.len(), 1);
        let (s, problems) = read("m", &json!({"trinket_slots": 3}));
        assert_eq!((s.slots, problems.len()), (None, 1));
    }

    #[test]
    fn one_set_of_slots_for_every_mod() {
        let all = all_of(&[
            ("bmod", json!({"trinket_slots": {"slots": 5, "key": "key.keyboard.h", "setting": "s"}})),
            ("amod", json!({"trinket_slots": {"slots": 2}})),
            ("cmod", json!({"trinket_slots": {"slots": 4, "key": "key.keyboard.j"}})),
        ]);
        // The most slots; the first key by mod id (amod names none).
        assert_eq!(all.slots, 5);
        assert_eq!(all.key, "key.keyboard.h");
        assert_eq!(
            all.declared_by,
            [("amod".into(), None), ("bmod".into(), Some("s".into())), ("cmod".into(), None)]
        );
        let none = all_of(&[("m", json!({"trinkets": [{"items": ["m:a"]}]}))]);
        assert_eq!((none.slots, none.key.as_str()), (0, DEFAULT_KEY));
        assert!(none.is_trinket("m:a"));
    }

    #[test]
    fn ids_are_unique_and_valid() {
        let all = all_of(&[("mymod", ring_file())]);
        let on = |_: &TrinketDef| true;
        let applied = all.applied(&["mymod:ring", "mymod:charm", "mymod:charm_gold"], None, &on);
        let ids: Vec<&str> = applied.iter().map(|a| a.id.as_str()).collect();
        // One per trinket item and modifier: the ring's one, each charm's two.
        assert_eq!(ids.len(), 5);
        for (i, id) in ids.iter().enumerate() {
            assert!(valid_id(id), "{id}");
            assert!(id.starts_with("armory_lib:trinket/"));
            assert!(!ids[i + 1..].contains(id), "{id} twice");
        }
    }

    #[test]
    fn which_modifiers_apply() {
        let all = all_of(&[("mymod", ring_file())]);
        let on = |_: &TrinketDef| true;
        let ring = all.applied(&["mymod:ring"], None, &on);
        assert_eq!(ring.len(), 1);
        assert_eq!((ring[0].attribute.as_str(), ring[0].amount), ("minecraft:max_health", 4.0));
        // Two copies of one trinket count once.
        assert_eq!(all.applied(&["mymod:ring", "mymod:ring"], None, &on), ring);
        // One also in the off hand gives nothing here; the others still do.
        assert!(all.applied(&["mymod:ring"], Some("mymod:ring"), &on).is_empty());
        assert_eq!(all.applied(&["mymod:ring", "mymod:charm"], Some("mymod:ring"), &on).len(), 2);
        // An item that isn't a trinket, and a rule not in force, give nothing.
        assert!(all.applied(&["minecraft:stick"], None, &on).is_empty());
        let charms_off = |d: &TrinketDef| d.setting.is_some();
        assert_eq!(all.applied(&["mymod:ring", "mymod:charm"], None, &charms_off), ring);
    }

    #[test]
    fn experience_bonuses_add_up() {
        let all = all_of(&[(
            "m",
            json!({"trinkets": [
                {"items": ["m:a"], "experience_bonus": 0.25},
                {"items": ["m:b"], "experience_bonus": 0.5, "setting": "b"},
                {"items": ["m:c"]}
            ]}),
        )]);
        let on = |_: &TrinketDef| true;
        assert_eq!(all.experience_bonus(&["m:a", "m:b", "m:c"], None, &on), 0.75);
        // The off hand counts too, an item once.
        assert_eq!(all.experience_bonus(&["m:a"], Some("m:b"), &on), 0.75);
        assert_eq!(all.experience_bonus(&["m:a", "m:a"], Some("m:a"), &on), 0.25);
        assert_eq!(all.experience_bonus(&[], Some("m:a"), &on), 0.25);
        let b_off = |d: &TrinketDef| d.setting.is_none();
        assert_eq!(all.experience_bonus(&["m:a", "m:b"], None, &b_off), 0.25);
    }

    #[test]
    fn an_item_is_the_first_entrys() {
        let sections: BTreeMap<String, Section> = [
            ("amod", json!({"trinkets": [{"items": ["x:ring"], "experience_bonus": 1}]})),
            ("bmod", json!({"trinkets": [{"items": ["x:ring", "b:own"], "experience_bonus": 2}]})),
        ]
        .into_iter()
        .map(|(id, f)| (id.to_owned(), section(id, f)))
        .collect();
        let (all, problems) = Trinkets::combine(&sections);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].0, "bmod");
        let on = |_: &TrinketDef| true;
        assert_eq!(all.experience_bonus(&["x:ring"], None, &on), 1.0);
        assert_eq!(all.experience_bonus(&["b:own"], None, &on), 2.0);
    }

    #[test]
    fn slot_layout() {
        assert_eq!(input_slots(3, 0), 3..6);
        assert_eq!(input_slots(1, 0), 4..5);
        assert_eq!(input_slots(2, 1), 3..5);
        assert_eq!(input_slots(5, 0), 2..7);
        assert_eq!(input_slots(7, 7), 1..8);
        // A file that reaches further than the slots widens the row.
        assert_eq!(input_slots(3, 5), 2..7);
        assert_eq!(input_slots(0, 0), 4..5);
        assert_eq!(input_slots(3, 20), 0..9);
        for n in MIN_SLOTS..=MAX_SLOTS {
            let r = input_slots(n, 0);
            assert_eq!(r.len(), n);
            // Centred: as many slots to the left as to the right, or one more to the right.
            let (left, right) = (r.start, ROW - r.end);
            assert!(right == left || right == left + 1, "{n}: {r:?}");
        }
    }

    #[test]
    fn what_a_closing_menu_keeps() {
        let is = |i: &str| i.starts_with("m:");
        // Trinkets keep their positions; anything else stays to go back.
        let items = [Some("m:a"), Some("minecraft:dirt"), Some("m:b")];
        assert_eq!(keep_plan(&items, 3, is), [(0, 0), (2, 2)]);
        assert_eq!(keep_plan(&[None, None, None], 3, is), []);
        // No more than there are slots.
        let items = [Some("m:a"), Some("m:b"), Some("m:c")];
        assert_eq!(keep_plan(&items, 2, is), [(0, 0), (1, 1)]);
        // A widened row: the ones past the slots move into the free ones.
        let items = [None, Some("x:no"), Some("m:a"), Some("m:b"), Some("m:c")];
        assert_eq!(keep_plan(&items, 3, is), [(2, 2), (3, 0), (4, 1)]);
        assert_eq!(keep_plan(&items, 0, is), []);
        // Two copies are both kept (each in a slot of its own).
        assert_eq!(keep_plan(&[Some("m:a"), Some("m:a")], 3, is), [(0, 0), (1, 1)]);
    }

    #[test]
    fn file_names() {
        let uuid = 0x069a79f4_44e9_4726_a5be_fca90e38aaf5_u128;
        assert_eq!(uuid_text(uuid), "069a79f4-44e9-4726-a5be-fca90e38aaf5");
        assert_eq!(file_name(1), "trinkets/00000000-0000-0000-0000-000000000001.dat");
    }

    #[test]
    fn loading_replaces_a_mods_trinkets() {
        // The one test that uses the statics.
        let problems = load_file("zz_test_mod", &ring_file());
        assert!(problems.is_empty(), "{problems:?}");
        assert!(trinkets().is_trinket("mymod:ring"));
        assert_eq!(trinkets().slots, 3);
        load_file("zz_test_mod", &json!({"trinkets": [{"items": ["mymod:other"]}]}));
        assert!(!trinkets().is_trinket("mymod:ring"));
        assert!(trinkets().is_trinket("mymod:other"));
        assert_eq!(trinkets().slots, 0);
        load_file("zz_test_mod", &json!({}));
        assert!(trinkets().defs.is_empty());
    }
}
