// The soul pool in the game, added as `game::armory_lib_souls`: every player's souls, the
// charged attacks that spend them, soul weapons, the trinkets and sets that change the pool, and
// creatures whose blows take souls. What the mods declare, and the sums, are in
// `armory_lib_souls_logic` (souls_logic.rs).
//
// - A player's souls are kept for each world in `<world>/armory_lib/souls/<uuid>.json`, apart
//   from the player's own file, so they stay through death. The file is written whenever the
//   count changes.
// - A kill by the player's blow (`native_mob_killed`) adds to the pool; a whole set that says
//   so adds more and fills the soul jar as many times.
// - A charged weapon is used over time like a bow (`native_use_duration`); let go after its
//   charge time (`native_release_using`) it slams or thrusts, for souls, then cools down.
// - Damage goes through the game's own damage hook (`native_living_incoming_damage`), which
//   runs whether or not a trait mod is loaded: a soul weapon's hit spends a soul for more
//   damage, a set's immunity cancels its damage, and a creature's blow that steals souls is
//   noted for the hurt player's next tick (that hook has no game).
use super::armory_lib_souls_logic as s;
use super::armory_lib_store as store;
use super::armory_lib_trinkets_logic as t;
use crate::game::Game;
use crate::game::native_player_state::NativeEffect;
use horizon_client::armory_lib_bridge as bridge;
use horizon_client::items::ItemStack;
use horizon_client::mod_api::NativeServerPlayer;
use horizon_client::particles::ParticleOptions;
use horizon_client::text::{self, Text};
use horizon_world::consumable::UseAnimation;
use horizon_world::player_inventory::EquipmentSlot;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic in another hook must not take the souls down with it.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The `SoundSource` ordinal of players.
const PLAYERS: usize = 7;
/// How long the pool's line stays between two showings, and the trinket slots between reads.
const EVERY_MS: u64 = 1000;
/// Effects a trinket gives last this long and are given again every two seconds.
const EFFECT_TICKS: i32 = 320;
const EFFECT_EVERY_MS: u64 = 2000;
/// A player not ticked for this long is forgotten by the damage hook.
const STALE_MS: u64 = 2000;

fn on(game: &Game<'_>, mod_id: &str, setting: Option<&str>) -> bool {
    super::armory_lib_gear::on(game, mod_id, setting)
}

/// What the library keeps of a player while they play.
#[derive(Default)]
struct Player {
    /// The pool's size as their last tick found it.
    max: u32,
    /// When their last charged attack went off (`bridge::now_ms`).
    charged_at: Option<u64>,
    shown_at: u64,
    /// The trinket bonus in their slots, read at `slots_at`.
    bonus: Option<s::TrinketBonus>,
    slots_at: Option<u64>,
    /// The effects the bonus gave, last given at `given_at`.
    given: Vec<String>,
    given_at: u64,
}

/// A player's body as the damage hook needs it (it has only entity ids).
struct Body {
    uuid: u128,
    /// Damage types (and effects) the set they wear keeps off them.
    immune: Vec<String>,
    /// Their soul weapon's hit is paid for: (souls, bonus damage). Used once a tick.
    armed: Option<(u32, f32)>,
    seen: u64,
}

static PLAYERS_SEEN: Mutex<Option<HashMap<u128, Player>>> = Mutex::new(None);
/// Souls by file, read the first time they are needed.
static POOLS: Mutex<Option<HashMap<PathBuf, u32>>> = Mutex::new(None);
static BODIES: Mutex<Option<HashMap<i32, Body>>> = Mutex::new(None);
/// Blows on players by creatures: (player's entity id, the creature's).
static HITS: Mutex<Vec<(i32, i32)>> = Mutex::new(Vec::new());
/// Souls soul weapons spent since their wielder's last tick: (player, souls).
static SPENT: Mutex<Vec<(u128, u32)>> = Mutex::new(Vec::new());

fn with_player<R>(uuid: u128, f: impl FnOnce(&mut Player) -> R) -> R {
    let mut all = lock(&PLAYERS_SEEN);
    let all = all.get_or_insert_with(HashMap::new);
    if all.len() > 1024 && !all.contains_key(&uuid) {
        all.clear();
    }
    f(all.entry(uuid).or_default())
}

