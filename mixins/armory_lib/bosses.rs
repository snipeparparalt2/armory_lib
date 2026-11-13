// Bosses in the game, added to the game as `game::armory_lib_bosses`: the seals and altars that
// raise them, their boss bars, phases and blows, the experience they drop, the music of the
// fight, and creatures drawn above the ground. What the mods' files declare, and the sums, are
// in `armory_lib_bosses_logic` (bosses_logic.rs).
//
// The server's part runs in each player's tick, with that player swapped in, so it works in
// single player, on a LAN host and on a dedicated server, and only where a player is near. Blows
// are heard in `native_living_damage_post`, which (unlike `native_living_incoming_damage_v3`
// and `native_living_death`) fires when no trait mod is installed; experience is added as the
// boss's loot drops. Phases are kept as an entity tag, so a boss reloaded mid-fight doesn't call
// its helpers twice.
use super::armory_lib_bosses_logic as b;
use crate::game::native_mobs::NativeSpawn;
use crate::game::native_player_state::NativeEffect;
use crate::game::{BlockEntityDraw, Game, NativeMobRender};
use horizon_client::armory_lib_bridge as bridge;
use horizon_client::items::ItemStack;
use horizon_client::mobs::Who;
use horizon_client::mobs::finalize::{self, Attr, Op};
use horizon_client::mod_api::{NativeBossBar, NativeLootDrops};
use horizon_client::music::Music;
use horizon_client::particles::ParticleOptions;
use horizon_client::text::Text;
use std::sync::{Mutex, MutexGuard};

/// The `SoundSource` ordinal of hostile creatures.
const HOSTILE: usize = 5;
/// The modifier a "leader" zombie gets (much more health, more reinforcements): not for a boss.
const LEADER: &str = "minecraft:leader_zombie_bonus";
const RISE_SOUND: &str = "minecraft:entity.warden.emerge";
const PHASE_SOUND: &str = "minecraft:entity.ravager.roar";

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn on(game: &Game<'_>, mod_id: &str, setting: Option<&str>) -> bool {
    super::armory_lib_gear::on(game, mod_id, setting)
}

/// Mobs' blows on players, for the hurt player's next tick: (player's entity id, mob's id).
static BLOWS: Mutex<Vec<(i32, u32)>> = Mutex::new(Vec::new());
/// The boss bars each player was shown last tick.
static BARS: Mutex<Vec<(u128, Vec<u128>)>> = Mutex::new(Vec::new());

/// `native_living_damage_post` (no game: the hit is in progress): a mob's blow hurt a player.
pub(super) fn on_damage_post(entity: i32, kind: &str, damage_type: &str, causing: i32, amount: f32) {
    if kind != "minecraft:player"
        || causing <= 0
        || !(amount > 0.0)
        || !super::armory_lib_logic::is_melee(damage_type)
        || !b::bosses().blows_matter()
    {
        return;
    }
    let mut blows = lock(&BLOWS);
    if blows.len() >= 256 {
        blows.remove(0);
    }
    blows.push((entity, causing as u32));
}

