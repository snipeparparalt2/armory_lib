// Bosses, with nothing from the game in them: what content mods declare in their
// `data/<mod>/armory_lib.json` (`bosses`, `boss_triggers`, and `hover` on a `mob_looks` entry; the
// format is above `parse`) and the sums the game module `armory_lib_bosses` (bosses.rs) makes with
// them. Added to the game as `game::armory_lib_bosses_logic`; it is also the `bosses` test target
// of the repository's Cargo package, so `cargo test` runs it alone.
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// What a zombie copy drops of its own when a player kills it (a monster's `Mob.xpReward`).
pub const TEMPLATE_EXPERIENCE: u32 = 5;
/// How near a boss, in blocks, a player sees its bar and hears its music.
pub const NEAR: f64 = 48.0;
/// The entity tag that keeps how many phases a boss has begun (tags are saved with every mob).
pub const PHASE_TAG: &str = "armory_lib.boss_phase.";
/// `BossBarColor`'s names, in the order of its ordinals.
const COLORS: [&str; 7] = ["pink", "blue", "red", "green", "yellow", "purple", "white"];
const WHITE: i32 = 6;

/// An effect a blow leaves on the player it hurt.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub effect: String,
    /// 0 for level I.
    pub amplifier: u8,
    pub ticks: i32,
}

/// What a boss's blows do to a player: effects, and setting them alight.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Blows {
    pub on_hit: Vec<Effect>,
    pub ignite_ticks: i32,
}

impl Blows {
    pub fn is_empty(&self) -> bool {
        self.on_hit.is_empty() && self.ignite_ticks <= 0
    }
}

/// A part of a boss fight that begins when its health first falls to `below` of its maximum.
#[derive(Clone, Debug, PartialEq)]
pub struct Phase {
    pub below: f32,
    /// A creature called once, and how many.
    pub summon: Option<(String, u32)>,
    /// Faster by this share of its base speed (0.2: a fifth faster), from then on.
    pub speed: f64,
    /// What its blows do from then on, besides the boss's own.
    pub blows: Blows,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BossDef {
    pub mod_id: String,
    pub setting: Option<String>,
    /// The entity type, `mymod:warden`.
    pub kind: String,
    /// The bar's name where the player's language has no name for the type.
    pub name: String,
    /// `BossBarColor`'s ordinal.
    pub color: i32,
    /// The experience it is worth, the template's own included.
    pub experience: u32,
    /// A vanilla music event played to players near it (`minecraft:music.dragon`).
    pub music: Option<String>,
    pub blows: Blows,
    /// Highest `below` first.
    pub phases: Vec<Phase>,
}

impl BossDef {
    /// How many phases have begun at `health` of `max` (0: the fight as it starts).
    pub fn phases_at(&self, health: f32, max: f32) -> usize {
        if !(max > 0.0) {
            return 0;
        }
        self.phases.iter().filter(|p| health <= p.below * max).count()
    }

    /// What its blows do once `begun` phases have begun.
    pub fn blows(&self, begun: usize) -> Blows {
        let mut all = self.blows.clone();
        for phase in self.phases.iter().take(begun) {
            all.on_hit.extend(phase.blows.on_hit.iter().cloned());
            all.ignite_ticks = all.ignite_ticks.max(phase.blows.ignite_ticks);
        }
        all
    }

    /// Its type's name key, `entity.mymod.warden`.
    pub fn lang_key(&self) -> String {
        format!("entity.{}", self.kind.replacen(':', ".", 1))
    }
}

/// What raises a boss. With `use_item`, a right click with that item on `block` (standing on
/// `on`, if named) raises it on top, using the item up. Without, a player within `radius` of
/// `block` raises it on top, and the block becomes `becomes`, so it rises once.
#[derive(Clone, Debug, PartialEq)]
pub struct TriggerDef {
    pub mod_id: String,
    pub setting: Option<String>,
    pub boss: String,
    pub block: String,
    pub radius: i32,
    pub becomes: Option<String>,
    pub use_item: Option<String>,
    pub on: Option<String>,
}

/// What one mod's file declares.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModBosses {
    pub bosses: Vec<BossDef>,
    pub triggers: Vec<TriggerDef>,
    /// Creatures drawn above the ground: type and blocks.
    pub hovers: Vec<(String, f32)>,
}