// ---- The pool's file --------------------------------------------------------------------

fn file_of(game: &Game<'_>, uuid: u128) -> Option<PathBuf> {
    store::path(game, &format!("souls/{}.json", t::uuid_text(uuid)))
}

/// The player's souls and their file; None with no world to keep them in.
fn souls_of(game: &Game<'_>, uuid: u128) -> Option<(PathBuf, u32)> {
    let path = file_of(game, uuid)?;
    let mut pools = lock(&POOLS);
    let pools = pools.get_or_insert_with(HashMap::new);
    if let Some(n) = pools.get(&path) {
        return Some((path, *n));
    }
    let n = store::read_json(&path)
        .and_then(|v| v.get("souls")?.as_u64())
        .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX));
    if pools.len() > 4096 {
        pools.clear();
    }
    pools.insert(path.clone(), n);
    Some((path, n))
}

fn set_souls(path: &PathBuf, n: u32) {
    lock(&POOLS).get_or_insert_with(HashMap::new).insert(path.clone(), n);
    if let Err(e) = store::write_json(path, &serde_json::json!({ "souls": n })) {
        eprintln!("Armory Lib: souls not saved ({e})");
    }
}

fn pool_line(souls: u32, max: u32) -> Text {
    Text::translate_with_fallback(
        "armory_lib.souls.pool",
        "Souls %s/%s",
        &[Text::literal(souls.min(max).to_string()), Text::literal(max.to_string())],
    )
    .with_color(text::AQUA)
}

fn say(game: &mut Game<'_>, uuid: u128, line: Text) {
    game.native_action_bar(uuid, line);
    with_player(uuid, |p| p.shown_at = bridge::now_ms());
}

// ---- Each player tick -------------------------------------------------------------------

/// The trinket bonus in the player's trinket slots, from their file, read once a second.
fn slot_bonus(game: &Game<'_>, pool: &s::PoolDef, uuid: u128, now: u64) -> Option<s::TrinketBonus> {
    let fresh = with_player(uuid, |p| {
        p.slots_at
            .is_some_and(|at| now.saturating_sub(at) < EVERY_MS)
            .then(|| p.bonus.clone())
    });
    if let Some(bonus) = fresh {
        return bonus;
    }
    let slots = t::trinkets().slots;
    // While the trinkets menu is open the file keeps what it held as it opened, as the
    // trinkets' own modifiers do.
    let held: Vec<String> = store::path(game, &t::file_name(uuid))
        .map(|path| store::read_items(&path, t::ROW))
        .unwrap_or_default()
        .into_iter()
        .take(slots)
        .flatten()
        .map(|s| s.item.to_string())
        .collect();
    let bonus = pool
        .trinkets
        .iter()
        .filter(|b| b.items.iter().any(|i| held.contains(i)))
        .find(|b| on(game, &pool.mod_id, b.setting.as_deref()))
        .cloned();
    with_player(uuid, |p| {
        p.bonus = bonus.clone();
        p.slots_at = Some(now);
    });
    bonus
}

/// The whole set the player wears that the pool names, if its rule is in force.
fn worn_set(game: &Game<'_>, pool: &s::PoolDef) -> Option<s::SetBonus> {
    let worn: Vec<String> = [
        EquipmentSlot::Head,
        EquipmentSlot::Chest,
        EquipmentSlot::Legs,
        EquipmentSlot::Feet,
    ]
    .into_iter()
    .filter_map(|slot| game.native_equipment(slot).map(|s| s.item.to_string()))
    .collect();
    pool.sets
        .iter()
        .filter(|set| set.pieces.iter().all(|p| worn.contains(p)))
        .find(|set| on(game, &pool.mod_id, set.setting.as_deref()))
        .cloned()
}