/// `native_player_tick`, each server tick of each player, with that player swapped in: the
/// blows they took from bosses; the bosses near them (bars, phases); once a second, the seals
/// near them.
pub(super) fn on_player_tick(game: &mut Game<'_>) {
    let all = b::bosses();
    if all.is_empty() || !game.native_authoritative() {
        return;
    }
    let Some(me) = game.native_acting_player() else {
        return;
    };
    blows_taken(game, &all, me.uuid, me.entity_id);
    let near: Vec<(u32, String)> = game
        .mobs
        .iter()
        .filter(|m| !m.removed && m.alive() && all.boss(&m.kind.id).is_some())
        .filter(|m| b::within([m.position.x, m.position.y, m.position.z], me.position, b::NEAR))
        .map(|m| (m.id, m.kind.id.clone()))
        .collect();
    let mut shown = Vec::new();
    for (id, kind) in near {
        let Some(boss) = all.boss(&kind) else {
            continue;
        };
        if !on(game, &boss.mod_id, boss.setting.as_deref()) {
            continue;
        }
        tidy(game, id);
        advance(game, &me.dimension, id, boss);
        let bar_id = b::bar_id(bridge::uuid_from_words(game.uuid_for(id)));
        if let Some(bar) = bar_of(game, id, boss)
            && game.native_boss_bar(me.uuid, bar_id, Some(&bar))
        {
            shown.push(bar_id);
        }
    }
    let before = {
        let mut bars = lock(&BARS);
        let before = match bars.iter_mut().find(|(p, _)| *p == me.uuid) {
            Some((_, list)) => std::mem::replace(list, shown.clone()),
            None => {
                bars.push((me.uuid, shown.clone()));
                Vec::new()
            }
        };
        bars.retain(|(_, list)| !list.is_empty());
        before
    };
    for gone in before.into_iter().filter(|id| !shown.contains(id)) {
        game.native_boss_bar(me.uuid, gone, None);
    }
    let tick = game.native_clock().map_or(0, |c| c.game_time);
    if (tick + i64::from(me.entity_id)).rem_euclid(20) == 0 {
        raise_from_seals(game, &all, &me.dimension, me.position);
    }
}

/// What the blows the player took since their last tick do, by the boss's phase.
fn blows_taken(game: &mut Game<'_>, all: &b::Bosses, uuid: u128, entity: i32) {
    let mine: Vec<u32> = {
        let mut blows = lock(&BLOWS);
        let mine = blows.iter().filter(|(p, _)| *p == entity).map(|(_, m)| *m).collect();
        blows.retain(|(p, _)| *p != entity);
        mine
    };
    for attacker in mine {
        let Some(kind) = game.mobs.iter().find(|m| m.id == attacker).map(|m| m.kind.id.clone()) else {
            continue;
        };
        let Some(boss) = all.boss(&kind) else {
            continue;
        };
        if !on(game, &boss.mod_id, boss.setting.as_deref()) {
            continue;
        }
        let blows = boss.blows(phases_begun(game, attacker));
        if blows.ignite_ticks > 0 {
            // Set alight in `gear.rs`'s player tick, as a mob rule's burning blow is.
            bridge::queue_ignite(uuid, blows.ignite_ticks);
        }
        for hit in &blows.on_hit {
            let effect = NativeEffect {
                id: hit.effect.clone(),
                amplifier: hit.amplifier,
                duration: hit.ticks,
                ambient: false,
                particles: true,
            };
            game.native_add_effect(uuid, &effect);
        }
    }
}

fn phases_begun(game: &Game<'_>, id: u32) -> usize {
    game.native_entity(id)
        .and_then(|handle| game.native_entity_tags(handle))
        .map_or(0, |tags| b::phases_from_tags(&tags))
}

/// A boss stays a boss: it never despawns, and none is a baby or a zombie "leader" (more
/// health, more reinforcements), nor calls copies of itself as a zombie does on Hard.
fn tidy(game: &mut Game<'_>, id: u32) {
    let Some(mob) = game.mobs.iter_mut().find(|m| m.id == id) else {
        return;
    };
    mob.persistent = true;
    let leader = finalize::has_modifier(mob, Attr::MaxHealth, LEADER)
        || finalize::has_modifier(mob, Attr::SpawnReinforcements, LEADER);
    if !mob.is_baby() && !leader && mob.attrs.spawn_reinforcements <= 0.0 {
        return;
    }
    finalize::set_baby(mob, false);
    finalize::remove_modifier(mob, Attr::MaxHealth, LEADER);
    finalize::remove_modifier(mob, Attr::SpawnReinforcements, LEADER);
    finalize::set_base(mob, Attr::SpawnReinforcements, 0.0);
    mob.health = mob.health.min(mob.attrs.max_health);
    mob.needs_sync = true;
}

