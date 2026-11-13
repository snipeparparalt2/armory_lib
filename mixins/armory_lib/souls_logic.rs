// The soul pool, charged attacks, soul weapons and soul-stealing creatures, with nothing from
// the game in them: what content mods declare in their `data/<mod>/armory_lib.json`
// (`soul_pool`, `charged_attacks`, `soul_weapons`, and `steal_souls` on a `mob_rules` entry)
// and the sums the game module `armory_lib_souls` (souls.rs) makes with them. Added to the game
// as `game::armory_lib_souls_logic`; it is also the `souls` test target of the repository's
// Cargo package, so `cargo test` runs it alone.
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// A charged weapon's use time: held until let go, as a bow is.
pub const USE_TICKS: i32 = 72_000;
/// How far to the side of a thrust's line a creature's middle may be and still be hit.
pub const THRUST_WIDTH: f64 = 1.0;

/// A trinket that, in a trinket slot, makes the pool hold more and gives effects.
#[derive(Clone, Debug, PartialEq)]
pub struct TrinketBonus {
    pub setting: Option<String>,
    pub items: Vec<String>,
    pub max: u32,
    pub effects: Vec<String>,
}

/// An armour set that, worn whole, takes more souls a kill (and fills a jar as fast) and keeps
/// some harm off its wearer (effects taken away, damage of the same id cancelled).
#[derive(Clone, Debug, PartialEq)]
pub struct SetBonus {
    pub setting: Option<String>,
    pub pieces: Vec<String>,
    pub per_kill: u32,
    pub immune: Vec<String>,
}

/// Every player's soul pool: `per_kill` souls for each mob their blow kills, up to `max`.
#[derive(Clone, Debug, PartialEq)]
pub struct PoolDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub max: u32,
    pub per_kill: u32,
    pub trinkets: Vec<TrinketBonus>,
    pub sets: Vec<SetBonus>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attack {
    /// Every creature within `reach` of the player.
    Slam,
    /// Every creature on a line `reach` long from the player's eyes.
    Thrust,
}

/// A weapon held `charge_ticks` and let go strikes for `multiplier` times the player's attack
/// damage, for `souls` souls, then rests `cooldown_ticks`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChargeDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub items: Vec<String>,
    pub attack: Attack,
    pub reach: f64,
    pub multiplier: f32,
    pub souls: u32,
    pub cooldown_ticks: i32,
    pub charge_ticks: i32,
}

/// A weapon whose hit spends `souls` souls from the pool for `bonus` more damage.
#[derive(Clone, Debug, PartialEq)]
pub struct SoulWeaponDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub items: Vec<String>,
    pub souls: u32,
    pub bonus: f32,
}

/// What one mod's file declares.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModSouls {
    pub pool: Option<PoolDef>,
    pub charges: Vec<ChargeDef>,
    pub weapons: Vec<SoulWeaponDef>,
    /// Entity type and the souls its blow takes.
    pub steals: Vec<(String, u32)>,
}

/// Every mod's, by mod id; where two name the same item or type, the mod whose id sorts first
/// wins, and the pool is that of the first mod that declares one.
#[derive(Debug, Default)]
pub struct Souls {
    by_mod: BTreeMap<String, ModSouls>,
}

impl Souls {
    pub fn pool(&self) -> Option<&PoolDef> {
        self.by_mod.values().find_map(|m| m.pool.as_ref())
    }

    pub fn charge(&self, item: &str) -> Option<&ChargeDef> {
        self.by_mod
            .values()
            .flat_map(|m| &m.charges)
            .find(|c| c.items.iter().any(|i| i == item))
    }

    pub fn weapon(&self, item: &str) -> Option<&SoulWeaponDef> {
        self.by_mod
            .values()
            .flat_map(|m| &m.weapons)
            .find(|w| w.items.iter().any(|i| i == item))
    }

    /// The mod of the rule and the souls a blow of entity type `kind` takes.
    pub fn steal(&self, kind: &str) -> Option<(&str, u32)> {
        self.by_mod.iter().find_map(|(mod_id, m)| {
            m.steals
                .iter()
                .find(|(k, _)| k == kind)
                .map(|(_, n)| (mod_id.as_str(), *n))
        })
    }

    pub fn has_steals(&self) -> bool {
        self.by_mod.values().any(|m| !m.steals.is_empty())
    }
}

static SOULS: RwLock<Option<Arc<Souls>>> = RwLock::new(None);

