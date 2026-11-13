//! Armory Lib: shared native machinery for Horizon mods.
//!
//! A mod that names `armory_lib` under `[dependencies]` in its `horizon-mixin.toml` calls the
//! `register_*` functions from its own `hooks()`, once, at start. The library's hooks then do
//! the work each frame and tick:
//!
//! - [`register_mob_look`]: a mod entity type drawn with its own texture on a vanilla body.
//! - [`register_mob_rules`]: no conversion, no harm from burning, effects on a hit.
//! - [`register_carried_effect`]: an effect while an item is held, in the hotbar or worn.
//! - [`register_gear_light`]: items that light the world in their colour.
//! - [`register_page`]: a row in Options > Mods with one switch per feature; [`switch`] reads one.
//!
//! It uses only Horizon's catalog hooks and `Host`, so it has no mixin injections.
#![forbid(unsafe_code)]

mod logic;

pub use logic::{
    CarriedEffect, GearLight, HitEffect, ID, Motion, MobLook, MobRules, NAME, Page, Place, Rig,
    Switch, VERSION,
};

use horizon_client::mod_api::{
    EquipmentSlot, Flow, Hooks, Host, LivingConversionPreEvent, LivingIncomingDamageV3Event,
    MobSubmitEvent, ModelPart, ModsEvent, NativeColoredLight, NativeEffect, NativeMod,
    PlayerTickEvent, SettingsClickEvent, SettingsEvent, VisualsEvent,
};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------------
// What mods registered. Hooks run on the render thread and the server thread, and can run
// again inside themselves, so each lock is held only to copy out or to store.
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct Registry {
    lights: HashMap<String, GearLight>,
    looks: HashMap<String, MobLook>,
    rules: HashMap<String, MobRules>,
    carried: Vec<CarriedEffect>,
    pages: Vec<Page>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic in another hook must not take the library down with it.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn registry() -> MutexGuard<'static, Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    lock(R.get_or_init(Mutex::default))
}

/// Makes `light.item` glow. A later registration of the same item replaces the earlier one.
pub fn register_gear_light(light: GearLight) {
    registry().lights.insert(light.item.clone(), light);
}

/// Draws entity type `look.kind` with its own texture. A later registration replaces it.
pub fn register_mob_look(look: MobLook) {
    registry().looks.insert(look.kind.clone(), look);
}

/// Sets the rules of entity type `rules.kind`. A later registration replaces them.
pub fn register_mob_rules(rules: MobRules) {
    registry().rules.insert(rules.kind.clone(), rules);
}

/// Adds an effect a player has while carrying an item.
pub fn register_carried_effect(effect: CarriedEffect) {
    registry().carried.push(effect);
}

/// Adds a mod's row to Options > Mods, with its switches. One page a mod: a later
/// registration with the same `mod_id` replaces the earlier one.
pub fn register_page(page: Page) {
    let mut r = registry();
    r.pages.retain(|p| p.mod_id != page.mod_id);
    r.pages.push(page);
}

// ---------------------------------------------------------------------------------------
// Settings. The game keeps each setting in a file, so reads are kept for a second.
// ---------------------------------------------------------------------------------------

const SETTING_AGE: Duration = Duration::from_secs(1);

fn settings() -> MutexGuard<'static, HashMap<(String, String), (bool, Instant)>> {
    static S: OnceLock<Mutex<HashMap<(String, String), (bool, Instant)>>> = OnceLock::new();
    lock(S.get_or_init(Mutex::default))
}

fn default_of(mod_id: &str, key: &str) -> bool {
    registry()
        .pages
        .iter()
        .find(|p| p.mod_id == mod_id)
        .and_then(|p| p.switches.iter().find(|s| s.key == key))
        .is_none_or(|s| s.default)
}

/// Whether switch `key` of mod `mod_id`'s page is on. A key with no switch reads as on
/// unless its file says `false`. The answer is at most a second old.
pub fn switch(host: &dyn Host, mod_id: &str, key: &str) -> bool {
    let id = (mod_id.to_owned(), key.to_owned());
    if let Some((on, at)) = settings().get(&id)
        && at.elapsed() < SETTING_AGE
    {
        return *on;
    }
    let on = logic::read_switch(host.setting(mod_id, key).as_deref(), default_of(mod_id, key));
    settings().insert(id, (on, Instant::now()));
    on
}

/// Whether a registration is in force: its mod is on, and so is its switch if it names one.
fn in_force(host: &dyn Host, mod_id: &str, setting: Option<&str>) -> bool {
    host.on(mod_id) && setting.is_none_or(|key| switch(host, mod_id, key))
}