/// Begins the phases the boss's health has reached since its tag was written: each calls its
/// helpers once and makes it faster.
fn advance(game: &mut Game<'_>, dimension: &str, id: u32, boss: &b::BossDef) {
    let Some(handle) = game.native_entity(id) else {
        return;
    };
    let tags = game.native_entity_tags(handle).unwrap_or_default();
    let begun = b::phases_from_tags(&tags);
    let Some((now, at, target)) = game.mobs.iter().find(|m| m.id == id).map(|m| {
        let now = boss.phases_at(m.health, m.attrs.max_health);
        (now, [m.position.x, m.position.y, m.position.z], m.ai.target)
    }) else {
        return;
    };
    if now <= begun {
        return;
    }
    for (n, phase) in boss.phases.iter().enumerate().take(now).skip(begun) {
        if let Some((kind, count)) = &phase.summon {
            summon(game, dimension, kind, *count, at, target);
        }
        if phase.speed != 0.0
            && let Some(mob) = game.mobs.iter_mut().find(|m| m.id == id)
        {
            let name = format!("armory_lib:boss_phase_{}", n + 1);
            finalize::add_modifier(mob, Attr::MovementSpeed, name, phase.speed, Op::AddMultipliedBase);
        }
    }
    game.native_play_sound_in_place(PHASE_SOUND, at, HOSTILE, 2.0, 0.8);
    for old in tags.iter().filter(|t| t.starts_with(b::PHASE_TAG)) {
        game.native_entity_remove_tag(handle, old);
    }
    game.native_entity_add_tag(handle, &b::phase_tag(now));
}

/// `count` creatures of `kind` around `at`, set on the boss's target.
fn summon(game: &mut Game<'_>, dimension: &str, kind: &str, count: u32, at: [f64; 3], target: Option<Who>) {
    for [dx, dz] in b::summon_spots(count) {
        let spot = [at[0] + dx, at[1], at[2] + dz];
        let spot = if open(game, spot) { spot } else { at };
        if let Some(new) = game.native_spawn_mob(dimension, kind, spot, &NativeSpawn::default())
            && let Some(mob) = game.mobs.iter_mut().find(|m| m.id == new)
        {
            mob.ai.target = target;
        }
    }
    puff(game, at);
}

/// Whether a creature fits at `at`: air at its feet and head.
fn open(game: &Game<'_>, at: [f64; 3]) -> bool {
    let [x, y, z] = at.map(|c| c.floor() as i32);
    (0..2).all(|dy| game.res.behaviours().is_air(game.world.block(x, y + dy, z)))
}

fn puff(game: &mut Game<'_>, at: [f64; 3]) {
    let smoke = ParticleOptions::named("minecraft:large_smoke");
    game.native_send_particles_in_place(&smoke, None, [at[0], at[1] + 1.0, at[2]], [0.6, 0.8, 0.6], 0.02, 30);
}

fn bar_of(game: &Game<'_>, id: u32, boss: &b::BossDef) -> Option<NativeBossBar> {
    let mob = game.mobs.iter().find(|m| m.id == id)?;
    Some(NativeBossBar {
        name: Text::translate_with_fallback(&boss.lang_key(), &boss.name, &[]),
        progress: b::progress(mob.health, mob.attrs.max_health),
        color: boss.color,
        ..NativeBossBar::default()
    })
}

/// The seals within reach of a player standing at `at`: each raises its boss on top of it and
/// becomes its plain block, so the boss rises once.
fn raise_from_seals(game: &mut Game<'_>, all: &b::Bosses, dimension: &str, at: [f64; 3]) {
    // Each seal in force as (its state, the state it becomes, its trigger).
    let seals: Vec<_> = all
        .seals()
        .filter(|seal| on(game, &seal.mod_id, seal.setting.as_deref()))
        .filter_map(|seal| {
            let state = game.native_default_state(&seal.block)?;
            let becomes = game.native_default_state(seal.becomes.as_deref()?)?;
            Some((state, becomes, seal))
        })
        .collect();
    let Some(r) = seals.iter().map(|(_, _, seal)| seal.radius).max() else {
        return;
    };
    let feet = at.map(|c| c.floor() as i32);
    let mut found = Vec::new();
    for dx in -r..=r {
        for dy in -r..=r {
            for dz in -r..=r {
                let pos = [feet[0] + dx, feet[1] + dy, feet[2] + dz];
                let block = game.world.block(pos[0], pos[1], pos[2]);
                if let Some(seal) = seals
                    .iter()
                    .find(|(state, _, seal)| *state == block && b::in_reach([dx, dy, dz], seal.radius))
                {
                    found.push((pos, seal.1, &seal.2.boss));
                }
            }
        }
    }
    for (pos, becomes, boss) in found {
        let top = [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 1.0, f64::from(pos[2]) + 0.5];
        if raise(game, dimension, boss, top) {
            game.native_set_block(dimension, pos, becomes, horizon_world::level::flags::UPDATE_ALL);
        }
    }
}