/// Every mod's bosses, by mod id. Where two mods name one type, the id that sorts first wins.
#[derive(Debug, Default)]
pub struct Bosses {
    by_mod: BTreeMap<String, ModBosses>,
}

impl Bosses {
    pub fn is_empty(&self) -> bool {
        self.by_mod.values().all(|m| m.bosses.is_empty() && m.triggers.is_empty())
    }

    pub fn boss(&self, kind: &str) -> Option<&BossDef> {
        self.by_mod.values().flat_map(|m| &m.bosses).find(|b| b.kind == kind)
    }

    /// The triggers a player raises by coming near.
    pub fn seals(&self) -> impl Iterator<Item = &TriggerDef> {
        self.by_mod.values().flat_map(|m| &m.triggers).filter(|t| t.use_item.is_none())
    }

    /// The triggers a right click with `item` raises.
    pub fn altars<'a>(&'a self, item: &'a str) -> impl Iterator<Item = &'a TriggerDef> {
        self.by_mod
            .values()
            .flat_map(|m| &m.triggers)
            .filter(move |t| t.use_item.as_deref() == Some(item))
    }

    /// How far above the ground type `kind` is drawn, and the mod that says so.
    pub fn hover(&self, kind: &str) -> Option<(&str, f32)> {
        self.by_mod.iter().find_map(|(id, m)| {
            m.hovers.iter().find(|(k, _)| k == kind).map(|(_, h)| (id.as_str(), *h))
        })
    }

    /// Whether some boss's blows do something to a player.
    pub fn blows_matter(&self) -> bool {
        self.by_mod.values().flat_map(|m| &m.bosses).any(|b| {
            !b.blows.is_empty() || b.phases.iter().any(|p| !p.blows.is_empty())
        })
    }

    pub fn has_music(&self) -> bool {
        self.by_mod.values().flat_map(|m| &m.bosses).any(|b| b.music.is_some())
    }
}

static BOSSES: RwLock<Option<Arc<Bosses>>> = RwLock::new(None);

/// The bosses in force.
pub fn bosses() -> Arc<Bosses> {
    BOSSES
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Reads mod `mod_id`'s bosses from its file and puts them in force in place of what its file
/// declared before (`hooks.rs` calls it for each mod each time the files are read). Returns what
/// it could not use, one line each, for the log.
pub fn read(mod_id: &str, file: &serde_json::Value) -> Vec<String> {
    let (declared, problems) = parse(mod_id, file);
    let mut slot = BOSSES.write().unwrap_or_else(|e| e.into_inner());
    let mut by_mod = slot.as_ref().map(|b| b.by_mod.clone()).unwrap_or_default();
    if declared == ModBosses::default() {
        by_mod.remove(mod_id);
    } else {
        by_mod.insert(mod_id.to_owned(), declared);
    }
    *slot = Some(Arc::new(Bosses { by_mod }));
    problems
}

fn list<'a>(value: &'a serde_json::Value, key: &str) -> impl Iterator<Item = &'a serde_json::Value> {
    value.get(key).and_then(|v| v.as_array()).into_iter().flatten()
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

fn number(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|v| v.as_f64())
}

/// `on_hit` and `ignite_seconds` of a boss or one of its phases.
fn blows(value: &serde_json::Value) -> Blows {
    Blows {
        on_hit: list(value, "on_hit")
            .filter_map(|hit| {
                Some(Effect {
                    effect: text(hit, "effect")?,
                    amplifier: hit.get("amplifier").and_then(|v| v.as_u64()).unwrap_or(0).min(255) as u8,
                    ticks: hit.get("ticks").and_then(|v| v.as_i64()).unwrap_or(100).clamp(1, 72000) as i32,
                })
            })
            .collect(),
        ignite_ticks: (number(value, "ignite_seconds").unwrap_or(0.0).clamp(0.0, 600.0) * 20.0).round() as i32,
    }
}

/// A sound event's full id: `music.dragon` is `minecraft:music.dragon`.
fn event(name: String) -> String {
    if name.contains(':') { name } else { format!("minecraft:{name}") }
}

