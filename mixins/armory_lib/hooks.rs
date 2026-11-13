// Armory Lib's catalog hooks, added to the game as the public module
// `horizon_client::game::armory_lib`.
//
// Another native mod's mixin code can call the `register_*` functions (its bundle then needs
// Armory Lib installed beside it). The library's hooks do the work each frame and tick:
//
// - `register_mob_look`: a mod entity type drawn with its own texture on a vanilla body.
// - `register_mob_rules`: no conversion, no burning in daylight, no harm from fire, effects
//   on a hit.
// - `register_carried_effect`: an effect while an item is held, in the hotbar or worn.
// - `register_gear_light`: items that light the world in their colour.
// - `register_page`: a row in Options > Mods with one switch per feature; `switch` reads one.
//
// A content mod needs no code at all: it declares the same things, and its gear rules
// (armour set bonuses, tool abilities, harvest sweeps, weapon effects), in
// `data/<its id>/armory_lib.json`, which the library reads.
//
// The hooks are Horizon's catalog hooks with `Host`, reached by the injections in
// `horizon-mixin.toml` at the head of the game's own `native_*` functions (the `on_*`
// functions at the end of this file), so the library needs no `[api]` package and the
// launcher builds the game as it is. The gear rules are in `gear.rs`, what both share in
// `bridge.rs`, the sums in `logic.rs`.

pub use super::armory_lib_logic::{
    CarriedEffect, GearLight, HitEffect, ID, Motion, MobLook, MobRules, NAME, Page, Place, Rig,
    Switch, VERSION,
};

use super::armory_lib_logic as logic;
use super::{BlockEntityDraw, NativeMobRender};
use crate::mod_api::{
    ClientResourcesReloadedEvent, EquipmentSlot, Flow, Hooks, Host, LivingConversionPreEvent,
    LivingIncomingDamageV3Event, MobSubmitEvent, ModelPart, ModsEvent, NativeBlockRim,
    NativeColoredLight, NativeEffect, NativeMod, PlayerTickEvent, ServerDataLoadedEvent,
    SettingsClickEvent, SettingsEvent, VisualsEvent,
};
use horizon_client::armory_lib_bridge as bridge;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------------
// What mods registered. Hooks run on the render thread and the server thread, and can run
// again inside themselves, so each lock is held only to copy out or to store.
// ---------------------------------------------------------------------------------------

/// What is in force: the mods' files overlaid by what was registered from code.
#[derive(Clone, Default)]
struct Registry {
    lights: HashMap<String, GearLight>,
    looks: HashMap<String, MobLook>,
    rules: HashMap<String, MobRules>,
    carried: Vec<CarriedEffect>,
    pages: Vec<Page>,
    /// Rare variants' looks by variant name (`mymod:gold_chicken`): from the files only.
    variant_looks: HashMap<String, MobLook>,
}

/// Where the registry comes from: `api` is what `register_*` was given, `files` what the
/// content mods' `data/<mod>/armory_lib.json` declare, read again when the data reloads.
#[derive(Default)]
struct Sources {
    api: Registry,
    files: Registry,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic in another hook must not take the library down with it.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn registry() -> MutexGuard<'static, Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    lock(R.get_or_init(Mutex::default))
}

fn sources() -> MutexGuard<'static, Sources> {
    static S: OnceLock<Mutex<Sources>> = OnceLock::new();
    lock(S.get_or_init(Mutex::default))
}

/// Puts the two sources together again. Code wins where both name the same thing.
fn rebuild(sources: &Sources) {
    let mut all = sources.files.clone();
    let api = &sources.api;
    all.lights.extend(api.lights.clone());
    all.looks.extend(api.looks.clone());
    all.rules.extend(api.rules.clone());
    all.carried.extend(api.carried.iter().cloned());
    for page in &api.pages {
        all.pages.retain(|p| p.mod_id != page.mod_id);
        all.pages.push(page.clone());
    }
    spare_from_sun(&all.rules, |_| true);
    *registry() = all;
}

/// Makes `light.item` glow. A later registration of the same item replaces the earlier one.
pub fn register_gear_light(light: GearLight) {
    let mut s = sources();
    s.api.lights.insert(light.item.clone(), light);
    rebuild(&s);
}

/// Draws entity type `look.kind` with its own texture. A later registration replaces it.
pub fn register_mob_look(look: MobLook) {
    let mut s = sources();
    s.api.looks.insert(look.kind.clone(), look);
    rebuild(&s);
}

/// Sets the rules of entity type `rules.kind`. A later registration replaces them.
pub fn register_mob_rules(rules: MobRules) {
    let mut s = sources();
    s.api.rules.insert(rules.kind.clone(), rules);
    rebuild(&s);
}