/// The rules in force.
pub fn souls() -> Arc<Souls> {
    SOULS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Reads mod `mod_id`'s sections from its file and puts them in force in place of what that
/// mod's file declared before (`hooks.rs` calls it for each mod each time the files are read).
/// Returns what it could not use, one line each, for the log.
pub fn read(mod_id: &str, file: &serde_json::Value) -> Vec<String> {
    let (declared, problems) = parse(mod_id, file);
    let mut slot = SOULS.write().unwrap_or_else(|e| e.into_inner());
    let mut by_mod = slot.as_ref().map(|s| s.by_mod.clone()).unwrap_or_default();
    if declared == ModSouls::default() {
        by_mod.remove(mod_id);
    } else {
        by_mod.insert(mod_id.to_owned(), declared);
    }
    *slot = Some(Arc::new(Souls { by_mod }));
    problems
}

/// A number under `key` of `entry`, `default` when there is none; a line and the nearest
/// usable value when it isn't a number in `min..=max`.
fn number(
    entry: &serde_json::Value,
    key: &str,
    (default, min, max): (f64, f64, f64),
    what: &str,
    problems: &mut Vec<String>,
) -> f64 {
    match entry.get(key) {
        None => default,
        Some(v) => match v.as_f64().filter(|n| n.is_finite()) {
            Some(n) if (min..=max).contains(&n) => n,
            Some(n) => {
                let used = n.clamp(min, max);
                problems.push(format!("{what} {key} {n} is not {min} to {max}; using {used}"));
                used
            }
            None => {
                problems.push(format!("{what} {key} {v} is not a number; using {default}"));
                default
            }
        },
    }
}

fn whole(n: f64) -> u32 {
    n.round() as u32
}

fn strings(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_owned))
        .collect()
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

/// The objects of the list `key`; a line for a list that isn't one and each entry that isn't
/// an object.
fn entries<'a>(
    mod_id: &str,
    value: &'a serde_json::Value,
    key: &str,
    problems: &mut Vec<String>,
) -> Vec<&'a serde_json::Value> {
    let Some(v) = value.get(key) else {
        return Vec::new();
    };
    let Some(list) = v.as_array() else {
        problems.push(format!("{mod_id}: {key} is not a list"));
        return Vec::new();
    };
    list.iter()
        .filter(|e| {
            if !e.is_object() {
                problems.push(format!("{mod_id}: a {key} entry is not an object"));
            }
            e.is_object()
        })
        .collect()
}

