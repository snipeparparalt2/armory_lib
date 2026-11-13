//! Armory Lib's logic, with nothing from the game in it: what mods register, and the sums the
//! hooks in `lib.rs` do with it. `cargo test` at the repository root runs this file alone.

pub const ID: &str = "armory_lib";
pub const NAME: &str = "Armory Lib";
pub const VERSION: &str = "0.1.0";

// ---------------------------------------------------------------------------------------
// Coloured gear light
// ---------------------------------------------------------------------------------------

/// An item that lights the world around whoever holds or wears it, and where it lies.
#[derive(Clone, Debug, PartialEq)]
pub struct GearLight {
    /// The mod that owns it: its light goes out while that mod is off in Options > Mods.
    pub mod_id: String,
    /// A switch of that mod's settings page ([`Page`]) that turns the light off, if any.
    pub setting: Option<String>,
    /// The item id, `mymod:glow_sword`.
    pub item: String,
    /// 0xRRGGBB.
    pub rgb: u32,
    /// How strong it is, 0 to 1: 1 is a full light, 0.5 half of one.
    pub level: f32,
}

impl GearLight {
    pub fn new(mod_id: &str, item: &str, rgb: u32) -> Self {
        Self {
            mod_id: mod_id.into(),
            setting: None,
            item: item.into(),
            rgb,
            level: 1.0,
        }
    }
    pub fn level(mut self, level: f32) -> Self {
        self.level = level;
        self
    }
    pub fn setting(mut self, key: &str) -> Self {
        self.setting = Some(key.into());
        self
    }
}

/// One light, as Horizon's `NativeColoredLight` takes it besides the position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub rgb: u32,
    /// Blocks, 0.25 to 16.
    pub radius: f32,
    /// 0 to 1.
    pub intensity: f32,
}

/// Horizon draws at most 8 coloured lights per mod; the nearest ones are kept.
pub const MAX_LIGHTS: usize = 8;
/// Horizon refuses lights more than 32 blocks from the player; stay well inside.
pub const REACH: f64 = 24.0;
/// A held item counts fully, a worn piece half.
pub const HELD: f32 = 1.0;
pub const WORN: f32 = 0.5;

/// The one light of everything a creature holds and wears: `pieces` are `(rgb, weight)`,
/// the weight being [`HELD`] or [`WORN`] times the item's level. The colours are mixed by
/// weight, and more gear makes a wider, stronger light.
pub fn blend(pieces: impl IntoIterator<Item = (u32, f32)>) -> Option<Glow> {
    let mut sum = [0.0f32; 3];
    let mut weight = 0.0f32;
    for (rgb, w) in pieces {
        if !(w > 0.0) {
            continue;
        }
        let [_, r, g, b] = rgb.to_be_bytes();
        sum[0] += f32::from(r) * w;
        sum[1] += f32::from(g) * w;
        sum[2] += f32::from(b) * w;
        weight += w;
    }
    if weight <= 0.0 {
        return None;
    }
    let channel = |v: f32| (v / weight).round().clamp(0.0, 255.0) as u32;
    Some(Glow {
        rgb: channel(sum[0]) << 16 | channel(sum[1]) << 8 | channel(sum[2]),
        // One held item: 6.5 blocks at 0.45. A full set and a weapon: 9.5 blocks at 0.75.
        radius: (5.0 + 1.5 * weight).min(11.0),
        intensity: (0.30 + 0.15 * weight).min(0.9),
    })
}

/// The fainter light of a glowing item lying on the ground.
pub fn dropped(rgb: u32, level: f32) -> Option<Glow> {
    (level > 0.0).then(|| Glow {
        rgb,
        radius: (3.5 * level).clamp(0.25, 16.0),
        intensity: (0.3 * level).clamp(0.0, 1.0),
    })
}

/// Keeps the `max` lights nearest `player` within [`REACH`], nearest first.
pub fn nearest(player: [f64; 3], lights: &mut Vec<([f64; 3], Glow)>, max: usize) {
    let far = |at: &[f64; 3]| {
        (at[0] - player[0]).powi(2) + (at[1] - player[1]).powi(2) + (at[2] - player[2]).powi(2)
    };
    lights.retain(|(at, _)| at.iter().all(|v| v.is_finite()) && far(at) <= REACH * REACH);
    lights.sort_by(|a, b| far(&a.0).total_cmp(&far(&b.0)));
    lights.truncate(max);
}