/// `native_player_tick`, with the player swapped in: the trinket bonus's effects, the set's
/// immunity, whether their soul weapon's next hit is paid for, the souls their weapon spent and
/// creatures took, and the pool's line while they hold a weapon that uses it.
pub(super) fn on_player_tick(game: &mut Game<'_>) {
    if !game.native_authoritative() {
        return;
    }
    let all = s::souls();
    let Some(pool) = all.pool().cloned() else {
        return;
    };
    let Some(me) = game.native_acting_player() else {
        return;
    };
    let now = bridge::now_ms();
    let in_force = on(game, &pool.mod_id, pool.setting.as_deref());
    let bonus = in_force.then(|| slot_bonus(game, &pool, me.uuid, now)).flatten();
    let max = s::max_of(pool.max, bonus.as_ref().map(|b| b.max));
    keep_effects(game, me.uuid, bonus.as_ref().map_or(&[][..], |b| &b.effects), now);
    let set = in_force.then(|| worn_set(game, &pool)).flatten();
    let immune = set.map(|s| s.immune).unwrap_or_default();
    if !immune.is_empty()
        && let Some(effects) = game.native_effects(me.uuid)
    {
        for e in effects.iter().filter(|e| immune.contains(&e.id)) {
            game.native_remove_effect(me.uuid, &e.id);
        }
    }
    let Some((path, souls)) = souls_of(game, me.uuid) else {
        return;
    };
    with_player(me.uuid, |p| p.max = max);

    // Paid for and taken since their last tick.
    let spent: u32 = {
        let mut spent = lock(&SPENT);
        let mine = spent.iter().filter(|(u, _)| *u == me.uuid).map(|(_, n)| *n).sum();
        spent.retain(|(u, _)| *u != me.uuid);
        mine
    };
    let attackers: Vec<i32> = {
        let mut hits = lock(&HITS);
        let mine = hits.iter().filter(|(p, _)| *p == me.entity_id).map(|(_, a)| *a).collect();
        hits.retain(|(p, _)| *p != me.entity_id);
        mine
    };
    let mut stolen = 0;
    for attacker in attackers {
        let kind = u32::try_from(attacker)
            .ok()
            .and_then(|id| game.native_mob(&me.dimension, id))
            .map(|mob| mob.kind);
        if let Some((mod_id, n)) = kind.as_deref().and_then(|k| all.steal(k))
            && on(game, mod_id, None)
            && in_force
        {
            stolen += n;
        }
    }
    let now_souls = s::take(s::take(souls, spent, u32::MAX), stolen, max);
    if now_souls != souls {
        set_souls(&path, now_souls);
    }
    if stolen > 0 {
        say(game, me.uuid, pool_line(now_souls, max).with_color(text::RED));
    }

    // Their weapon: paid for when its rule is in force and the pool can pay.
    let held = game.native_equipment(EquipmentSlot::MainHand);
    let armed = held
        .as_ref()
        .and_then(|h| all.weapon(&h.item))
        .filter(|w| in_force && on(game, &w.mod_id, w.setting.as_deref()))
        .filter(|w| s::spend(now_souls, w.souls, max).is_some())
        .map(|w| (w.souls, w.bonus));
    {
        let mut bodies = lock(&BODIES);
        let bodies = bodies.get_or_insert_with(HashMap::new);
        bodies.retain(|_, b| now.saturating_sub(b.seen) < STALE_MS && b.uuid != me.uuid);
        if armed.is_some() || !immune.is_empty() {
            bodies.insert(me.entity_id, Body { uuid: me.uuid, immune, armed, seen: now });
        }
    }

    let shows = held
        .as_ref()
        .is_some_and(|h| all.charge(&h.item).is_some() || all.weapon(&h.item).is_some());
    let due = with_player(me.uuid, |p| now.saturating_sub(p.shown_at) >= EVERY_MS);
    if in_force && shows && due {
        say(game, me.uuid, pool_line(now_souls, max));
    }
}