/// What mod `mod_id`'s file declares, with defaults filled in and values clamped, and a line for
/// each entry or value it could not use as written. The format:
///
/// ```json
/// "bosses": [{"kind": "mymod:warden", "name": "Warden", "color": "red", "experience": 50,
///             "music": "minecraft:music.dragon",
///             "on_hit": [{"effect": "minecraft:wither", "amplifier": 0, "ticks": 60}],
///             "ignite_seconds": 0,
///             "phases": [{"below": 0.5, "summon": {"kind": "mymod:guard", "count": 2},
///                         "speed": 0.2, "ignite_seconds": 5, "on_hit": []}]}],
/// "boss_triggers": [
///   {"boss": "mymod:warden", "block": "mymod:warden_seal", "radius": 10,
///    "becomes": "minecraft:chiseled_polished_blackstone"},
///   {"boss": "mymod:king", "block": "minecraft:gold_block", "on": "minecraft:polished_blackstone",
///    "use_item": "mymod:star"}],
/// "mob_looks": [{"kind": "mymod:ghost", "texture": "mymod:entity/ghost", "hover": 0.35}]
/// ```
///
/// `color` is a boss bar colour (pink, blue, red, green, yellow, purple, white; white if left
/// out). `experience` counts the zombie template's own 5. `music` is a vanilla music event. Any
/// entry may name a `setting` switch of the mod's page.
pub fn parse(mod_id: &str, file: &serde_json::Value) -> (ModBosses, Vec<String>) {
    let mut out = ModBosses::default();
    let mut problems = Vec::new();
    for entry in list(file, "bosses") {
        let Some(kind) = text(entry, "kind") else {
            problems.push(format!("{mod_id}: a bosses entry needs kind"));
            continue;
        };
        let color = match text(entry, "color") {
            None => WHITE,
            Some(name) => match COLORS.iter().position(|c| *c == name) {
                Some(i) => i as i32,
                None => {
                    problems.push(format!("{mod_id}: unknown boss bar color {name:?} for {kind}"));
                    WHITE
                }
            },
        };
        let mut phases = Vec::new();
        for phase in list(entry, "phases") {
            let below = number(phase, "below").filter(|b| *b > 0.0 && *b <= 1.0);
            let Some(below) = below else {
                problems.push(format!("{mod_id}: a phase of {kind} needs below, above 0 and up to 1"));
                continue;
            };
            let summon = phase.get("summon").and_then(|s| {
                let count = s.get("count").and_then(|v| v.as_u64()).unwrap_or(1).clamp(1, 16) as u32;
                Some((text(s, "kind")?, count))
            });
            if phase.get("summon").is_some() && summon.is_none() {
                problems.push(format!("{mod_id}: a summon of {kind} needs kind"));
            }
            phases.push(Phase {
                below: below as f32,
                summon,
                speed: number(phase, "speed").unwrap_or(0.0).clamp(-0.9, 4.0),
                blows: blows(phase),
            });
        }
        phases.sort_by(|a, b| b.below.total_cmp(&a.below));
        out.bosses.push(BossDef {
            mod_id: mod_id.to_owned(),
            setting: text(entry, "setting"),
            name: text(entry, "name").unwrap_or_else(|| kind.clone()),
            color,
            experience: entry.get("experience").and_then(|v| v.as_u64()).unwrap_or(0).min(100_000) as u32,
            music: text(entry, "music").map(event),
            blows: blows(entry),
            phases,
            kind,
        });
    }
    for entry in list(file, "boss_triggers") {
        let (Some(boss), Some(block)) = (text(entry, "boss"), text(entry, "block")) else {
            problems.push(format!("{mod_id}: a boss_triggers entry needs boss and block"));
            continue;
        };
        let (use_item, becomes) = (text(entry, "use_item"), text(entry, "becomes"));
        if use_item.is_none() && becomes.is_none() {
            problems.push(format!(
                "{mod_id}: the trigger of {boss} at {block} needs becomes (or use_item), or it would raise it again and again"
            ));
            continue;
        }
        out.triggers.push(TriggerDef {
            mod_id: mod_id.to_owned(),
            setting: text(entry, "setting"),
            radius: number(entry, "radius").unwrap_or(10.0).clamp(1.0, 32.0) as i32,
            on: text(entry, "on"),
            boss,
            block,
            becomes,
            use_item,
        });
    }
    for look in list(file, "mob_looks") {
        if let (Some(kind), Some(height)) = (text(look, "kind"), number(look, "hover")) {
            out.hovers.push((kind, height.clamp(0.0, 4.0) as f32));
        }
    }
    (out, problems)
}