// ---------------------------------------------------------------------------------------
// Settings pages
// ---------------------------------------------------------------------------------------

/// One on/off switch of a settings page.
#[derive(Clone, Debug, PartialEq)]
pub struct Switch {
    /// The setting's key: the game keeps it in a file of that name in the mod's data folder.
    pub key: String,
    /// What the button says before `: ON` or `: OFF`.
    pub label: String,
    /// What it is before the player touches it.
    pub default: bool,
}

impl Switch {
    pub fn new(key: &str, label: &str) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            default: true,
        }
    }
    pub fn off_by_default(mut self) -> Self {
        self.default = false;
        self
    }
}

/// A mod's row in Options > Mods and the switches on its page.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub mod_id: String,
    /// `Name 1.0`.
    pub title: String,
    pub description: String,
    pub switches: Vec<Switch>,
}

/// A kept setting as a switch: `true` and `false` are themselves, anything else (or no
/// file yet) is the default.
pub fn read_switch(value: Option<&str>, default: bool) -> bool {
    match value.map(str::trim) {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

pub fn switch_label(label: &str, on: bool) -> String {
    format!("{label}: {}", if on { "ON" } else { "OFF" })
}

// ---------------------------------------------------------------------------------------
// Mob looks
// ---------------------------------------------------------------------------------------

/// How a model part moves each frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// Turns with the head: yaw and pitch.
    Head,
    /// Swings with the walk cycle. `phase` is 0 or `PI` (the opposite limb), `reach` how far:
    /// 1.4 for legs, 1.0 for a walking arm.
    Swing { phase: f32, reach: f32 },
    /// Held out in front as a zombie's arms, with a slow bob.
    Raised,
}

/// A vanilla model layer and how its parts move: the body a registered mob is drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    /// The entity model layer, as Horizon's `bake_layer` takes it: `minecraft:zombie#main`.
    pub layer: String,
    /// Part names (searched through the whole tree) and their motion.
    pub parts: Vec<(String, Motion)>,
}

impl Rig {
    fn of(layer: &str, parts: &[(&str, Motion)]) -> Self {
        Self {
            layer: layer.into(),
            parts: parts.iter().map(|(n, m)| ((*n).into(), *m)).collect(),
        }
    }
    /// A zombie's body on the 64x64 humanoid texture layout, arms held out.
    pub fn zombie() -> Self {
        use std::f32::consts::PI;
        Self::of(
            "minecraft:zombie#main",
            &[
                ("head", Motion::Head),
                ("right_arm", Motion::Raised),
                ("left_arm", Motion::Raised),
                ("right_leg", Motion::Swing { phase: 0.0, reach: 1.4 }),
                ("left_leg", Motion::Swing { phase: PI, reach: 1.4 }),
            ],
        )
    }
    /// The same body with arms that swing as it walks.
    pub fn humanoid() -> Self {
        use std::f32::consts::PI;
        Self::of(
            "minecraft:zombie#main",
            &[
                ("head", Motion::Head),
                ("right_arm", Motion::Swing { phase: PI, reach: 1.0 }),
                ("left_arm", Motion::Swing { phase: 0.0, reach: 1.0 }),
                ("right_leg", Motion::Swing { phase: 0.0, reach: 1.4 }),
                ("left_leg", Motion::Swing { phase: PI, reach: 1.4 }),
            ],
        )
    }
    /// A wolf's body (64x32 wolf texture layout).
    pub fn wolf() -> Self {
        use std::f32::consts::PI;
        Self::of(
            "minecraft:wolf#main",
            &[
                ("head", Motion::Head),
                ("right_hind_leg", Motion::Swing { phase: 0.0, reach: 1.4 }),
                ("left_hind_leg", Motion::Swing { phase: PI, reach: 1.4 }),
                ("right_front_leg", Motion::Swing { phase: PI, reach: 1.4 }),
                ("left_front_leg", Motion::Swing { phase: 0.0, reach: 1.4 }),
            ],
        )
    }
    /// A chicken's body (64x32 chicken texture layout).
    pub fn chicken() -> Self {
        use std::f32::consts::PI;
        Self::of(
            "minecraft:chicken#main",
            &[
                ("head", Motion::Head),
                ("right_leg", Motion::Swing { phase: 0.0, reach: 1.4 }),
                ("left_leg", Motion::Swing { phase: PI, reach: 1.4 }),
            ],
        )
    }
    /// Any other layer, with the part motions given.
    pub fn custom(layer: &str, parts: &[(&str, Motion)]) -> Self {
        Self::of(layer, parts)
    }
}