/// What mod `mod_id`'s file declares, with defaults filled in and values clamped, and a line
/// for each entry or value it could not use as written.
pub fn parse(mod_id: &str, file: &serde_json::Value) -> (ModSouls, Vec<String>) {
    let mut out = ModSouls::default();
    let mut problems = Vec::new();
    if let Some(pool) = file.get("soul_pool") {
        if pool.is_object() {
            let what = format!("{mod_id}: soul_pool");
            let max = whole(number(pool, "max", (100.0, 1.0, 1e6), &what, &mut problems));
            let per_kill = whole(number(pool, "per_kill", (1.0, 0.0, 1000.0), &what, &mut problems));
            let mut made = PoolDef {
                mod_id: mod_id.to_owned(),
                setting: text(pool, "setting"),
                max,
                per_kill,
                trinkets: Vec::new(),
                sets: Vec::new(),
            };
            for t in entries(mod_id, pool, "trinkets", &mut problems) {
                let items = strings(t.get("items"));
                if items.is_empty() {
                    problems.push(format!("{mod_id}: a soul_pool trinkets entry has no items"));
                    continue;
                }
                let what = format!("{mod_id}: soul_pool trinket {}", items[0]);
                made.trinkets.push(TrinketBonus {
                    setting: text(t, "setting"),
                    max: whole(number(t, "max", (f64::from(max), 1.0, 1e6), &what, &mut problems)),
                    effects: strings(t.get("effects")),
                    items,
                });
            }
            for s in entries(mod_id, pool, "sets", &mut problems) {
                let pieces = strings(s.get("pieces"));
                if pieces.is_empty() {
                    problems.push(format!("{mod_id}: a soul_pool sets entry has no pieces"));
                    continue;
                }
                let what = format!("{mod_id}: soul_pool set {}", pieces[0]);
                made.sets.push(SetBonus {
                    setting: text(s, "setting"),
                    per_kill: whole(number(s, "per_kill", (f64::from(per_kill), 0.0, 1000.0), &what, &mut problems)),
                    immune: strings(s.get("immune")),
                    pieces,
                });
            }
            out.pool = Some(made);
        } else {
            problems.push(format!("{mod_id}: soul_pool is not an object"));
        }
    }
    for c in entries(mod_id, file, "charged_attacks", &mut problems) {
        let items = strings(c.get("items"));
        if items.is_empty() {
            problems.push(format!("{mod_id}: a charged_attacks entry has no items"));
            continue;
        }
        let what = format!("{mod_id}: charged attack of {}", items[0]);
        let (attack, reach_key, reach) = match c.get("attack").and_then(|v| v.as_str()) {
            Some("slam") => (Attack::Slam, "radius", 3.0),
            Some("thrust") => (Attack::Thrust, "length", 6.0),
            other => {
                problems.push(format!("{what}: attack {other:?} is not \"slam\" or \"thrust\""));
                continue;
            }
        };
        out.charges.push(ChargeDef {
            mod_id: mod_id.to_owned(),
            setting: text(c, "setting"),
            attack,
            reach: number(c, reach_key, (reach, 0.5, 16.0), &what, &mut problems),
            multiplier: number(c, "multiplier", (2.0, 0.0, 100.0), &what, &mut problems) as f32,
            souls: whole(number(c, "souls", (5.0, 0.0, 1e6), &what, &mut problems)),
            cooldown_ticks: (number(c, "cooldown_seconds", (5.0, 0.0, 3600.0), &what, &mut problems)
                * 20.0)
                .round() as i32,
            charge_ticks: (number(c, "charge_seconds", (1.0, 0.0, 60.0), &what, &mut problems)
                * 20.0)
                .round() as i32,
            items,
        });
    }
    for w in entries(mod_id, file, "soul_weapons", &mut problems) {
        let items = strings(w.get("items"));
        if items.is_empty() {
            problems.push(format!("{mod_id}: a soul_weapons entry has no items"));
            continue;
        }
        let what = format!("{mod_id}: soul weapon {}", items[0]);
        out.weapons.push(SoulWeaponDef {
            mod_id: mod_id.to_owned(),
            setting: text(w, "setting"),
            souls: whole(number(w, "souls", (1.0, 1.0, 1e6), &what, &mut problems)),
            bonus: number(w, "bonus_damage", (4.0, 0.0, 1000.0), &what, &mut problems) as f32,
            items,
        });
    }
    // `mob_rules` is read in full by the catalog; only `steal_souls` is ours.
    for rule in file.get("mob_rules").and_then(|v| v.as_array()).into_iter().flatten() {
        let (Some(kind), Some(_)) = (text(rule, "kind"), rule.get("steal_souls")) else {
            continue;
        };
        let what = format!("{mod_id}: mob rule of {kind}");
        let n = whole(number(rule, "steal_souls", (0.0, 0.0, 1e6), &what, &mut problems));
        if n > 0 {
            out.steals.push((kind, n));
        }
    }
    (out, problems)
}

// ---------------------------------------------------------------------------------------
// The sums
// ---------------------------------------------------------------------------------------

/// The pool's size: its own, or the biggest a bonus in force gives.
pub fn max_of(base: u32, bonuses: impl IntoIterator<Item = u32>) -> u32 {
    bonuses.into_iter().fold(base, u32::max)
}

/// A pool of `souls` takes `n` more, up to `max`. Souls past a `max` that shrank (a trinket
/// taken off) go, as health past a lower maximum does.
pub fn add(souls: u32, n: u32, max: u32) -> u32 {
    souls.min(max).saturating_add(n).min(max)
}

/// The pool after paying `cost`, or None when it holds too few.
pub fn spend(souls: u32, cost: u32, max: u32) -> Option<u32> {
    souls.min(max).checked_sub(cost)
}

/// The pool after a creature took `n` (never below none).
pub fn take(souls: u32, n: u32, max: u32) -> u32 {
    souls.min(max).saturating_sub(n)
}

/// Whole seconds (rounded up) left of a cooldown of `cooldown_ms` that started at `since`.
pub fn seconds_left(since: Option<u64>, now: u64, cooldown_ms: u64) -> u64 {
    since.map_or(0, |at| (at + cooldown_ms).saturating_sub(now).div_ceil(1000))
}