/// Keeps `effects` on the player while a trinket gives them, and takes those it gave away
/// when it no longer does (only what looks like its own: level I, no longer than it gives).
fn keep_effects(game: &mut Game<'_>, uuid: u128, effects: &[String], now: u64) {
    let (gone, give) = with_player(uuid, |p| {
        let gone: Vec<String> = p.given.iter().filter(|e| !effects.contains(e)).cloned().collect();
        let give = !effects.is_empty()
            && (p.given != effects || now.saturating_sub(p.given_at) >= EFFECT_EVERY_MS);
        if give {
            p.given_at = now;
        }
        p.given = effects.to_vec();
        (gone, give)
    });
    if !gone.is_empty()
        && let Some(now) = game.native_effects(uuid)
    {
        for id in gone {
            let ours = now
                .iter()
                .any(|n| n.id == id && n.amplifier == 0 && (0..=EFFECT_TICKS).contains(&n.duration));
            if ours {
                game.native_remove_effect(uuid, &id);
            }
        }
    }
    if give {
        for id in effects {
            let effect = NativeEffect {
                id: id.clone(),
                amplifier: 0,
                duration: EFFECT_TICKS,
                ambient: true,
                particles: false,
            };
            game.native_add_effect(uuid, &effect);
        }
    }
}

// ---- Kills ------------------------------------------------------------------------------

/// `native_mob_killed`, with the player swapped in: their blow killed `entity`. The pool takes
/// its souls; a whole set that takes more fills their soul jar as many times.
pub(super) fn on_mob_killed(game: &mut Game<'_>, player: u128, entity: u32) {
    if !game.native_authoritative() {
        return;
    }
    let Some(pool) = s::souls().pool().cloned() else {
        return;
    };
    if game.native_acting_player().is_none_or(|me| me.uuid != player)
        || !on(game, &pool.mod_id, pool.setting.as_deref())
    {
        return;
    }
    let set = worn_set(game, &pool);
    let per_kill = set.as_ref().map_or(pool.per_kill, |s| s.per_kill);
    let max = with_player(player, |p| p.max).max(pool.max);
    if let Some((path, souls)) = souls_of(game, player) {
        let now = s::add(souls, per_kill, max);
        if now != souls {
            set_souls(&path, now);
        }
    }
    // The jar's own hook took one soul already.
    if set.is_some() {
        for _ in 1..per_kill {
            super::armory_lib_packs::on_mob_killed(game, player, entity);
        }
    }
}

// ---- Charged attacks --------------------------------------------------------------------

/// The charged attack rule of `stack`, if it is in force.
fn charge_of(game: &Game<'_>, stack: &ItemStack) -> Option<s::ChargeDef> {
    s::souls()
        .charge(&stack.item)
        .filter(|c| on(game, &c.mod_id, c.setting.as_deref()))
        .cloned()
}

/// `native_use_duration` (both sides, no user): a charged weapon is held until let go. A use
/// the item has of its own stays.
pub(super) fn on_use_duration(game: &Game<'_>, stack: &ItemStack, duration: &mut i32) {
    if *duration == 0 && charge_of(game, stack).is_some() {
        *duration = s::USE_TICKS;
    }
}

/// `native_use_animation` (the client): a charged weapon is raised as a trident is.
pub(super) fn on_use_animation(game: &Game<'_>, stack: &ItemStack, animation: &mut UseAnimation) {
    if matches!(animation, UseAnimation::None) && charge_of(game, stack).is_some() {
        *animation = UseAnimation::Trident;
    }
}

/// `native_use_item`, with the player swapped in: a charge starts. The pool is shown, or that
/// it can't pay. The use itself is left to the game.
pub(super) fn on_use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if *claimed || !game.native_authoritative() {
        return;
    }
    let Some(c) = charge_of(game, stack) else {
        return;
    };
    let Some(me) = game.native_acting_player() else {
        return;
    };
    let Some(pool) = s::souls().pool().cloned() else {
        return;
    };
    let Some((_, souls)) = souls_of(game, me.uuid) else {
        return;
    };
    let max = with_player(me.uuid, |p| p.max).max(pool.max);
    let line = if game.creative || s::spend(souls, c.souls, max).is_some() {
        pool_line(souls, max)
    } else {
        not_enough(c.souls)
    };
    say(game, me.uuid, line);
}

fn not_enough(cost: u32) -> Text {
    Text::translate_with_fallback(
        "armory_lib.souls.not_enough",
        "Not enough souls (%s needed)",
        &[Text::literal(cost.to_string())],
    )
    .with_color(text::RED)
}