/// The look of one entity type.
#[derive(Clone, Debug, PartialEq)]
pub struct MobLook {
    /// The mod that owns it: while that mod is off the mob is drawn as its template.
    pub mod_id: String,
    /// The entity type, `mymod:shade`.
    pub kind: String,
    pub rig: Rig,
    /// The sprite, `mymod:entity/shade/shade` for `assets/mymod/textures/entity/shade/shade.png`.
    pub texture: String,
    /// A second sprite with only the glowing pixels, drawn over the first at full brightness.
    pub emissive: Option<String>,
    /// The model's size against the vanilla one.
    pub scale: f32,
}

impl MobLook {
    pub fn new(mod_id: &str, kind: &str, rig: Rig, texture: &str) -> Self {
        Self {
            mod_id: mod_id.into(),
            kind: kind.into(),
            rig,
            texture: texture.into(),
            emissive: None,
            scale: 1.0,
        }
    }
    pub fn emissive(mut self, texture: &str) -> Self {
        self.emissive = Some(texture.into());
        self
    }
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }
}

/// What the frame says of a mob, as far as the poses need it (Horizon's `NativeMobRender`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    pub age: f32,
    /// Degrees from the body.
    pub head_yaw: f32,
    /// Degrees, positive looking down.
    pub pitch: f32,
    pub walk_position: f32,
    pub walk_speed: f32,
}

/// A part's rotation `[x, y, z]` in radians for `motion` this frame, as vanilla's
/// `HumanoidModel.setupAnim`, `QuadrupedModel.setupAnim` and `animateZombieArms` set them.
/// `right` says which side a raised arm is on (they bob in opposite directions).
pub fn pose(motion: Motion, frame: &Frame, right: bool) -> [f32; 3] {
    match motion {
        Motion::Head => [frame.pitch.to_radians(), frame.head_yaw.to_radians(), 0.0],
        Motion::Swing { phase, reach } => [
            (frame.walk_position * 0.6662 + phase).cos() * reach * frame.walk_speed,
            0.0,
            0.0,
        ],
        Motion::Raised => {
            let bob_z = (frame.age * 0.09).cos() * 0.05 + 0.05;
            let bob_x = (frame.age * 0.067).sin() * 0.05;
            let side = if right { 1.0 } else { -1.0 };
            [
                -std::f32::consts::PI / 2.25 + bob_x * side,
                0.0,
                bob_z * side,
            ]
        }
    }
}

/// How far a dying mob has tipped over, in degrees (vanilla's `setupRotations`: 90 degrees
/// over 20 ticks, eased by a square root).
pub fn death_tilt(death_time: i32, partial: f32) -> f32 {
    if death_time <= 0 {
        return 0.0;
    }
    let f = ((death_time as f32 + partial - 1.0) / 20.0 * 1.6).max(0.0);
    f.sqrt().min(1.0) * 90.0
}

/// The tint a mob is drawn with: red while the hurt flash lasts or it is dying.
pub fn tint(hurt_time: i32, death_time: i32) -> u32 {
    if hurt_time > 0 || death_time > 0 {
        0xFF8080
    } else {
        0xFFFFFF
    }
}

// ---------------------------------------------------------------------------------------
// Mob rules
// ---------------------------------------------------------------------------------------

/// An effect a mob's hit leaves on the player it hurt.
#[derive(Clone, Debug, PartialEq)]
pub struct HitEffect {
    /// `minecraft:slowness`.
    pub effect: String,
    /// 0 for level I.
    pub amplifier: u8,
    /// Ticks.
    pub duration: i32,
}

/// Rules for one entity type that its template doesn't have.
#[derive(Clone, Debug, PartialEq)]
pub struct MobRules {
    pub mod_id: String,
    pub kind: String,
    /// It keeps its type when its template would turn into another (a zombie copy under
    /// water stays what it is).
    pub no_conversion: bool,
    /// Burning does it no harm (`minecraft:on_fire` and `minecraft:in_fire` damage), so
    /// daylight can't kill a copy of an undead mob. It can still be seen burning.
    pub fire_proof: bool,
    pub on_hit: Vec<HitEffect>,
}