/// The direction a player looks, from their yaw and pitch in degrees (vanilla's
/// `Entity.calculateViewVector`).
pub fn look(yaw: f32, pitch: f32) -> [f64; 3] {
    let (yaw, pitch) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Whether a creature whose middle is at `at` is caught by a slam of `radius` around `centre`.
pub fn in_slam(centre: [f64; 3], radius: f64, at: [f64; 3]) -> bool {
    let d = sub(at, centre);
    dot(d, d) <= radius * radius
}

/// Whether a creature whose middle is at `at` is on a thrust from `eye` along `dir` (a unit
/// vector), `length` long and `width` to each side.
pub fn on_line(eye: [f64; 3], dir: [f64; 3], length: f64, width: f64, at: [f64; 3]) -> bool {
    let d = sub(at, eye);
    let along = dot(d, dir);
    (0.0..=length).contains(&along) && dot(d, d) - along * along <= width * width
}

/// The push a slam gives a creature at `at`: away from `centre` along the ground, and up.
pub fn push_from(centre: [f64; 3], at: [f64; 3]) -> [f64; 3] {
    let (x, z) = (at[0] - centre[0], at[2] - centre[2]);
    let len = (x * x + z * z).sqrt();
    if len < 1e-6 {
        return [0.0, 0.4, 0.0];
    }
    [x / len * 0.6, 0.35, z / len * 0.6]
}

/// Types a charged attack never hits, whoever made them: villagers and the wandering trader.
pub fn spared(kind: &str) -> bool {
    matches!(
        kind.strip_prefix("minecraft:").unwrap_or(kind),
        "villager" | "wandering_trader"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_full_file_with_defaults() {
        let file = json!({
            "soul_pool": {"max": 100, "per_kill": 1,
                "trinkets": [{"items": ["m:crown"], "max": 150, "effects": ["minecraft:night_vision"],
                              "setting": "trinkets"}],
                "sets": [{"pieces": ["m:h", "m:c", "m:l", "m:b"], "per_kill": 2,
                          "immune": ["minecraft:wither"]}]},
            "charged_attacks": [
                {"items": ["m:greatsword"], "attack": "slam", "radius": 3, "souls": 5,
                 "cooldown_seconds": 5, "setting": "weapon_effects"},
                {"items": ["m:halberd"], "attack": "thrust"}
            ],
            "soul_weapons": [{"items": ["m:blade"], "souls": 1, "bonus_damage": 4}],
            "mob_rules": [{"kind": "m:wraith", "steal_souls": 5}, {"kind": "m:other"}]
        });
        let (s, problems) = parse("m", &file);
        assert!(problems.is_empty(), "{problems:?}");
        let pool = s.pool.unwrap();
        assert_eq!((pool.max, pool.per_kill), (100, 1));
        assert_eq!(pool.trinkets[0].max, 150);
        assert_eq!(pool.trinkets[0].effects, ["minecraft:night_vision"]);
        assert_eq!(pool.sets[0].per_kill, 2);
        assert_eq!(pool.sets[0].immune, ["minecraft:wither"]);
        assert_eq!(s.charges[0].attack, Attack::Slam);
        assert_eq!((s.charges[0].reach, s.charges[0].souls), (3.0, 5));
        assert_eq!((s.charges[0].cooldown_ticks, s.charges[0].charge_ticks), (100, 20));
        // A thrust's defaults: 6 blocks, twice the damage, 5 souls, 5 s, held 1 s.
        let t = &s.charges[1];
        assert_eq!((t.attack, t.reach, t.multiplier, t.souls), (Attack::Thrust, 6.0, 2.0, 5));
        assert_eq!((t.cooldown_ticks, t.charge_ticks, t.setting.as_deref()), (100, 20, None));
        assert_eq!((s.weapons[0].souls, s.weapons[0].bonus), (1, 4.0));
        assert_eq!(s.steals, [("m:wraith".to_owned(), 5)]);
    }

    #[test]
    fn what_cannot_be_used_is_said() {
        let file = json!({
            "soul_pool": {"max": -3, "trinkets": [{"items": []}, "x"]},
            "charged_attacks": [{"items": ["m:a"], "attack": "spin"}, {"attack": "slam"},
                                {"items": ["m:b"], "attack": "slam", "radius": 99}],
            "soul_weapons": {"items": ["m:c"]},
            "mob_rules": [{"kind": "m:w", "steal_souls": "lots"}]
        });
        let (s, problems) = parse("m", &file);
        assert_eq!(s.pool.as_ref().map(|p| p.max), Some(1));
        assert_eq!(s.charges.len(), 1);
        assert_eq!(s.charges[0].reach, 16.0);
        assert!(s.weapons.is_empty() && s.steals.is_empty());
        // max, two trinkets entries, an unknown attack, no items, a radius, the weapons list,
        // the steal.
        assert_eq!(problems.len(), 8, "{problems:#?}");
        assert!(problems.iter().all(|p| p.starts_with("m: ")));
        let (s, problems) = parse("m", &json!({"soul_pool": 4}));
        assert_eq!((s, problems.len()), (ModSouls::default(), 1));
    }

    #[test]
    fn each_mod_replaces_its_own_rules() {
        // The one test that touches the rules in force.
        read("b_mod", &json!({"soul_pool": {"max": 50},
                              "charged_attacks": [{"items": ["x:sword"], "attack": "slam"}]}));
        read("a_mod", &json!({"soul_pool": {"max": 100},
                              "charged_attacks": [{"items": ["x:sword"], "attack": "thrust"}],
                              "mob_rules": [{"kind": "a:w", "steal_souls": 3}]}));
        let s = souls();
        assert_eq!(s.pool().map(|p| p.max), Some(100));
        assert_eq!(s.charge("x:sword").map(|c| c.attack), Some(Attack::Thrust));
        assert_eq!(s.steal("a:w"), Some(("a_mod", 3)));
        assert!(s.has_steals() && s.weapon("x:sword").is_none());
        read("a_mod", &json!({}));
        let s = souls();
        assert_eq!(s.pool().map(|p| p.max), Some(50));
        assert_eq!(s.charge("x:sword").map(|c| c.attack), Some(Attack::Slam));
        assert!(!s.has_steals());
    }

    #[test]
    fn the_pool_fills_spends_and_caps() {
        assert_eq!(max_of(100, []), 100);
        assert_eq!(max_of(100, [150]), 150);
        assert_eq!(add(45, 1, 100), 46);
        assert_eq!(add(99, 2, 100), 100);
        // A crown taken off: the souls past 100 go with the next change.
        assert_eq!(add(140, 1, 100), 100);
        assert_eq!(spend(5, 5, 100), Some(0));
        assert_eq!(spend(4, 5, 100), None);
        assert_eq!(spend(140, 5, 100), Some(95));
        assert_eq!(take(3, 5, 100), 0);
        assert_eq!(take(30, 5, 100), 25);
    }

    #[test]
    fn cooldowns_count_down_in_whole_seconds() {
        assert_eq!(seconds_left(None, 10_000, 5000), 0);
        assert_eq!(seconds_left(Some(10_000), 10_000, 5000), 5);
        assert_eq!(seconds_left(Some(10_000), 14_001, 5000), 1);
        assert_eq!(seconds_left(Some(10_000), 15_000, 5000), 0);
    }

    #[test]
    fn who_a_slam_and_a_thrust_reach() {
        let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9);
        // Yaw 0 faces south (+z); pitch 90 looks straight down.
        assert!(near(look(0.0, 0.0), [0.0, 0.0, 1.0]));
        assert!(near(look(90.0, 0.0), [-1.0, 0.0, 0.0]));
        assert!(near(look(0.0, 90.0), [0.0, -1.0, 0.0]));
        let c = [0.0, 64.0, 0.0];
        assert!(in_slam(c, 3.0, [2.0, 64.0, 2.0]));
        assert!(!in_slam(c, 3.0, [2.5, 64.0, 2.5]));
        let eye = [0.0, 65.6, 0.0];
        let south = look(0.0, 0.0);
        assert!(on_line(eye, south, 6.0, THRUST_WIDTH, [0.5, 65.0, 5.5]));
        assert!(!on_line(eye, south, 6.0, THRUST_WIDTH, [0.0, 65.6, 6.5]));
        assert!(!on_line(eye, south, 6.0, THRUST_WIDTH, [0.0, 65.6, -1.0]));
        assert!(!on_line(eye, south, 6.0, THRUST_WIDTH, [1.5, 65.6, 3.0]));
        let p = push_from(c, [3.0, 64.0, 0.0]);
        assert!(near(p, [0.6, 0.35, 0.0]));
        assert!(near(push_from(c, c), [0.0, 0.4, 0.0]));
        assert!(spared("minecraft:villager") && spared("wandering_trader"));
        assert!(!spared("minecraft:zombie") && !spared("m:villager"));
    }

    /// For mod authors: `ARMORY_LIB_FILE=<path to armory_lib.json> cargo test checks_a_mods_file`
    /// fails with what the library could not use of the file's soul sections.
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
