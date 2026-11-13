// The soul forge and its upgrades, with nothing from the game in them: what content mods declare
// under `soul_forge` in their `data/<mod>/armory_lib.json` (the README has the format), the forge's
// rules and the upgrades' sums. Added to the game as `game::armory_lib_forge_logic` for
// `armory_lib_forge` (forge.rs); it is also the `forge` test target, so `cargo test` runs it.
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// The custom data key of an item's upgrades: a list of their names.
pub const UPGRADES_KEY: &str = "armory_lib:upgrades";
/// The level costs of an item's first, second and third upgrade when a file names none.
pub const DEFAULT_LEVELS: [u32; 3] = [5, 10, 15];
/// The most upgrades a file may allow on one item.
pub const MAX_UPGRADES: usize = 6;

/// The six upgrades.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Upgrade {
    /// More attack damage (`amount`, 2).
    Sharpened,
    /// The share of wear the item still takes (`amount`, 0.5).
    Reinforced,
    /// Faster attacks for a weapon, faster mining for a tool (`amount`, 0.1 for 10%).
    Swift,
    /// Less damage taken for each upgraded piece worn (`amount`, 0.08 for 8%).
    Warded,
    /// Kept through death.
    Soulbound,
    /// Lights the world while held or worn (`amount`, the light level, 12).
    Radiant,
}

pub const ALL: [Upgrade; 6] = [
    Upgrade::Sharpened,
    Upgrade::Reinforced,
    Upgrade::Swift,
    Upgrade::Warded,
    Upgrade::Soulbound,
    Upgrade::Radiant,
];

impl Upgrade {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sharpened => "sharpened",
            Self::Reinforced => "reinforced",
            Self::Swift => "swift",
            Self::Warded => "warded",
            Self::Soulbound => "soulbound",
            Self::Radiant => "radiant",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        ALL.into_iter().find(|u| u.name() == name)
    }

    /// The tooltip line: `Sharpened`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Sharpened => "Sharpened",
            Self::Reinforced => "Reinforced",
            Self::Swift => "Swift",
            Self::Warded => "Warded",
            Self::Soulbound => "Soulbound",
            Self::Radiant => "Radiant",
        }
    }

    /// The owner's numbers: +2 damage, half the wear, 10% faster, 8% less damage a piece,
    /// light 12.
    pub fn default_amount(self) -> f32 {
        match self {
            Self::Sharpened => 2.0,
            Self::Reinforced => 0.5,
            Self::Swift => 0.1,
            Self::Warded => 0.08,
            Self::Soulbound => 0.0,
            Self::Radiant => 12.0,
        }
    }

    /// What `amount` may be.
    fn range(self) -> (f32, f32) {
        match self {
            Self::Sharpened => (0.0, 100.0),
            Self::Reinforced => (0.0, 1.0),
            Self::Swift => (0.0, 10.0),
            Self::Warded => (0.0, 0.25),
            Self::Soulbound => (0.0, 0.0),
            Self::Radiant => (1.0, 15.0),
        }
    }

    /// The owner's rule: weapons take sharpened and swift, tools swift, armour warded, and
    /// everything reinforced, soulbound and radiant.
    pub fn default_kinds(self) -> Vec<Kind> {
        match self {
            Self::Sharpened => vec![Kind::Weapons],
            Self::Swift => vec![Kind::Weapons, Kind::Tools],
            Self::Warded => vec![Kind::Armour],
            _ => vec![Kind::All],
        }
    }
}

/// What an item is, for the upgrades it may take. Every item is `All`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Weapons,
    Tools,
    Armour,
    All,
}

impl Kind {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "weapons" => Some(Self::Weapons),
            "tools" => Some(Self::Tools),
            "armour" => Some(Self::Armour),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Weapons => "weapons",
            Self::Tools => "tools",
            Self::Armour => "armour",
            Self::All => "anything",
        }
    }
}

/// One upgrade a forge offers: the relic that makes it, the kinds of item it fits, its number.
#[derive(Clone, Debug, PartialEq)]
pub struct UpgradeDef {
    pub upgrade: Upgrade,
    pub relic: String,
    pub kinds: Vec<Kind>,
    pub amount: f32,
}