impl MobRules {
    pub fn new(mod_id: &str, kind: &str) -> Self {
        Self {
            mod_id: mod_id.into(),
            kind: kind.into(),
            no_conversion: false,
            fire_proof: false,
            on_hit: Vec::new(),
        }
    }
    pub fn no_conversion(mut self) -> Self {
        self.no_conversion = true;
        self
    }
    pub fn fire_proof(mut self) -> Self {
        self.fire_proof = true;
        self
    }
    pub fn on_hit(mut self, effect: &str, amplifier: u8, duration: i32) -> Self {
        self.on_hit.push(HitEffect {
            effect: effect.into(),
            amplifier,
            duration,
        });
        self
    }
}

/// Whether `damage_type` is what burning deals.
pub fn is_burning(damage_type: &str) -> bool {
    matches!(damage_type, "minecraft:on_fire" | "minecraft:in_fire")
}

/// Whether `damage_type` is a mob's own blow (not its arrow, its fire or its explosion).
pub fn is_melee(damage_type: &str) -> bool {
    matches!(
        damage_type,
        "minecraft:mob_attack" | "minecraft:mob_attack_no_aggro"
    )
}

// ---------------------------------------------------------------------------------------
// Carried-item effects
// ---------------------------------------------------------------------------------------

/// Where an item must be for its effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The held slot.
    MainHand,
    OffHand,
    /// Any of the nine hotbar slots or the off hand.
    Hotbar,
    /// Any armour slot.
    Worn,
    /// Every listed item worn at once: a full set.
    AllWorn,
}

/// An effect a player has while they carry an item.
#[derive(Clone, Debug, PartialEq)]
pub struct CarriedEffect {
    pub mod_id: String,
    /// A switch of the mod's settings page that turns it off, if any.
    pub setting: Option<String>,
    /// The item ids. One of them is enough, except for [`Place::AllWorn`], which needs all.
    pub items: Vec<String>,
    pub place: Place,
    /// `minecraft:speed`.
    pub effect: String,
    /// 0 for level I.
    pub amplifier: u8,
}

impl CarriedEffect {
    pub fn new(mod_id: &str, items: &[&str], place: Place, effect: &str) -> Self {
        Self {
            mod_id: mod_id.into(),
            setting: None,
            items: items.iter().map(|i| (*i).into()).collect(),
            place,
            effect: effect.into(),
            amplifier: 0,
        }
    }
    pub fn amplifier(mut self, amplifier: u8) -> Self {
        self.amplifier = amplifier;
        self
    }
    pub fn setting(mut self, key: &str) -> Self {
        self.setting = Some(key.into());
        self
    }
}

/// What a player carries this tick, by item id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Carried {
    pub main_hand: Option<String>,
    pub off_hand: Option<String>,
    /// The nine hotbar slots.
    pub hotbar: Vec<Option<String>>,
    /// Head, chest, legs, feet.
    pub worn: [Option<String>; 4],
}

/// Whether `carried` meets `rule`'s place.
pub fn applies(rule: &CarriedEffect, carried: &Carried) -> bool {
    let is = |slot: &Option<String>| {
        slot.as_deref()
            .is_some_and(|id| rule.items.iter().any(|i| i == id))
    };
    match rule.place {
        Place::MainHand => is(&carried.main_hand),
        Place::OffHand => is(&carried.off_hand),
        Place::Hotbar => is(&carried.off_hand) || carried.hotbar.iter().any(is),
        Place::Worn => carried.worn.iter().any(is),
        Place::AllWorn => {
            !rule.items.is_empty()
                && rule
                    .items
                    .iter()
                    .all(|i| carried.worn.iter().any(|w| w.as_deref() == Some(i.as_str())))
        }
    }
}

/// How long a carried effect is given for, and how often it is given again. It outlasts
/// the gap with room to spare, and is long enough that night vision doesn't flicker.
pub const EFFECT_TICKS: i32 = 320;
pub const REFRESH_TICKS: u32 = 40;