// ---- Sums ------------------------------------------------------------------------------

/// The tag that says `begun` phases have begun.
pub fn phase_tag(begun: usize) -> String {
    format!("{PHASE_TAG}{begun}")
}

/// How many phases a boss has begun, by its tags (the most any of them says).
pub fn phases_from_tags(tags: &[String]) -> usize {
    tags.iter()
        .filter_map(|t| t.strip_prefix(PHASE_TAG)?.parse().ok())
        .max()
        .unwrap_or(0)
}

/// How full a bar is, 0 to 1.
pub fn progress(health: f32, max: f32) -> f32 {
    if max > 0.0 { (health / max).clamp(0.0, 1.0) } else { 0.0 }
}

/// The experience a boss drops besides what its zombie template drops for a player's kill.
pub fn extra_experience(declared: u32) -> u32 {
    declared.saturating_sub(TEMPLATE_EXPERIENCE)
}

/// The boss bar's id for the mob with `uuid`, kept clear of the mob's own UUID.
pub fn bar_id(uuid: u128) -> u128 {
    uuid ^ 0xa4d1_b055_0000_0000_0000_0000_0000_0000
}

/// Where `count` summoned creatures appear around their boss: on a ring of 2 blocks, evenly.
pub fn summon_spots(count: u32) -> Vec<[f64; 2]> {
    (0..count)
        .map(|i| {
            let angle = std::f64::consts::TAU * f64::from(i) / f64::from(count.max(1));
            [2.0 * angle.cos(), 2.0 * angle.sin()]
        })
        .collect()
}

pub fn within(a: [f64; 3], b: [f64; 3], reach: f64) -> bool {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>() <= reach * reach
}

/// Whether the block `offset` from a player's feet is within `radius` blocks of them.
pub fn in_reach(offset: [i32; 3], radius: i32) -> bool {
    offset.iter().map(|c| c * c).sum::<i32>() <= radius * radius
}