/// A mod's soul forge: its blocks, what pays (a full jar, which comes back empty, or the
/// tome), the levels each upgrade costs (their count is how many an item may have), what
/// counts as a weapon, a tool and armour (item ids, or item tags as `#minecraft:swords`), and
/// its upgrades.
#[derive(Clone, Debug, PartialEq)]
pub struct ForgeDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub blocks: Vec<String>,
    pub full_jar: String,
    pub empty_jar: String,
    pub tome: Option<String>,
    pub levels: Vec<u32>,
    pub weapons: Vec<String>,
    pub tools: Vec<String>,
    pub armour: Vec<String>,
    pub upgrades: Vec<UpgradeDef>,
}

impl ForgeDef {
    /// The upgrade relic `item` makes.
    pub fn relic(&self, item: &str) -> Option<&UpgradeDef> {
        self.upgrades.iter().find(|u| u.relic == item)
    }

    /// The number of `upgrade`: this forge's, or the default.
    pub fn amount(&self, upgrade: Upgrade) -> f32 {
        self.upgrades
            .iter()
            .find(|u| u.upgrade == upgrade)
            .map_or(upgrade.default_amount(), |u| u.amount)
    }

    /// The kinds of the item `item`: `All`, and each list naming it (or a tag `in_tag` says
    /// holds it).
    pub fn kinds_of(&self, item: &str, in_tag: impl Fn(&str) -> bool) -> Vec<Kind> {
        let listed = |list: &[String]| {
            list.iter().any(|entry| match entry.strip_prefix('#') {
                Some(tag) => in_tag(tag),
                None => entry == item,
            })
        };
        let mut kinds = vec![Kind::All];
        for (list, kind) in [
            (&self.weapons, Kind::Weapons),
            (&self.tools, Kind::Tools),
            (&self.armour, Kind::Armour),
        ] {
            if listed(list) {
                kinds.push(kind);
            }
        }
        kinds
    }
}

/// Every mod's forge, by mod id.
#[derive(Debug, Default)]
pub struct Forges {
    by_mod: BTreeMap<String, ForgeDef>,
}

impl Forges {
    /// The forge whose block is `block`; the mod whose id sorts first where two name it.
    pub fn by_block(&self, block: &str) -> Option<&ForgeDef> {
        self.by_mod.values().find(|f| f.blocks.iter().any(|b| b == block))
    }

    pub fn get(&self, mod_id: &str) -> Option<&ForgeDef> {
        self.by_mod.get(mod_id)
    }

    /// The forges, by mod id.
    pub fn all(&self) -> impl Iterator<Item = &ForgeDef> {
        self.by_mod.values()
    }

    pub fn is_empty(&self) -> bool {
        self.by_mod.is_empty()
    }
}

static FORGES: RwLock<Option<Arc<Forges>>> = RwLock::new(None);