/// Which mods are on, as the last hook with a host saw them, for the hooks with none.
fn mods_on() -> MutexGuard<'static, HashMap<String, bool>> {
    static M: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    lock(M.get_or_init(Mutex::default))
}

// ---------------------------------------------------------------------------------------
// The hooks
// ---------------------------------------------------------------------------------------

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Lib::default())
}

/// A player the library gave effects to.
#[derive(Default)]
struct Wearer {
    ticks: u32,
    given: Vec<(String, u8)>,
}

#[derive(Default)]
struct Lib {
    /// Baked model layers by name, each the library's own copy to pose.
    models: Mutex<HashMap<String, Option<ModelPart>>>,
    wearers: Mutex<HashMap<u128, Wearer>>,
    /// Hits waiting for a hook with a host: the player hurt and the entity that did it.
    hits: Mutex<Vec<(u128, u32)>>,
}

const ARMOUR: [EquipmentSlot; 4] = [
    EquipmentSlot::Head,
    EquipmentSlot::Chest,
    EquipmentSlot::Legs,
    EquipmentSlot::Feet,
];
const HANDS: [EquipmentSlot; 2] = [EquipmentSlot::MainHand, EquipmentSlot::OffHand];

impl Lib {
    /// The light of the items `ids` yield for each slot, hands first.
    fn glow_of(
        host: &dyn Host,
        lights: &HashMap<String, GearLight>,
        mut item: impl FnMut(EquipmentSlot) -> Option<String>,
    ) -> Option<logic::Glow> {
        let mut pieces = Vec::new();
        for (slots, weight) in [(&HANDS[..], logic::HELD), (&ARMOUR[..], logic::WORN)] {
            for &slot in slots {
                let Some(light) = item(slot).and_then(|id| lights.get(&id)) else {
                    continue;
                };
                if in_force(host, &light.mod_id, light.setting.as_deref()) {
                    pieces.push((light.rgb, weight * light.level));
                }
            }
        }
        logic::blend(pieces)
    }

    fn draw_look(&self, host: &dyn Host, look: &MobLook, event: &mut MobSubmitEvent<'_>) -> bool {
        let mob = event.mob;
        let mut models = lock(&self.models);
        let model = models
            .entry(look.rig.layer.clone())
            .or_insert_with(|| host.bake_layer(&look.rig.layer));
        let Some(model) = model.as_mut() else {
            return false;
        };
        let frame = logic::Frame {
            age: mob.age,
            head_yaw: mob.head_yaw,
            pitch: mob.pitch,
            walk_position: mob.walk_position,
            walk_speed: mob.walk_speed,
        };
        model.reset_all_poses();
        for (name, motion) in &look.rig.parts {
            if let Some(part) = find(model, name) {
                let [x, y, z] = logic::pose(*motion, &frame, name.starts_with("right"));
                part.x_rot += x;
                part.y_rot += y;
                part.z_rot += z;
            }
        }
        let draw = &mut *event.draw;
        draw.push();
        let tilt = logic::death_tilt(mob.death_time, mob.partial);
        if tilt > 0.0 {
            draw.rotate_about([0.0, 0.0, 1.0], tilt, [0.0, 0.0, 0.0]);
        }
        // The mob's frame into the model's (+y down, front towards -z), as
        // `LivingEntityRenderer` does, at the look's size (a baby at half of it).
        let size = look.scale * if mob.baby { 0.5 } else { 1.0 };
        draw.scale([size, -size, -size]);
        draw.translate([0.0, -1.501, 0.0]);
        let tint = logic::tint(mob.hurt_time, mob.death_time);
        let drawn = host.draw_model_part(draw, model, &look.texture, tint);
        if drawn && let Some(emissive) = &look.emissive {
            // The glowing pixels again, a hair larger so they sit over the body, fully lit.
            let light = draw.light;
            draw.light = 0xF0F0;
            grow(model, 1.004);
            host.draw_model_part(draw, model, emissive, 0xFFFFFF);
            draw.light = light;
        }
        draw.pop();
        drawn
    }

    /// What the acting player carries, by item id.
    fn carried(host: &mut dyn Host) -> logic::Carried {
        let id = |host: &dyn Host, slot| host.equipment(slot).map(|s| s.item.to_string());
        logic::Carried {
            main_hand: id(&*host, EquipmentSlot::MainHand),
            off_hand: id(&*host, EquipmentSlot::OffHand),
            hotbar: (0..9)
                .map(|slot| host.carried_stack(slot).map(|s| s.item.to_string()))
                .collect(),
            worn: ARMOUR.map(|slot| id(&*host, slot)),
        }
    }
}