/// How high a hovering creature is drawn `age` ticks into its life: `height`, bobbing gently.
pub fn hover_lift(height: f32, age: f32) -> f32 {
    height + (age * 0.1).sin() * 0.06
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn example() -> serde_json::Value {
        json!({
            "bosses": [
                {"kind": "m:warden", "name": "Warden", "color": "red", "experience": 50,
                 "music": "music.overworld.deep_dark",
                 "phases": [{"below": 0.5, "summon": {"kind": "m:sentry", "count": 2}, "speed": 0.2,
                             "ignite_seconds": 5}]},
                {"kind": "m:reaper", "color": "purple", "experience": 100, "music": "minecraft:music.dragon",
                 "on_hit": [{"effect": "minecraft:wither", "ticks": 60}],
                 "phases": [{"below": 0.33, "speed": 0.3}, {"below": 0.66, "summon": {"kind": "m:shade", "count": 3}}]},
                {"kind": "m:odd", "color": "teal", "phases": [{"below": 0}, {"summon": {"count": 2}, "below": 0.5}]},
                {"name": "nobody"}
            ],
            "boss_triggers": [
                {"boss": "m:warden", "block": "m:warden_seal", "radius": 10, "becomes": "minecraft:chiseled_polished_blackstone"},
                {"boss": "m:king", "block": "minecraft:gold_block", "on": "minecraft:polished_blackstone", "use_item": "m:star"},
                {"boss": "m:reaper", "block": "m:reaper_seal"},
                {"block": "m:x"}
            ],
            "mob_looks": [
                {"kind": "m:wraith", "texture": "m:entity/wraith", "hover": 0.35},
                {"kind": "m:warden", "texture": "m:entity/warden"}
            ]
        })
    }

    #[test]
    fn a_mods_bosses_are_read() {
        let (d, problems) = parse("m", &example());
        assert_eq!(problems.len(), 6, "{problems:#?}");
        let warden = &d.bosses[0];
        assert_eq!((warden.name.as_str(), warden.color, warden.experience), ("Warden", 2, 50));
        assert_eq!(warden.music.as_deref(), Some("minecraft:music.overworld.deep_dark"));
        assert_eq!(warden.phases[0].summon, Some(("m:sentry".to_string(), 2)));
        assert_eq!(warden.phases[0].blows.ignite_ticks, 100);
        let reaper = &d.bosses[1];
        assert_eq!(reaper.name, "m:reaper");
        assert_eq!(reaper.phases.iter().map(|p| p.below).collect::<Vec<_>>(), [0.66, 0.33]);
        assert_eq!(reaper.blows.on_hit[0], Effect { effect: "minecraft:wither".into(), amplifier: 0, ticks: 60 });
        let odd = &d.bosses[2];
        assert_eq!((odd.color, odd.phases.len(), odd.phases[0].summon.clone()), (WHITE, 1, None));
        assert_eq!(d.bosses.len(), 3);
        assert_eq!(d.triggers.len(), 2);
        assert_eq!((d.triggers[0].radius, d.triggers[1].on.as_deref()), (10, Some("minecraft:polished_blackstone")));
        assert_eq!(d.hovers, vec![("m:wraith".to_string(), 0.35)]);
        assert_eq!(warden.lang_key(), "entity.m.warden");
    }

    #[test]
    fn what_is_in_force() {
        read("m", &example());
        let all = bosses();
        assert!(!all.is_empty() && all.blows_matter() && all.has_music());
        assert_eq!(all.boss("m:reaper").map(|b| b.experience), Some(100));
        assert_eq!(all.seals().map(|t| t.boss.as_str()).collect::<Vec<_>>(), ["m:warden"]);
        assert_eq!(all.altars("m:star").count(), 1);
        assert_eq!(all.altars("m:stone").count(), 0);
        assert_eq!(all.hover("m:wraith"), Some(("m", 0.35)));
        assert_eq!(all.hover("m:warden"), None);
        read("m", &json!({}));
        assert!(bosses().is_empty());
    }

    #[test]
    fn phases_begin_by_health_and_add_to_the_blows() {
        let (d, _) = parse("m", &example());
        let reaper = &d.bosses[1];
        assert_eq!(reaper.phases_at(300.0, 300.0), 0);
        assert_eq!(reaper.phases_at(198.0, 300.0), 1);
        assert_eq!(reaper.phases_at(99.0, 300.0), 2);
        assert_eq!(reaper.phases_at(5.0, 0.0), 0);
        let warden = &d.bosses[0];
        assert_eq!(warden.blows(0).ignite_ticks, 0);
        assert_eq!(warden.blows(1).ignite_ticks, 100);
        assert_eq!(reaper.blows(2).on_hit.len(), 1);
        assert!(!reaper.blows(0).is_empty() && Blows::default().is_empty());
    }

    #[test]
    fn phase_tags() {
        assert_eq!(phase_tag(2), "armory_lib.boss_phase.2");
        let tags = vec!["other".to_string(), phase_tag(1), phase_tag(2), format!("{PHASE_TAG}x")];
        assert_eq!(phases_from_tags(&tags), 2);
        assert_eq!(phases_from_tags(&[]), 0);
    }

    #[test]
    fn sums() {
        assert_eq!(progress(50.0, 200.0), 0.25);
        assert_eq!((progress(-1.0, 200.0), progress(300.0, 200.0), progress(1.0, 0.0)), (0.0, 1.0, 0.0));
        assert_eq!((extra_experience(50), extra_experience(3)), (45, 0));
        assert_ne!(bar_id(7), 7);
        assert_eq!(bar_id(bar_id(7)), 7);
        let spots = summon_spots(3);
        assert_eq!(spots.len(), 3);
        assert!(spots.iter().all(|[x, z]| ((x * x + z * z).sqrt() - 2.0).abs() < 1e-9));
        assert!(within([0.0, 64.0, 0.0], [48.0, 64.0, 0.0], NEAR) && !within([0.0; 3], [48.0, 1.0, 0.0], NEAR));
        assert!(in_reach([6, 6, 5], 10) && !in_reach([10, 1, 0], 10));
        assert!((hover_lift(0.35, 0.0) - 0.35).abs() < 1e-6 && (hover_lift(0.35, 15.7) - 0.35).abs() <= 0.06);
    }

    /// For mod authors: `ARMORY_LIB_FILE=<path to armory_lib.json> cargo test checks_a_mods_file`
    /// fails with what the library could not use of the file's bosses and boss triggers.
    #[test]
    fn checks_a_mods_file() {
        let Ok(path) = std::env::var("ARMORY_LIB_FILE") else {
            return;
        };
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let file: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        let problems = parse("mod", &file).1;
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