/// The forges in force.
pub fn forges() -> Arc<Forges> {
    FORGES
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Reads mod `mod_id`'s `soul_forge` and puts it in force in place of what that mod declared
/// before (`hooks.rs` calls it for each mod each time the files are read). Returns what it could
/// not use, one line each.
pub fn read(mod_id: &str, file: &serde_json::Value) -> Vec<String> {
    let (declared, problems) = parse(mod_id, file);
    let mut slot = FORGES.write().unwrap_or_else(|e| e.into_inner());
    let mut by_mod = slot.as_ref().map(|f| f.by_mod.clone()).unwrap_or_default();
    match declared {
        Some(def) => by_mod.insert(mod_id.to_owned(), def),
        None => by_mod.remove(mod_id),
    };
    *slot = Some(Arc::new(Forges { by_mod }));
    problems
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

fn strings(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// What mod `mod_id`'s file declares under `soul_forge` (None for no forge), with defaults filled
/// in and numbers clamped, and a line for each thing it could not use as written.
pub fn parse(mod_id: &str, file: &serde_json::Value) -> (Option<ForgeDef>, Vec<String>) {
    let mut problems = Vec::new();
    let Some(section) = file.get("soul_forge") else {
        return (None, problems);
    };
    if !section.is_object() {
        problems.push(format!("{mod_id}: soul_forge is not an object"));
        return (None, problems);
    }
    let blocks = strings(section.get("blocks"));
    let (Some(full_jar), Some(empty_jar)) = (text(section, "full_jar"), text(section, "empty_jar"))
    else {
        problems.push(format!("{mod_id}: soul_forge needs full_jar and empty_jar"));
        return (None, problems);
    };
    if blocks.is_empty() {
        problems.push(format!("{mod_id}: soul_forge has no blocks"));
        return (None, problems);
    }
    let levels = match section.get("levels") {
        None => DEFAULT_LEVELS.to_vec(),
        Some(v) => {
            let list: Option<Vec<u32>> = v.as_array().and_then(|a| {
                a.iter()
                    .map(|n| n.as_u64().filter(|n| *n <= 1000).map(|n| n as u32))
                    .collect()
            });
            match list {
                Some(list) if (1..=MAX_UPGRADES).contains(&list.len()) => list,
                _ => {
                    problems.push(format!(
                        "{mod_id}: soul_forge levels {v} is not a list of 1 to {MAX_UPGRADES} \
                         numbers 0 to 1000; using 5, 10, 15"
                    ));
                    DEFAULT_LEVELS.to_vec()
                }
            }
        }
    };
    let kinds = section.get("kinds");
    let list = |key: &str| strings(kinds.and_then(|k| k.get(key)));
    let mut upgrades: Vec<UpgradeDef> = Vec::new();
    for entry in section
        .get("upgrades")
        .and_then(|u| u.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let name = text(entry, "name").unwrap_or_default();
        let Some(upgrade) = Upgrade::from_name(&name) else {
            problems.push(format!("{mod_id}: unknown upgrade {name:?}"));
            continue;
        };
        let Some(relic) = text(entry, "relic") else {
            problems.push(format!("{mod_id}: upgrade {name} has no relic"));
            continue;
        };
        if upgrades.iter().any(|u| u.upgrade == upgrade || u.relic == relic) {
            problems.push(format!("{mod_id}: upgrade {name} or relic {relic} is there twice"));
            continue;
        }
        let kinds = match entry.get("kinds") {
            None => upgrade.default_kinds(),
            Some(v) => {
                let names = strings(Some(v));
                let kinds: Vec<Kind> = names.iter().filter_map(|n| Kind::from_name(n)).collect();
                if kinds.is_empty() || kinds.len() != names.len() {
                    problems.push(format!(
                        "{mod_id}: upgrade {name} kinds {v} are not weapons, tools, armour or all"
                    ));
                }
                if kinds.is_empty() { upgrade.default_kinds() } else { kinds }
            }
        };
        let (low, high) = upgrade.range();
        let amount = match entry.get("amount") {
            None => upgrade.default_amount(),
            Some(v) => match v.as_f64() {
                Some(n) if (f64::from(low)..=f64::from(high)).contains(&n) => n as f32,
                _ => {
                    let used = v.as_f64().map_or(upgrade.default_amount(), |n| {
                        (n as f32).clamp(low, high)
                    });
                    problems.push(format!(
                        "{mod_id}: upgrade {name} amount {v} is not {low} to {high}; using {used}"
                    ));
                    used
                }
            },
        };
        upgrades.push(UpgradeDef { upgrade, relic, kinds, amount });
    }
    if upgrades.is_empty() {
        problems.push(format!("{mod_id}: soul_forge has no upgrades"));
    }
    let def = ForgeDef {
        mod_id: mod_id.to_owned(),
        setting: text(section, "setting"),
        blocks,
        full_jar,
        empty_jar,
        tome: text(section, "tome"),
        levels,
        weapons: list("weapons"),
        tools: list("tools"),
        armour: list("armour"),
        upgrades,
    };
    (Some(def), problems)
}

// ---------------------------------------------------------------------------------------
// The forge's rule
// ---------------------------------------------------------------------------------------

/// What the forge does when the preview is clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub upgrade: Upgrade,
    /// Levels to take: none with the tome or in creative.
    pub levels: u32,
    /// Paid with the tome rather than a full jar.
    pub tome: bool,
}

/// Why the forge won't.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    NoItem,
    OneItem,
    NoRelic,
    NotARelic,
    Already(Upgrade),
    Full(usize),
    WrongKind(Upgrade, Vec<Kind>),
    NoPayment,
    OneJar,
    Levels(u32),
}

impl Refusal {
    /// What the action bar says.
    pub fn text(&self) -> String {
        match self {
            Self::NoItem => "Put the item to upgrade in the first slot".into(),
            Self::OneItem => "Upgrade one item at a time".into(),
            Self::NoRelic => "Put a relic in the second slot".into(),
            Self::NotARelic => "The forge knows no upgrade for that relic".into(),
            Self::Already(u) => format!("It is {} already", u.name()),
            Self::Full(n) => format!("It has {n} upgrades already"),
            Self::WrongKind(u, kinds) => {
                let nouns: Vec<&str> = kinds.iter().map(|k| k.noun()).collect();
                format!("{} is only for {}", u.label(), nouns.join(" and "))
            }
            Self::NoPayment => "Put a full soul jar or an upgrade tome in the third slot".into(),
            Self::OneJar => "Put one full soul jar in the third slot".into(),
            Self::Levels(n) => format!("You need {n} levels"),
        }
    }

    /// A hint (what is missing) rather than a refusal.
    pub fn is_hint(&self) -> bool {
        matches!(self, Self::NoItem | Self::NoRelic | Self::NoPayment)
    }
}

/// The forge's rule for what lies in its slots: the item (its count, its upgrades, its kinds),
/// the relic, the payment (item and count), the player's levels and whether they are in
/// creative (who pays no levels).
#[allow(clippy::too_many_arguments)]
pub fn plan(
    def: &ForgeDef,
    item: Option<u32>,
    has: &[Upgrade],
    kinds: &[Kind],
    relic: Option<&str>,
    payment: Option<(&str, u32)>,
    levels: i32,
    creative: bool,
) -> Result<Plan, Refusal> {
    let count = item.ok_or(Refusal::NoItem)?;
    if count != 1 {
        return Err(Refusal::OneItem);
    }
    let relic = relic.ok_or(Refusal::NoRelic)?;
    let up = def.relic(relic).ok_or(Refusal::NotARelic)?;
    if has.contains(&up.upgrade) {
        return Err(Refusal::Already(up.upgrade));
    }
    if has.len() >= def.levels.len() {
        return Err(Refusal::Full(has.len()));
    }
    if !up.kinds.iter().any(|k| kinds.contains(k)) {
        return Err(Refusal::WrongKind(up.upgrade, up.kinds.clone()));
    }
    let (paid_with, paid) = payment.ok_or(Refusal::NoPayment)?;
    let tome = def.tome.as_deref() == Some(paid_with);
    if !tome && paid_with != def.full_jar {
        return Err(Refusal::NoPayment);
    }
    if !tome && paid != 1 {
        return Err(Refusal::OneJar);
    }
    let cost = if tome || creative { 0 } else { def.levels[has.len()] };
    if i64::from(levels) < i64::from(cost) {
        return Err(Refusal::Levels(cost));
    }
    Ok(Plan { upgrade: up.upgrade, levels: cost, tome })
}

/// The upgrades named in an item's list, known ones only, each once, in their order.
pub fn upgrades_of<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<Upgrade> {
    let mut out = Vec::new();
    for upgrade in names.into_iter().filter_map(Upgrade::from_name) {
        if !out.contains(&upgrade) {
            out.push(upgrade);
        }
    }
    out
}

/// The wear a reinforced item still takes of `amount` points, keeping the share `kept`: the
/// whole points, and one more when `roll` (0 to 1) falls under what is left.
pub fn kept_wear(amount: i32, kept: f32, roll: f32) -> i32 {
    if amount <= 0 {
        return amount;
    }
    let exact = amount as f32 * kept.clamp(0.0, 1.0);
    let whole = exact.floor();
    whole as i32 + i32::from(roll < exact - whole)
}

/// What still reaches a player wearing `pieces` warded pieces, each taking `per_piece` off.
pub fn warded_share(pieces: usize, per_piece: f32) -> f32 {
    (1.0 - per_piece * pieces as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn forge() -> ForgeDef {
        let file = json!({"soul_forge": {
            "blocks": ["m:forge"], "full_jar": "m:jar_full", "empty_jar": "m:jar",
            "tome": "m:tome", "levels": [5, 10, 15],
            "kinds": {"weapons": ["#minecraft:swords", "m:dagger"], "tools": ["#minecraft:axes"],
                      "armour": ["m:helmet"]},
            "upgrades": [
                {"name": "sharpened", "relic": "m:hilt", "kinds": ["weapons"], "amount": 2},
                {"name": "reinforced", "relic": "m:hammer", "amount": 0.5},
                {"name": "swift", "relic": "m:shard", "kinds": ["weapons", "tools"]},
                {"name": "warded", "relic": "m:ring", "kinds": ["armour"], "amount": 0.08},
                {"name": "soulbound", "relic": "m:gem"},
                {"name": "radiant", "relic": "minecraft:glowstone", "amount": 12}
            ]
        }});
        let (def, problems) = parse("m", &file);
        assert!(problems.is_empty(), "{problems:?}");
        def.unwrap()
    }

    #[test]
    fn reads_a_forge() {
        let f = forge();
        assert_eq!(f.levels, [5, 10, 15]);
        assert_eq!(f.relic("m:hilt").map(|u| u.upgrade), Some(Upgrade::Sharpened));
        assert_eq!(f.relic("minecraft:glowstone").map(|u| u.amount), Some(12.0));
        assert_eq!(f.relic("m:hammer").map(|u| u.kinds.clone()), Some(vec![Kind::All]));
        assert_eq!(f.amount(Upgrade::Swift), 0.1);
        assert_eq!(f.tome.as_deref(), Some("m:tome"));
        assert!(parse("m", &json!({})).0.is_none());
    }

    #[test]
    fn a_bad_forge_says_why() {
        let (def, problems) = parse("m", &json!({"soul_forge": []}));
        assert!(def.is_none());
        assert_eq!(problems, ["m: soul_forge is not an object"]);
        let (def, problems) = parse("m", &json!({"soul_forge": {"blocks": ["m:f"]}}));
        assert!(def.is_none() && problems.len() == 1);
        let (def, problems) = parse("m", &json!({"soul_forge": {
            "blocks": ["m:f"], "full_jar": "m:j", "empty_jar": "m:e", "levels": "lots",
            "upgrades": [
                {"name": "glowing", "relic": "m:a"},
                {"name": "swift"},
                {"name": "warded", "relic": "m:r", "amount": 0.9, "kinds": ["hats"]},
                {"name": "radiant", "relic": "m:r"}
            ]}}));
        let def = def.unwrap();
        assert_eq!(def.levels, DEFAULT_LEVELS);
        assert_eq!(def.upgrades.len(), 1);
        assert_eq!(def.upgrades[0].amount, 0.25);
        assert_eq!(def.upgrades[0].kinds, [Kind::Armour]);
        // levels, unknown name, no relic, amount, kinds, the relic twice.
        assert_eq!(problems.len(), 6, "{problems:?}");
        assert!(problems.iter().all(|p| p.starts_with("m: ")));
    }

    #[test]
    fn kinds_by_tag_and_id() {
        let f = forge();
        let tags = |t: &str| t == "minecraft:swords";
        assert_eq!(f.kinds_of("minecraft:iron_sword", tags), [Kind::All, Kind::Weapons]);
        assert_eq!(f.kinds_of("m:dagger", |_| false), [Kind::All, Kind::Weapons]);
        let axe = |t: &str| t == "minecraft:axes";
        assert_eq!(f.kinds_of("minecraft:iron_axe", axe), [Kind::All, Kind::Tools]);
        assert_eq!(f.kinds_of("m:helmet", |_| false), [Kind::All, Kind::Armour]);
        assert_eq!(f.kinds_of("minecraft:dirt", |_| false), [Kind::All]);
    }

    #[test]
    fn the_forge_rule() {
        let f = forge();
        let weapon = [Kind::All, Kind::Weapons];
        let jar = Some(("m:jar_full", 1));
        let go = |has: &[Upgrade], kinds: &[Kind], relic, pay, levels, creative| {
            plan(&f, Some(1), has, kinds, relic, pay, levels, creative)
        };
        assert_eq!(
            go(&[], &weapon, Some("m:hilt"), jar, 5, false),
            Ok(Plan { upgrade: Upgrade::Sharpened, levels: 5, tome: false })
        );
        // The second costs 10, the third 15; the tome pays the levels.
        let one = [Upgrade::Radiant];
        assert_eq!(go(&one, &weapon, Some("m:hilt"), jar, 9, false), Err(Refusal::Levels(10)));
        let two = [Upgrade::Radiant, Upgrade::Soulbound];
        assert_eq!(go(&two, &weapon, Some("m:hilt"), jar, 15, false).map(|p| p.levels), Ok(15));
        let tome = go(&two, &weapon, Some("m:hilt"), Some(("m:tome", 3)), 0, false);
        assert_eq!(tome, Ok(Plan { upgrade: Upgrade::Sharpened, levels: 0, tome: true }));
        assert_eq!(go(&[], &weapon, Some("m:hilt"), jar, 0, true).map(|p| p.levels), Ok(0));
        // Refusals, in the order the slots are read.
        assert_eq!(plan(&f, None, &[], &weapon, None, None, 0, false), Err(Refusal::NoItem));
        assert_eq!(
            plan(&f, Some(2), &[], &weapon, Some("m:hilt"), jar, 99, false),
            Err(Refusal::OneItem)
        );
        assert_eq!(go(&[], &weapon, None, jar, 99, false), Err(Refusal::NoRelic));
        assert_eq!(go(&[], &weapon, Some("m:dirt"), jar, 99, false), Err(Refusal::NotARelic));
        assert_eq!(
            go(&[Upgrade::Sharpened], &weapon, Some("m:hilt"), jar, 99, false),
            Err(Refusal::Already(Upgrade::Sharpened))
        );
        let three = [Upgrade::Radiant, Upgrade::Soulbound, Upgrade::Reinforced];
        assert_eq!(go(&three, &weapon, Some("m:hilt"), jar, 99, false), Err(Refusal::Full(3)));
        assert_eq!(
            go(&[], &[Kind::All, Kind::Tools], Some("m:ring"), jar, 99, false),
            Err(Refusal::WrongKind(Upgrade::Warded, vec![Kind::Armour]))
        );
        assert_eq!(go(&[], &weapon, Some("m:hilt"), None, 99, false), Err(Refusal::NoPayment));
        let dirt = Some(("minecraft:dirt", 1));
        assert_eq!(go(&[], &weapon, Some("m:hilt"), dirt, 99, false), Err(Refusal::NoPayment));
        let jars = Some(("m:jar_full", 2));
        assert_eq!(go(&[], &weapon, Some("m:hilt"), jars, 99, false), Err(Refusal::OneJar));
        // Swift fits a tool, the common three anything.
        let tool = [Kind::All, Kind::Tools];
        assert!(go(&[], &tool, Some("m:shard"), jar, 5, false).is_ok());
        assert!(go(&[], &[Kind::All], Some("minecraft:glowstone"), jar, 5, false).is_ok());
        assert_eq!(
            Refusal::WrongKind(Upgrade::Swift, vec![Kind::Weapons, Kind::Tools]).text(),
            "Swift is only for weapons and tools"
        );
    }

    #[test]
    fn upgrades_read_from_an_item() {
        assert_eq!(
            upgrades_of(["swift", "nonsense", "radiant", "swift"]),
            [Upgrade::Swift, Upgrade::Radiant]
        );
        for u in ALL {
            assert_eq!(Upgrade::from_name(u.name()), Some(u));
        }
    }

    #[test]
    fn reinforced_halves_the_wear() {
        assert_eq!(kept_wear(4, 0.5, 0.9), 2);
        assert_eq!(kept_wear(1, 0.5, 0.2), 1);
        assert_eq!(kept_wear(1, 0.5, 0.7), 0);
        assert_eq!(kept_wear(3, 0.5, 0.49), 2);
        assert_eq!(kept_wear(0, 0.5, 0.0), 0);
        assert_eq!(kept_wear(5, 1.0, 0.99), 5);
        // On average, half.
        let total: i32 = (0..100).map(|i| kept_wear(1, 0.5, i as f32 / 100.0)).sum();
        assert_eq!(total, 50);
    }

    #[test]
    fn warded_pieces_add_up() {
        assert_eq!(warded_share(0, 0.08), 1.0);
        assert!((warded_share(1, 0.08) - 0.92).abs() < 1e-6);
        assert!((warded_share(4, 0.08) - 0.68).abs() < 1e-6);
        assert_eq!(warded_share(9, 0.25), 0.0);
    }

    #[test]
    fn each_mod_replaces_its_own_forge() {
        // The one test that touches the forges in force.
        let file = json!({"soul_forge": {"blocks": ["a:forge"], "full_jar": "a:j",
            "empty_jar": "a:e", "upgrades": [{"name": "soulbound", "relic": "a:gem"}]}});
        assert!(read("a_mod", &file).is_empty());
        assert_eq!(forges().by_block("a:forge").map(|f| f.mod_id.as_str()), Some("a_mod"));
        read("a_mod", &json!({}));
        assert!(forges().by_block("a:forge").is_none() && forges().is_empty());
    }

    /// For mod authors: `ARMORY_LIB_FILE=<path to armory_lib.json> cargo test checks_a_mods_file`
    /// fails with what the library could not use of the file's soul forge.
    #[test]
    fn checks_a_mods_file() {
        let Ok(path) = std::env::var("ARMORY_LIB_FILE") else {
            return;
        };
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let file: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        let problems = parse("mod", &file).1;
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