/// The strongest level of each effect among `active` rules: `(effect, amplifier)`, sorted.
pub fn strongest<'a>(active: impl IntoIterator<Item = &'a CarriedEffect>) -> Vec<(String, u8)> {
    let mut out: Vec<(String, u8)> = Vec::new();
    for rule in active {
        match out.iter_mut().find(|(id, _)| *id == rule.effect) {
            Some((_, level)) => *level = (*level).max(rule.amplifier),
            None => out.push((rule.effect.clone(), rule.amplifier)),
        }
    }
    out.sort();
    out
}

/// The effects to take away: those the library gave last time (`before`) that no rule
/// gives now.
pub fn lapsed(before: &[(String, u8)], now: &[(String, u8)]) -> Vec<String> {
    before
        .iter()
        .filter(|(id, _)| !now.iter().any(|(n, _)| n == id))
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn one_held_item_is_its_own_colour() {
        let g = blend([(0x8cffe2, HELD)]).unwrap();
        assert_eq!(g.rgb, 0x8cffe2);
        assert!((g.radius - 6.5).abs() < 1e-6);
        assert!((g.intensity - 0.45).abs() < 1e-6);
    }

    #[test]
    fn gear_mixes_by_weight_and_grows() {
        let g = blend([(0xff0000, HELD), (0x0000ff, HELD)]).unwrap();
        assert_eq!(g.rgb, 0x800080);
        let set = blend([
            (0x9e60ff, HELD),
            (0x9e60ff, WORN),
            (0x9e60ff, WORN),
            (0x9e60ff, WORN),
            (0x9e60ff, WORN),
        ])
        .unwrap();
        assert!((set.radius - 9.5).abs() < 1e-6);
        assert!((set.intensity - 0.75).abs() < 1e-6);
    }

    #[test]
    fn nothing_glowing_is_no_light() {
        assert_eq!(blend([]), None);
        assert_eq!(blend([(0xffffff, 0.0), (0xffffff, f32::NAN)]), None);
        assert_eq!(dropped(0xffffff, 0.0), None);
    }

    #[test]
    fn light_stays_inside_horizons_limits() {
        let g = blend((0..40).map(|_| (0xffffff, HELD))).unwrap();
        assert!(g.radius <= 16.0 && g.intensity <= 1.0);
        let d = dropped(0x123456, 1.0).unwrap();
        assert_eq!((d.radius, d.intensity), (3.5, 0.3));
    }

    #[test]
    fn nearest_lights_win() {
        let glow = Glow { rgb: 1, radius: 1.0, intensity: 1.0 };
        let mut lights: Vec<_> = (0..20).map(|i| ([f64::from(i) * 2.0, 64.0, 0.0], glow)).collect();
        lights.push(([f64::NAN, 0.0, 0.0], glow));
        lights.reverse();
        nearest([0.0, 64.0, 0.0], &mut lights, MAX_LIGHTS);
        assert_eq!(lights.len(), MAX_LIGHTS);
        assert_eq!(lights[0].0[0], 0.0);
        assert!(lights.iter().all(|(at, _)| at[0] <= REACH));
    }

    #[test]
    fn switches_read_and_label() {
        assert!(read_switch(None, true));
        assert!(!read_switch(None, false));
        assert!(!read_switch(Some("false\n"), true));
        assert!(read_switch(Some("true"), false));
        assert!(read_switch(Some("junk"), true));
        assert_eq!(switch_label("Glow", true), "Glow: ON");
        assert_eq!(switch_label("Glow", false), "Glow: OFF");
        assert!(!Switch::new("a", "A").off_by_default().default);
    }

    #[test]
    fn head_follows_the_look() {
        let f = Frame { head_yaw: 90.0, pitch: -45.0, ..Frame::default() };
        let [x, y, z] = pose(Motion::Head, &f, false);
        assert!((x + PI / 4.0).abs() < 1e-6 && (y - PI / 2.0).abs() < 1e-6 && z == 0.0);
    }

    #[test]
    fn legs_swing_opposite_and_rest_when_still() {
        let walking = Frame { walk_position: 1.0, walk_speed: 1.0, ..Frame::default() };
        let a = pose(Motion::Swing { phase: 0.0, reach: 1.4 }, &walking, true)[0];
        let b = pose(Motion::Swing { phase: PI, reach: 1.4 }, &walking, false)[0];
        assert!((a + b).abs() < 1e-5 && a.abs() > 0.5 && a.abs() <= 1.4);
        let still = Frame { walk_position: 7.0, ..Frame::default() };
        assert_eq!(pose(Motion::Swing { phase: 0.0, reach: 1.4 }, &still, true)[0], 0.0);
    }

    #[test]
    fn raised_arms_point_forward() {
        let [x, _, z] = pose(Motion::Raised, &Frame::default(), true);
        assert!(x < -1.2 && x > -1.6);
        let [_, _, zl] = pose(Motion::Raised, &Frame::default(), false);
        assert!((z + zl).abs() < 1e-6);
    }

    #[test]
    fn rigs_name_their_layers() {
        assert_eq!(Rig::zombie().layer, "minecraft:zombie#main");
        assert_eq!(Rig::wolf().parts.len(), 5);
        assert_eq!(Rig::chicken().layer, "minecraft:chicken#main");
        assert!(Rig::humanoid().parts.iter().all(|(_, m)| *m != Motion::Raised));
        let look = MobLook::new("m", "m:x", Rig::zombie(), "m:entity/x").emissive("m:entity/x_e").scale(1.2);
        assert_eq!(look.emissive.as_deref(), Some("m:entity/x_e"));
        assert_eq!(look.scale, 1.2);
    }

    #[test]
    fn dying_tips_over_and_flashes() {
        assert_eq!(death_tilt(0, 0.5), 0.0);
        assert!(death_tilt(5, 0.0) > 0.0 && death_tilt(5, 0.0) < 90.0);
        assert_eq!(death_tilt(20, 0.0), 90.0);
        assert_eq!(tint(0, 0), 0xFFFFFF);
        assert_eq!(tint(3, 0), 0xFF8080);
        assert_eq!(tint(0, 1), 0xFF8080);
    }

    #[test]
    fn damage_kinds() {
        assert!(is_burning("minecraft:on_fire") && is_burning("minecraft:in_fire"));
        assert!(!is_burning("minecraft:lava"));
        assert!(is_melee("minecraft:mob_attack") && !is_melee("minecraft:arrow"));
        let r = MobRules::new("m", "m:x").no_conversion().fire_proof().on_hit("minecraft:slowness", 1, 60);
        assert!(r.no_conversion && r.fire_proof && r.on_hit.len() == 1);
    }

    fn carried() -> Carried {
        Carried {
            main_hand: Some("m:sword".into()),
            off_hand: Some("m:ring".into()),
            hotbar: vec![Some("m:sword".into()), None, Some("m:charm".into())],
            worn: [Some("m:helm".into()), Some("m:chest".into()), Some("m:legs".into()), None],
        }
    }

    #[test]
    fn places() {
        let c = carried();
        let rule = |items: &[&str], place| CarriedEffect::new("m", items, place, "minecraft:speed");
        assert!(applies(&rule(&["m:sword"], Place::MainHand), &c));
        assert!(!applies(&rule(&["m:ring"], Place::MainHand), &c));
        assert!(applies(&rule(&["m:ring"], Place::OffHand), &c));
        assert!(applies(&rule(&["m:ring"], Place::Hotbar), &c));
        assert!(applies(&rule(&["m:charm"], Place::Hotbar), &c));
        assert!(!applies(&rule(&["m:helm"], Place::Hotbar), &c));
        assert!(applies(&rule(&["m:nope", "m:legs"], Place::Worn), &c));
        assert!(applies(&rule(&["m:helm", "m:chest", "m:legs"], Place::AllWorn), &c));
        assert!(!applies(&rule(&["m:helm", "m:chest", "m:legs", "m:boots"], Place::AllWorn), &c));
        assert!(!applies(&rule(&[], Place::AllWorn), &c));
    }

    #[test]
    fn strongest_level_wins_and_lapsed_effects_go() {
        let a = CarriedEffect::new("m", &["x"], Place::Worn, "minecraft:speed");
        let b = CarriedEffect::new("m", &["y"], Place::Worn, "minecraft:speed").amplifier(1);
        let c = CarriedEffect::new("m", &["z"], Place::Worn, "minecraft:haste");
        let now = strongest([&a, &b, &c]);
        assert_eq!(now, vec![("minecraft:haste".to_string(), 0), ("minecraft:speed".to_string(), 1)]);
        let later = strongest([&c]);
        assert_eq!(lapsed(&now, &later), vec!["minecraft:speed".to_string()]);
        assert!(lapsed(&later, &now).is_empty());
        assert!(EFFECT_TICKS > REFRESH_TICKS as i32 * 2);
    }
}