/// The part named `name` anywhere under `part`.
fn find<'a>(part: &'a mut ModelPart, name: &str) -> Option<&'a mut ModelPart> {
    if part.get_child(name).is_some() {
        return part.get_child_mut(name);
    }
    part.children_mut().find_map(|(_, child)| find(child, name))
}

/// Scales the parts right under `part` (and with them their children) by `by`, each about
/// its own pivot.
fn grow(part: &mut ModelPart, by: f32) {
    for (_, child) in part.children_mut() {
        child.x_scale *= by;
        child.y_scale *= by;
        child.z_scale *= by;
    }
}

impl Hooks for Lib {
    /// Options > Mods: a row for each registered page.
    fn mods(&self, _host: &mut dyn Host, event: &mut ModsEvent<'_>) {
        let pages = registry().pages.clone();
        for page in pages {
            if event.mods.iter().any(|m| m.id == page.mod_id) {
                continue;
            }
            event.mods.push(NativeMod {
                id: page.mod_id,
                title: page.title,
                description: page.description,
                keys: Vec::new(),
            });
        }
    }

    /// A registered page's buttons.
    fn settings(&self, host: &mut dyn Host, event: &mut SettingsEvent<'_>) {
        let page = registry().pages.iter().find(|p| p.mod_id == event.mod_id).cloned();
        let Some(page) = page else {
            return;
        };
        for s in &page.switches {
            let on = logic::read_switch(host.setting(&page.mod_id, &s.key).as_deref(), s.default);
            event.buttons.push(logic::switch_label(&s.label, on));
        }
    }

    /// A click flips the switch and keeps it.
    fn settings_click(&self, host: &mut dyn Host, event: &mut SettingsClickEvent<'_>) {
        let page = registry().pages.iter().find(|p| p.mod_id == event.mod_id).cloned();
        let Some(s) = page.as_ref().and_then(|p| p.switches.get(event.button)) else {
            return;
        };
        let on = !logic::read_switch(host.setting(event.mod_id, &s.key).as_deref(), s.default);
        host.set_setting(event.mod_id, &s.key, if on { "true" } else { "false" });
        settings().insert((event.mod_id.to_owned(), s.key.clone()), (on, Instant::now()));
    }

    /// Once a world frame: the lights of glowing gear on the player, on the creatures
    /// around and on the ground. They are frame-local, so they go out at once.
    fn visuals(&self, host: &dyn Host, event: &mut VisualsEvent<'_>) {
        let lights = registry().lights.clone();
        if lights.is_empty() {
            return;
        }
        let Some(state) = host.client_state() else {
            return;
        };
        let eye = match (state.eye_position, host.player()) {
            (Some(eye), _) => eye,
            (None, Some(p)) => [p.x, p.y + 1.6, p.z],
            (None, None) => return,
        };
        let partial = state.world_partial_tick;
        let mut found: Vec<([f64; 3], logic::Glow)> = Vec::new();

        let own = Self::glow_of(host, &lights, |slot| {
            host.equipment(slot).map(|s| s.item.to_string())
        });
        if let Some(glow) = own {
            found.push(([eye[0], eye[1] - 0.4, eye[2]], glow));
        }

        let near = |at: [f64; 3]| {
            (0..3).map(|i| (at[i] - eye[i]).powi(2)).sum::<f64>() <= logic::REACH * logic::REACH
        };
        for e in host.client_entities() {
            let at = e.position_at(partial);
            if !e.alive || e.invisible || e.spectator || !near(at) {
                continue;
            }
            let Some(living) = host.entity(e.id).and_then(|r| host.living(r)) else {
                continue;
            };
            let glow = Self::glow_of(host, &lights, |slot| {
                host.living_equipment(living, slot).map(|s| s.item.to_string())
            });
            if let Some(glow) = glow {
                found.push(([at[0], at[1] + f64::from(e.height) * 0.6, at[2]], glow));
            }
        }

        for item in host.client_items() {
            let at = item.position_at(partial);
            let Some(light) = lights.get(&*item.stack.item) else {
                continue;
            };
            if near(at)
                && in_force(host, &light.mod_id, light.setting.as_deref())
                && let Some(glow) = logic::dropped(light.rgb, light.level)
            {
                found.push(([at[0], at[1] + 0.25, at[2]], glow));
            }
        }

        logic::nearest(eye, &mut found, logic::MAX_LIGHTS);
        event.lights.extend(found.into_iter().map(|(at, glow)| NativeColoredLight {
            at,
            rgb: glow.rgb,
            radius: glow.radius,
            intensity: glow.intensity,
        }));
    }