/// `native_on_use_tick`: the charge is full; a sound says so.
pub(super) fn on_use_tick(game: &mut Game<'_>, stack: &ItemStack, remaining: i32) {
    if !game.native_authoritative() {
        return;
    }
    let Some(c) = charge_of(game, stack) else {
        return;
    };
    if s::USE_TICKS - remaining == c.charge_ticks
        && let Some(me) = game.native_acting_player()
    {
        let at = me.position;
        game.native_play_sound_in_place("minecraft:item.crossbow.loading_end", at, PLAYERS, 0.8, 0.7);
    }
}

/// `native_release_using`, with the player swapped in: a charged weapon let go. Held long
/// enough, off cooldown and paid for, it strikes; then it cools down as an ender pearl does.
pub(super) fn on_release_using(game: &mut Game<'_>, stack: &ItemStack, time_left: i32) {
    if !game.native_authoritative() {
        return;
    }
    let Some(c) = charge_of(game, stack) else {
        return;
    };
    let Some(me) = game.native_acting_player() else {
        return;
    };
    // Its damage is the main hand's: a charge from the off hand does nothing.
    let in_hand = game
        .native_equipment(EquipmentSlot::MainHand)
        .is_some_and(|h| h.item == stack.item);
    if !in_hand || s::USE_TICKS - time_left < c.charge_ticks {
        return;
    }
    let Some(pool) = s::souls().pool().cloned() else {
        return;
    };
    if !on(game, &pool.mod_id, pool.setting.as_deref()) {
        return;
    }
    let now = bridge::now_ms();
    let cooldown_ms = u64::try_from(c.cooldown_ticks).unwrap_or(0) * 50;
    let left = s::seconds_left(with_player(me.uuid, |p| p.charged_at), now, cooldown_ms);
    if left > 0 {
        let line = Text::translate_with_fallback(
            "armory_lib.souls.cooldown",
            "Ready again in %s s",
            &[Text::literal(left.to_string())],
        );
        say(game, me.uuid, line);
        return;
    }
    let Some((path, souls)) = souls_of(game, me.uuid) else {
        return;
    };
    let max = with_player(me.uuid, |p| p.max).max(pool.max);
    let cost = if game.creative { 0 } else { c.souls };
    let Some(left) = s::spend(souls, cost, max) else {
        say(game, me.uuid, not_enough(c.souls));
        return;
    };
    if left != souls {
        set_souls(&path, left);
    }
    strike(game, &me, &c);
    with_player(me.uuid, |p| p.charged_at = Some(now));
    if c.cooldown_ticks > 0 {
        game.add_cooldown(horizon_client::cooldowns::group(stack, None), c.cooldown_ticks);
    }
    say(game, me.uuid, pool_line(left, max));
}