/// Tells the bridge which types daylight must not light: those with `no_sunburn` whose
/// mod is `on`.
fn spare_from_sun(rules: &HashMap<String, MobRules>, on: impl Fn(&str) -> bool) {
    let mut kinds: Vec<String> = rules
        .values()
        .filter(|r| r.no_sunburn && on(&r.mod_id))
        .map(|r| r.kind.clone())
        .collect();
    kinds.sort();
    bridge::set_no_sunburn(kinds);
}

/// Adds an effect a player has while carrying an item.
pub fn register_carried_effect(effect: CarriedEffect) {
    let mut s = sources();
    s.api.carried.push(effect);
    rebuild(&s);
}

/// Adds a mod's row to Options > Mods, with its switches. One page a mod: a later
/// registration with the same `mod_id` replaces the earlier one.
pub fn register_page(page: Page) {
    let mut s = sources();
    s.api.pages.retain(|p| p.mod_id != page.mod_id);
    s.api.pages.push(page);
    rebuild(&s);
}

// ---------------------------------------------------------------------------------------
// The mods' files. A content mod declares what its items and creatures do in
// `data/<its id>/armory_lib.json` (the README has the format), so it needs no code of its
// own. They are read on the first hook with a host and again when the data reloads.
// ---------------------------------------------------------------------------------------

static LOADED: AtomicBool = AtomicBool::new(false);

fn ensure_loaded(host: &dyn Host) {
    if !LOADED.swap(true, Ordering::AcqRel) {
        load(host);
    }
}

fn load(host: &dyn Host) {
    let mut files = Registry::default();
    let mut gear = bridge::Gear::default();
    let mut problems = Vec::new();
    for id in bridge::content_mod_ids(&host.instance_dir().join("horizon-mods")) {
        let Some(bytes) = host.data(&format!("data/{id}/armory_lib.json")) else {
            continue;
        };
        let file: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(file) => file,
            Err(e) => {
                problems.push(format!("{id}: armory_lib.json is not JSON: {e}"));
                continue;
            }
        };
        let declared = logic::read_file(&id, &file);
        if !declared.usable {
            problems.extend(declared.problems);
            continue;
        }
        problems.extend(declared.problems);
        problems.extend(bridge::read_gear(&id, &file, &mut gear));
        // ---- Batch B: backpacks and soul jar (read here) ----

        // ---- Batch B: waystones and recall charm (read here) ----
        problems.extend(super::armory_lib_ways::read(&id, &file));

        // ---- Batch B: trinket slots (read here) ----

        files.lights.extend(declared.lights.into_iter().map(|l| (l.item.clone(), l)));
        files.looks.extend(declared.looks.into_iter().map(|l| (l.kind.clone(), l)));
        files.rules.extend(declared.rules.into_iter().map(|r| (r.kind.clone(), r)));
        files.carried.extend(declared.carried);
        files.pages.extend(declared.page);
        files.variant_looks.extend(declared.variant_looks);
    }
    for problem in problems {
        eprintln!("Armory Lib: {problem}");
    }
    bridge::set_gear(gear);
    bridge::forget_sightings();
    let mut s = sources();
    s.files = files;
    rebuild(&s);
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

/// The library's one set of hooks, made on first use.
fn lib() -> &'static Lib {
    static LIB: OnceLock<Lib> = OnceLock::new();
    LIB.get_or_init(Lib::default)
}

/// Whether the player has Armory Lib on in Options > Mods (as a trait mod, the game skipped
/// its hooks while it was off).
fn lib_on(host: &dyn Host) -> bool {
    let on = host.on(ID);
    LIB_ON.store(on, Ordering::Relaxed);
    on
}