    /// A registered entity type is drawn with its look in place of its template's model.
    fn mob_submit(&self, host: &dyn Host, event: &mut MobSubmitEvent<'_>) -> Flow {
        let look = registry().looks.get(event.mob.kind).cloned();
        match look {
            Some(look) if host.on(&look.mod_id) && self.draw_look(host, &look, event) => {
                Flow::Cancel
            }
            _ => Flow::Continue,
        }
    }

    /// A registered entity type keeps its type.
    fn living_conversion_pre(
        &self,
        host: &mut dyn Host,
        event: &mut LivingConversionPreEvent<'_>,
    ) -> Flow {
        let stays = registry()
            .rules
            .get(event.kind)
            .is_some_and(|r| r.no_conversion && host.on(&r.mod_id));
        if stays { Flow::Cancel } else { Flow::Continue }
    }

    /// Burning does a fire-proof type no harm; a hit by a type with hit effects is noted
    /// for the hurt player's next tick (this hook has no host).
    fn living_incoming_damage_v3(&self, event: &mut LivingIncomingDamageV3Event<'_>) -> Flow {
        let on = |mod_id: &str| mods_on().get(mod_id).copied().unwrap_or(true);
        if logic::is_burning(event.damage_type) {
            let proof = registry()
                .rules
                .get(event.kind)
                .is_some_and(|r| r.fire_proof && on(&r.mod_id));
            if proof {
                return Flow::Cancel;
            }
        }
        if logic::is_melee(event.damage_type)
            && let (Some(player), Some(attacker)) = (event.victim_uuid, event.attacker)
            && registry().rules.values().any(|r| !r.on_hit.is_empty())
        {
            let mut hits = lock(&self.hits);
            if hits.len() < 256 {
                hits.push((player, attacker));
            }
        }
        Flow::Continue
    }

    /// Each player's server tick: the effects of what they carry, and of the hits they took.
    fn player_tick(&self, host: &mut dyn Host, _event: &mut PlayerTickEvent) {
        let Some(me) = host.acting_player() else {
            return;
        };
        let (rules, carried_rules) = {
            let r = registry();
            (r.rules.clone(), r.carried.clone())
        };
        {
            let mut on = mods_on();
            for mod_id in rules.values().map(|r| &r.mod_id) {
                on.insert(mod_id.clone(), host.on(mod_id));
            }
        }

        // Hits on this player, now that there is a host to ask who dealt them.
        let mine: Vec<u32> = {
            let mut hits = lock(&self.hits);
            let mine = hits.iter().filter(|(p, _)| *p == me.uuid).map(|(_, a)| *a).collect();
            hits.retain(|(p, _)| *p != me.uuid);
            mine
        };
        for attacker in mine {
            let kind = host.entity(attacker).and_then(|e| host.entity_kind(e));
            let Some(rule) = kind.and_then(|k| rules.get(&k)) else {
                continue;
            };
            if !host.on(&rule.mod_id) {
                continue;
            }
            for hit in &rule.on_hit {
                host.add_effect(
                    me.uuid,
                    &NativeEffect {
                        id: hit.effect.clone(),
                        amplifier: hit.amplifier,
                        duration: hit.duration,
                        ambient: false,
                        particles: true,
                    },
                );
            }
        }

        if carried_rules.is_empty() {
            return;
        }
        let carried = Self::carried(host);
        let active: Vec<&CarriedEffect> = carried_rules
            .iter()
            .filter(|r| logic::applies(r, &carried))
            .filter(|r| in_force(host, &r.mod_id, r.setting.as_deref()))
            .collect();
        let now = logic::strongest(active);
        let (gone, give) = {
            let mut wearers = lock(&self.wearers);
            let w = wearers.entry(me.uuid).or_default();
            let gone = logic::lapsed(&w.given, &now);
            let give = w.given != now || w.ticks % logic::REFRESH_TICKS == 0;
            w.ticks = w.ticks.wrapping_add(1);
            w.given = now.clone();
            if now.is_empty() {
                wearers.remove(&me.uuid);
            }
            (gone, give)
        };
        for id in gone {
            host.remove_effect(me.uuid, &id);
        }
        if give {
            for (id, amplifier) in now {
                host.add_effect(
                    me.uuid,
                    &NativeEffect {
                        id,
                        amplifier,
                        duration: logic::EFFECT_TICKS,
                        ambient: true,
                        particles: false,
                    },
                );
            }
        }
    }
}