/// The slam or thrust itself: every creature it reaches but villagers and the player's own
/// companions and pets takes the player's attack damage times the rule's multiplier, from the
/// player (so with its knockback, and the slam's push on top).
fn strike(game: &mut Game<'_>, me: &NativeServerPlayer, c: &s::ChargeDef) {
    let damage = game
        .native_attribute(me.uuid, "minecraft:attack_damage")
        .map_or(1.0, |(_, value)| value as f32)
        * c.multiplier;
    let dimension = me.dimension.clone();
    let feet = me.position;
    let centre = [feet[0], feet[1] + 0.9, feet[2]];
    let eye = [feet[0], feet[1] + f64::from(me.eye_height), feet[2]];
    let dir = s::look(me.yaw, me.pitch);
    let owner = Some(bridge::uuid_words(me.uuid));
    let caught: Vec<(u32, [f64; 3])> = game
        .mobs
        .iter()
        .filter(|m| !m.removed && m.alive() && m.extra.owner != owner && !s::spared(&m.kind.id))
        .map(|m| {
            let at = [m.position.x, m.position.y + f64::from(m.height()) * 0.5, m.position.z];
            (m.id, at)
        })
        .filter(|(_, at)| match c.attack {
            s::Attack::Slam => s::in_slam(centre, c.reach, *at),
            s::Attack::Thrust => s::on_line(eye, dir, c.reach, s::THRUST_WIDTH, *at),
        })
        .collect();
    let by = u32::try_from(me.entity_id).ok();
    for (id, at) in caught {
        let tags = game.native_entity(id).and_then(|e| game.native_entity_tags(e));
        if bridge::owner_from_tags(tags.iter().flatten()) == Some(me.uuid) {
            continue;
        }
        let hurt = game.native_hurt_entity(&dimension, id, damage, "minecraft:player_attack", by);
        if hurt && c.attack == s::Attack::Slam {
            game.native_push_mob(&dimension, id, s::push_from(centre, at));
        }
    }
    match c.attack {
        s::Attack::Slam => {
            let ground = [feet[0], feet[1] + 0.1, feet[2]];
            let r = c.reach * 0.5;
            particles(game, "minecraft:explosion", ground, [r * 0.5, 0.0, r * 0.5], 0.0, 3);
            particles(game, "minecraft:soul_fire_flame", ground, [r, 0.05, r], 0.03, 40);
            particles(game, "minecraft:soul", ground, [r, 0.2, r], 0.02, 12);
            game.native_play_sound_in_place("minecraft:item.mace.smash_ground_heavy", feet, PLAYERS, 1.0, 0.8);
        }
        s::Attack::Thrust => {
            let steps = c.reach.ceil() as i32;
            for i in 1..=steps {
                let d = f64::from(i);
                let at = [eye[0] + dir[0] * d, eye[1] - 0.2 + dir[1] * d, eye[2] + dir[2] * d];
                particles(game, "minecraft:soul_fire_flame", at, [0.1, 0.1, 0.1], 0.01, 3);
            }
            let tip = [eye[0] + dir[0] * 2.0, eye[1] - 0.2 + dir[1] * 2.0, eye[2] + dir[2] * 2.0];
            particles(game, "minecraft:sweep_attack", tip, [0.0, 0.0, 0.0], 0.0, 1);
            game.native_play_sound_in_place("minecraft:item.trident.throw", feet, PLAYERS, 1.0, 0.7);
        }
    }
}

fn particles(game: &mut Game<'_>, name: &str, at: [f64; 3], spread: [f64; 3], speed: f64, count: i32) {
    game.native_send_particles_in_place(&ParticleOptions::named(name), None, at, spread, speed, count);
}

// ---- Damage -----------------------------------------------------------------------------

/// `native_living_incoming_damage` (no game: the hit is in progress, on the server thread).
/// A player wearing an immune set takes none of that damage; a creature's blow on a player is
/// noted for their tick, when there is a game to ask what it is; and the first hit of a paid
/// soul weapon's wielder each tick does more damage and spends its souls.
pub(super) fn on_incoming_damage(
    entity: i32,
    kind: &str,
    damage_type: &str,
    causing: i32,
    amount: &mut f32,
    canceled: &mut bool,
) {
    if *canceled {
        return;
    }
    if kind == "minecraft:player" {
        let immune = lock(&BODIES)
            .as_ref()
            .and_then(|b| b.get(&entity))
            .is_some_and(|b| b.immune.iter().any(|d| d == damage_type));
        if immune {
            *canceled = true;
            return;
        }
        if causing > 0 && super::armory_lib_logic::is_melee(damage_type) && s::souls().has_steals() {
            let mut hits = lock(&HITS);
            if hits.len() < 256 {
                hits.push((entity, causing));
            }
        }
        return;
    }
    if damage_type != "minecraft:player_attack" {
        return;
    }
    // The player whose attack this is (a player's hit names no causing entity here).
    let attacker = horizon_client::health::knockback_attacker();
    let paid = {
        let mut bodies = lock(&BODIES);
        bodies
            .as_mut()
            .and_then(|b| b.get_mut(&attacker))
            .and_then(|b| b.armed.take().map(|(souls, bonus)| (b.uuid, souls, bonus)))
    };
    if let Some((uuid, souls, bonus)) = paid {
        *amount += bonus;
        lock(&SPENT).push((uuid, souls));
    }
}