/// What [`lib_on`] last found, for the one hook with no host.
static LIB_ON: AtomicBool = AtomicBool::new(true);

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
    /// A world opened or `/reload` ran: the mods' files are read again.
    fn server_data_loaded(&self, host: &mut dyn Host, _event: &mut ServerDataLoadedEvent) {
        LOADED.store(true, Ordering::Release);
        load(host);
    }

    /// The resource packs reloaded: the mods' files are read again.
    fn client_resources_reloaded(
        &self,
        host: &mut dyn Host,
        _event: &mut ClientResourcesReloadedEvent,
    ) {
        LOADED.store(true, Ordering::Release);
        load(host);
    }

    /// Options > Mods: a row for each registered page.
    fn mods(&self, host: &mut dyn Host, event: &mut ModsEvent<'_>) {
        ensure_loaded(host);
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
        ensure_loaded(host);
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
        ensure_loaded(host);
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
        ensure_loaded(host);
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
            && registry().rules.values().any(MobRules::hits_do_something)
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
        ensure_loaded(host);
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
        spare_from_sun(&rules, |mod_id| host.on(mod_id));

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
            if rule.ignite_ticks > 0 {
                // Set alight in `gear.rs`, which runs next with the player's body.
                bridge::queue_ignite(me.uuid, rule.ignite_ticks);
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

// ---------------------------------------------------------------------------------------
// Where the game reaches the library: each is called by an injection at the head of the
// game's own `native_*` function of the same name, before the trait mods' hooks, with the
// game as the host. A flag the library sets (cancel, drawn) is one those hooks see set.
// ---------------------------------------------------------------------------------------

/// `native_server_data_loaded`. The files are read whether or not the library is on, so
/// they are current when it is turned on again.
pub(crate) fn on_server_data_loaded(host: &mut dyn Host, reload: bool) {
    lib().server_data_loaded(host, &mut ServerDataLoadedEvent { reload });
}

/// `native_client_resources_reloaded`. Read whether or not the library is on, as above.
pub(crate) fn on_client_resources_reloaded(host: &mut dyn Host) {
    lib().client_resources_reloaded(host, &mut ClientResourcesReloadedEvent {});
}

/// `native_mods`.
pub(crate) fn on_mods(host: &mut dyn Host, mods: &mut Vec<NativeMod>) {
    if lib_on(host) {
        lib().mods(host, &mut ModsEvent { mods });
    }
}

/// `native_settings`.
pub(crate) fn on_settings(host: &mut dyn Host, mod_id: &str, buttons: &mut Vec<String>) {
    if lib_on(host) {
        lib().settings(host, &mut SettingsEvent { mod_id, buttons });
    }
}

/// `native_settings_click`.
pub(crate) fn on_settings_click(host: &mut dyn Host, mod_id: &str, button: usize) {
    if lib_on(host) {
        lib().settings_click(host, &mut SettingsClickEvent { mod_id, button });
    }
}

/// `native_visuals`.
pub(crate) fn on_visuals(
    host: &dyn Host,
    rims: &mut Vec<NativeBlockRim>,
    lights: &mut Vec<NativeColoredLight>,
) {
    if lib_on(host) {
        lib().visuals(host, &mut VisualsEvent { rims, lights });
    }
}

/// `native_mob_submit`: `drawn` is set when the library drew the mob, a rare variant with
/// its variant's look, a mod's type with its type's.
pub(super) fn on_mob_submit(
    game: &super::Game<'_>,
    mob: &NativeMobRender<'_>,
    draw: &mut BlockEntityDraw,
    drawn: &mut bool,
) {
    let host: &dyn Host = game;
    if *drawn || !lib_on(host) {
        return;
    }
    if let Some(look) = variant_look(game, mob)
        && lib().draw_look(host, &look, &mut MobSubmitEvent { mob, draw })
    {
        *drawn = true;
        return;
    }
    if lib().mob_submit(host, &mut MobSubmitEvent { mob, draw }) == Flow::Cancel {
        *drawn = true;
    }
}

/// The look of the rare variant `mob` is, when it is one with a look and its rule is in
/// force. Its UUID is the one the game keeps for it (the server's sweep gives one to every
/// mob of a type with variants; a client has the server's).
fn variant_look(game: &super::Game<'_>, mob: &NativeMobRender<'_>) -> Option<MobLook> {
    let gear = bridge::gear();
    if !gear.has_variants(mob.kind) {
        return None;
    }
    let words = *game.persist.uuids.get(&mob.id)?;
    let variant = gear.variant_of(mob.kind, bridge::uuid_from_words(words))?;
    let look = registry().variant_looks.get(&variant.name).cloned()?;
    in_force(game, &variant.mod_id, variant.setting.as_deref()).then_some(look)
}

/// `native_living_conversion_pre`: `cancel` is set when the mob keeps its type.
pub(crate) fn on_living_conversion_pre(
    host: &mut dyn Host,
    entity: u32,
    kind: &str,
    outcome: &str,
    cancel: &mut bool,
) {
    if *cancel || !lib_on(host) {
        return;
    }
    let mut event = LivingConversionPreEvent { entity, kind, outcome };
    if lib().living_conversion_pre(host, &mut event) == Flow::Cancel {
        *cancel = true;
    }
}

/// `native_living_incoming_damage_v3` (it has no host).
#[allow(clippy::too_many_arguments)]
pub(crate) fn on_living_incoming_damage_v3(
    victim: u32,
    victim_uuid: Option<u128>,
    kind: &str,
    attacker: Option<u32>,
    attacker_uuid: Option<u128>,
    direct: Option<u32>,
    damage_type: &str,
    amount: &mut f32,
    cancel: &mut bool,
) {
    if *cancel || !LIB_ON.load(Ordering::Relaxed) {
        return;
    }
    let mut event = LivingIncomingDamageV3Event {
        victim,
        victim_uuid,
        kind,
        attacker,
        attacker_uuid,
        direct,
        damage_type,
        amount: *amount,
    };
    if lib().living_incoming_damage_v3(&mut event) == Flow::Cancel {
        *cancel = true;
    }
    *amount = event.amount;
}

/// `native_player_tick`, with the ticking player swapped in.
pub(crate) fn on_player_tick(host: &mut dyn Host) {
    if lib_on(host) {
        lib().player_tick(host, &mut PlayerTickEvent {});
    }
}