/// Spawns boss `kind` at `at`, with a roar and smoke. False when the game refused it (peaceful,
/// an unloaded chunk), so the seal or the item is kept.
fn raise(game: &mut Game<'_>, dimension: &str, kind: &str, at: [f64; 3]) -> bool {
    let Some(id) = game.native_spawn_mob(dimension, kind, at, &NativeSpawn::default()) else {
        return false;
    };
    tidy(game, id);
    game.native_play_sound_in_place(RISE_SOUND, at, HOSTILE, 2.0, 1.0);
    puff(game, at);
    true
}

/// `native_use_item_on`: an altar's item used on its block (standing on its base) raises the
/// boss on top of it and is used up.
pub(super) fn on_use_item_on(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], claimed: &mut bool) {
    if *claimed || !game.native_authoritative() {
        return;
    }
    let all = b::bosses();
    let below = [pos[0], pos[1] - 1, pos[2]];
    let altar = all.altars(&stack.item).find(|t| {
        game.native_block(pos) == t.block && t.on.as_ref().is_none_or(|base| game.native_block(below) == *base)
    });
    let Some(altar) = altar else {
        return;
    };
    if !on(game, &altar.mod_id, altar.setting.as_deref()) {
        return;
    }
    *claimed = true;
    let dimension = game.dim.id().to_owned();
    let top = [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 1.0, f64::from(pos[2]) + 0.5];
    if raise(game, &dimension, &altar.boss, top) {
        game.consume_held();
    }
}

/// `native_loot_drops`: a dying boss drops its experience besides its loot.
pub(super) fn on_loot_drops(game: &mut Game<'_>, context: &NativeLootDrops<'_>) {
    let Some((_, kind)) = context.entity else {
        return;
    };
    let all = b::bosses();
    let Some(boss) = all.boss(kind) else {
        return;
    };
    let extra = b::extra_experience(boss.experience);
    if extra > 0 && game.native_authoritative() && on(game, &boss.mod_id, boss.setting.as_deref()) {
        // As a dying mob's own experience drops (`Drop::Orb`).
        game.spawn_orbs(context.origin, extra);
    }
}

/// `native_music`, once a client tick: the music of the nearest fight, while a boss with music
/// is alive within reach.
pub(super) fn on_music(game: &Game<'_>, music: &mut Option<Music>) {
    let all = b::bosses();
    if !all.has_music() {
        return;
    }
    let Some(me) = game.native_player() else {
        return;
    };
    let here = [me.x, me.y, me.z];
    let fight = game
        .native_client_entities()
        .into_iter()
        .filter(|e| e.alive && b::within(e.position, here, b::NEAR))
        .find_map(|e| {
            let boss = all.boss(&e.kind)?;
            on(game, &boss.mod_id, boss.setting.as_deref()).then(|| boss.music.clone()).flatten()
        });
    if let Some(sound) = fight {
        *music = Some(Music::new(&sound, 0, 0, true));
    }
}

/// `native_mob_submit`, before the mob's look is drawn: a creature that hovers is lifted off the
/// ground, bobbing.
pub(super) fn on_mob_submit(game: &Game<'_>, mob: &NativeMobRender<'_>, draw: &mut BlockEntityDraw) {
    if let Some((mod_id, height)) = b::bosses().hover(mob.kind)
        && game.native_on(mod_id)
    {
        draw.translate([0.0, b::hover_lift(height, mob.age), 0.0]);
    }
}
