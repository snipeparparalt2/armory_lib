// Gear and creatures that do things: what armour set bonuses, tool abilities, harvest sweeps,
// weapon effects, companions, rare variants and burning blows read and change in the game. Added to the game as `game::armory_lib_gear`. Which
// items do what comes from the mods' own files, kept in `armory_lib_bridge` (bridge.rs) with
// the decisions. Everything here runs on the server (single player's server thread, a LAN
// host, a dedicated server), inside the hooks named in each function.
use crate::game::Game;
use crate::game::native_mobs::NativeSpawn;
use crate::game::native_player_state::NativeEffect;
use horizon_client::armory_lib_bridge as m;
use horizon_client::items::ItemStack;
use horizon_client::mod_api::NativeLootDrops;
use horizon_client::particles::ParticleOptions;
use horizon_world::player_inventory::EquipmentSlot;
use horizon_world::registry::BlockStateId;

/// Whether a rule of mod `mod_id` is in force: the mod is on in Options > Mods, and so is the
/// switch of its page the rule names, if it names one.
fn on(game: &Game<'_>, mod_id: &str, setting: Option<&str>) -> bool {
    let mod_on = game.native_on(mod_id);
    m::note_mod_on(mod_id, mod_on);
    mod_on && m::switch_on(mod_id, setting, m::now_ms(), |id, key| game.native_setting(id, key))
}

// ---- Set bonuses -----------------------------------------------------------------------

/// `native_player_tick`, each server tick of each living player, with that player swapped in:
/// reads their armour, keeps their set's effects on them and heals the wearer of a set that
/// regenerates. Also deals the splash damage of their weapon hits since their last tick.
pub(super) fn on_player_tick(game: &mut Game<'_>) {
    let Some(me) = game.native_acting_player() else {
        return;
    };
    let gear = m::gear();
    let set = if gear.sets.is_empty() {
        None
    } else {
        let worn = [
            EquipmentSlot::Head,
            EquipmentSlot::Chest,
            EquipmentSlot::Legs,
            EquipmentSlot::Feet,
        ]
        .map(|slot| game.native_equipment(slot));
        gear.set_of(worn.each_ref().map(|s| s.as_ref().map(|s| &*s.item)))
            .filter(|set| on(game, &set.mod_id, set.setting.as_deref()))
    };
    let (before, ticks) = m::note_wearer(me.uuid, me.entity_id, set.clone());
    if let Some(old) = &before
        && before != set
    {
        take_set_effects(game, me.uuid, old);
    }
    if let Some(set) = &set {
        if before.as_ref() != Some(set) || ticks % m::EFFECT_REFRESH_TICKS == 0 {
            for (id, amplifier) in &set.effects {
                game.native_add_effect(me.uuid, &effect(id, *amplifier));
            }
        }
        if set.regen_every > 0 && ticks % set.regen_every == 0 {
            let max = game
                .native_attribute(me.uuid, "minecraft:max_health")
                .map_or(20.0, |(_, value)| value as f32);
            if me.health > 0.0 && me.health < max {
                let health = (me.health + set.regen_health).min(max);
                game.native_set_player_status(me.uuid, Some(health), None, None);
            }
        }
    }
    // Queued only while their weapon's rule was in force.
    for splash in m::take_splashes(me.uuid) {
        deal_splash(game, &me.dimension, me.entity_id, &splash);
    }
    // A burning blow they took (`hooks.rs` queued it this tick, its mob's rule in force).
    if let Some(ticks) = m::take_ignite(me.uuid)
        && let Some(body) = game.player.as_mut()
    {
        body.fire_ticks = body.fire_ticks.max(ticks);
    }
    sweep(game, &me.dimension);
}

fn effect(id: &str, amplifier: u8) -> NativeEffect {
    NativeEffect {
        id: id.to_owned(),
        amplifier,
        duration: m::effect_ticks(id),
        ambient: true,
        particles: false,
    }
}

/// The set came off: its effects go at once rather than running out. Only what looks like
/// the set's own (the same level, no longer than it gives) is taken; a potion's stays.
fn take_set_effects(game: &mut Game<'_>, uuid: u128, set: &m::SetDef) {
    let Some(now) = game.native_effects(uuid) else {
        return;
    };
    for (id, amplifier) in &set.effects {
        let ours = now.iter().any(|n| {
            n.id == *id && n.amplifier == *amplifier && (0..=m::effect_ticks(id)).contains(&n.duration)
        });
        if ours {
            game.native_remove_effect(uuid, id);
        }
    }
}

/// The set the player `uuid` wears, if its rule is in force as the last player tick saw it
/// (for the hooks with no game).
fn set_now(set: Option<std::sync::Arc<m::SetDef>>) -> Option<std::sync::Arc<m::SetDef>> {
    set.filter(|s| m::mod_on(&s.mod_id) && m::switch_on_now(&s.mod_id, s.setting.as_deref()))
}

/// `native_living_incoming_damage_v3` (no game: the hit is in progress), for a player victim
/// (`victim_uuid`): a set may take no fall damage, or a share of an explosion's.
pub(super) fn on_incoming_damage(
    victim_uuid: Option<u128>,
    damage_type: &str,
    amount: &mut f32,
    canceled: &mut bool,
) {
    let Some(uuid) = victim_uuid else {
        return;
    };
    if *canceled {
        return;
    }
    let Some(set) = set_now(m::with_wearer(uuid, |w| w.set.clone()).flatten()) else {
        return;
    };
    match m::incoming_damage(&set, damage_type) {
        m::DamageChange::Keep => {}
        m::DamageChange::Cancel => *canceled = true,
        m::DamageChange::Scale(share) => *amount *= share,
    }
}

/// `native_living_knockback` (no game): a set may be pushed less far.
pub(super) fn on_knockback(entity: i32, kind: &str, strength: &mut f64) {
    if m::vanilla(kind) != "player" {
        return;
    }
    if let Some(set) = set_now(m::set_of_entity(entity)) {
        *strength *= set.knockback_taken;
    }
}

/// `native_player_exp_change`: an experience orb the player picks up (kills, ores, and also
/// smelting, breeding and fishing, which drop orbs too) is worth more with a set that says so.
pub(super) fn on_experience(game: &Game<'_>, player: u128, amount: &mut i32) {
    if *amount <= 0 {
        return;
    }
    let Some(set) = m::with_wearer(player, |w| w.set.clone()).flatten() else {
        return;
    };
    if set.experience_bonus <= 0.0 || !on(game, &set.mod_id, set.setting.as_deref()) {
        return;
    }
    let bonus = m::with_wearer(player, |w| {
        m::experience_bonus(*amount, set.experience_bonus, &mut w.xp_carry)
    });
    *amount = amount.saturating_add(bonus.unwrap_or(0));
}

// ---- Tool abilities --------------------------------------------------------------------

/// `native_destroy_block`: a player is about to break the block `state` at `pos`, with their
/// state swapped in. Sneaking with a vein-mining, wide-dig or tree-felling tool breaks more
/// blocks first, each as that player's own break (drops for the tool, wear, statistics,
/// experience, protection), which calls this hook again: `m::Breaking` keeps those alone.
pub(super) fn on_destroy_block(
    game: &mut Game<'_>,
    pos: [i32; 3],
    state: BlockStateId,
    canceled: &mut bool,
) {
    if *canceled || m::breaking() || !game.native_authoritative() || !game.native_sneaking() {
        return;
    }
    let Some(tool) = game.native_equipment(EquipmentSlot::MainHand) else {
        return;
    };
    let gear = m::gear();
    let Some(rule) = gear.tool(&tool.item) else {
        return;
    };
    if !on(game, &rule.mod_id, rule.setting.as_deref()) {
        return;
    }
    let ability = rule.ability;
    let Some(name) = game.native_state_block(state).map(str::to_owned) else {
        return;
    };
    let targets = match ability {
        m::Ability::VeinMine => vein(game, pos, &name, &tool),
        m::Ability::WideDig => wide(game, pos, state, &tool),
        m::Ability::FellTree => tree(game, pos, &name),
    };
    if targets.is_empty() {
        return;
    }
    let Some(_guard) = m::Breaking::start() else {
        return;
    };
    // The original break's position marks which destroy event the breaker's own client
    // predicted; the extra blocks' events must reach them.
    let own_break = game.lan_break_pos.take();
    for (at, block) in targets {
        if !tool_left(game, &tool) {
            break;
        }
        if game.native_block(at) != block {
            continue;
        }
        let _ = game.with_player_actor(|g| g.break_block_as_player(at));
    }
    game.lan_break_pos = own_break;
}

/// Whether the player still holds the tool (it may have broken) with wear to spare: enough
/// for this extra block and the original one, which comes after.
fn tool_left(game: &Game<'_>, tool: &ItemStack) -> bool {
    let Some(now) = game.native_equipment(EquipmentSlot::MainHand) else {
        return false;
    };
    if now.item != tool.item {
        return false;
    }
    let (max, per_block) = game
        .items()
        .get(&now.item)
        .map_or((0, 1), |i| (i.max_damage, i.tool_damage_per_block.unwrap_or(1)));
    m::spare_durability(max, now.damage, 2 * per_block)
}

/// Whether a block counts as an ore: an ore tag, an ore a mod listed, or (with no tags to
/// read) a name ending in `_ore`.
fn is_ore(game: &Game<'_>, block: &str) -> bool {
    if m::gear().is_listed_ore(block) {
        return true;
    }
    let mut tags = m::ORE_TAGS
        .iter()
        .map(|tag| game.native_block_name_in_tag(block, tag));
    match tags.next() {
        Some(Err(_)) | None => m::looks_like_ore(block),
        Some(Ok(first)) => first || tags.any(|t| t == Ok(true)),
    }
}

fn is_log(game: &Game<'_>, block: &str) -> bool {
    match game.native_block_name_in_tag(block, "minecraft:logs") {
        Ok(tagged) => tagged,
        Err(_) => m::looks_like_log(block),
    }
}

/// Whether the tool gets the block's drops (`Player.hasCorrectToolForDrops`).
fn harvests(game: &Game<'_>, tool: &ItemStack, state: BlockStateId) -> bool {
    let facts = game.res.facts().get(state);
    !facts.requires_tool || game.items().correct_for_drops(Some(tool), &facts.name)
}

/// The ore blocks of the broken one's kind connected to it, with their names.
fn vein(game: &Game<'_>, pos: [i32; 3], block: &str, tool: &ItemStack) -> Vec<([i32; 3], String)> {
    if !is_ore(game, block) {
        return Vec::new();
    }
    let kind = m::vein_kind(block);
    m::flood(pos, m::VEIN_MAX, |at| {
        let state = game.world.block(at[0], at[1], at[2]);
        let name = &game.res.facts().get(state).name;
        m::vein_kind(name) == kind && harvests(game, tool, state)
    })
    .into_iter()
    .map(|at| (at, block_name(game, at)))
    .collect()
}

/// The logs of the broken one's kind connected to it, at its height and above.
fn tree(game: &Game<'_>, pos: [i32; 3], block: &str) -> Vec<([i32; 3], String)> {
    if !is_log(game, block) {
        return Vec::new();
    }
    m::flood(pos, m::TREE_MAX, |at| at[1] >= pos[1] && block_name(game, at) == block)
        .into_iter()
        .map(|at| (at, block.to_owned()))
        .collect()
}

/// The 3 x 3 square around the broken block, across the face the player looks at: the blocks
/// the tool is made for, no harder than the one broken.
fn wide(
    game: &Game<'_>,
    pos: [i32; 3],
    state: BlockStateId,
    tool: &ItemStack,
) -> Vec<([i32; 3], String)> {
    let hardness = game.res.facts().get(state).hardness;
    let axis = game
        .native_raycast(8.0)
        .filter(|hit| [hit.block_x, hit.block_y, hit.block_z] == pos)
        .and_then(|hit| m::hit_axis(pos, [hit.x, hit.y, hit.z]))
        .or_else(|| {
            game.native_acting_player()
                .map(|p| m::look_axis(p.yaw, p.pitch))
        })
        .unwrap_or(m::Axis::Y);
    m::square(pos, axis)
        .into_iter()
        .filter_map(|at| {
            let state = game.world.block(at[0], at[1], at[2]);
            let facts = game.res.facts().get(state);
            let fits = !game.res.behaviours().is_air(state)
                && (0.0..=hardness).contains(&facts.hardness)
                && game.items().correct_for_drops(Some(tool), &facts.name);
            fits.then(|| (at, facts.name.to_string()))
        })
        .collect()
}

fn block_name(game: &Game<'_>, at: [i32; 3]) -> String {
    game.native_block(at)
}

// ---- Companions and rare variants -------------------------------------------------------

/// The `SoundSource` ordinal of creatures that aren't hostile.
const NEUTRAL: usize = 6;

/// Once a second in each dimension, from the first player tick there: companions get their
/// state from their tags (or keep their tags in step with it) and their melee damage; rare
/// variants drop their item now and then.
fn sweep(game: &mut Game<'_>, dimension: &str) {
    let gear = m::gear();
    if (gear.companions.is_empty() && gear.variants.is_empty()) || !game.native_authoritative() {
        return;
    }
    let Some(clock) = game.native_clock() else {
        return;
    };
    let sweep = clock.game_time.div_euclid(20);
    if !m::start_sweep(dimension, sweep) {
        return;
    }
    let found: Vec<(u32, String, [f64; 3])> = game
        .mobs
        .iter()
        .filter(|mob| !mob.removed && mob.alive())
        .filter(|mob| gear.companion(&mob.kind.id).is_some() || gear.has_variants(&mob.kind.id))
        .map(|mob| (mob.id, mob.kind.id.clone(), [mob.position.x, mob.position.y, mob.position.z]))
        .collect();
    for (id, kind, at) in found {
        if let Some(c) = gear.companion(&kind)
            && on(game, &c.mod_id, c.setting.as_deref())
        {
            keep_companion(game, dimension, id, c);
        }
        if !gear.has_variants(&kind) {
            continue;
        }
        let uuid = m::uuid_from_words(game.uuid_for(id));
        let Some(v) = gear.variant_of(&kind, uuid) else {
            continue;
        };
        if let Some(item) = &v.drop
            && m::drops_now(uuid, &v.name, v.every_ticks, sweep)
            && on(game, &v.mod_id, v.setting.as_deref())
        {
            let block = at.map(|c| c.floor() as i32);
            game.pop_resource(block, ItemStack::new(item, 1));
            game.native_play_sound_in_place("minecraft:entity.chicken.egg", at, NEUTRAL, 1.0, 1.0);
        }
    }
}

/// A companion's tame state, owner and sitting from its tags (saved with it) the first time
/// it is seen, its sitting into its tags after that, and its melee damage.
fn keep_companion(game: &mut Game<'_>, dimension: &str, id: u32, c: &m::CompanionDef) {
    let Some(handle) = game.native_entity(id) else {
        return;
    };
    let tags = game.native_entity_tags(handle).unwrap_or_default();
    let owner = m::owner_from_tags(&tags);
    let tagged_sitting = tags.iter().any(|t| t == m::SITTING_TAG);
    let first = m::first_sight(dimension, id);
    let Some(mob) = game.mobs.iter_mut().find(|mob| mob.id == id) else {
        return;
    };
    mob.extra.tame = owner.is_some();
    mob.extra.owner = owner.map(m::uuid_words);
    if first || owner.is_none() {
        mob.extra.sitting = owner.is_some() && tagged_sitting;
    }
    if mob.attrs.attack_damage < c.attack_damage {
        mob.attrs.attack_damage = c.attack_damage;
    }
    let sitting = mob.extra.sitting;
    if sitting != tagged_sitting {
        if sitting {
            game.native_entity_add_tag(handle, m::SITTING_TAG);
        } else {
            game.native_entity_remove_tag(handle, m::SITTING_TAG);
        }
    }
}

/// `native_interact_entity`: a right click on a companion, with the clicking player swapped
/// in. Its item tames a wild one (one used each try, at the companion's chance); its owner's
/// healing item heals it, anything else tells it to sit or stand. A companion's click is
/// always taken, so a copy of the wandering trader never opens a trader's menu.
pub(super) fn on_interact_entity(
    game: &mut Game<'_>,
    entity: i32,
    kind: &str,
    stack: Option<&ItemStack>,
    claimed: &mut bool,
) {
    if *claimed || !game.native_authoritative() {
        return;
    }
    let gear = m::gear();
    let Some(c) = gear.companion(kind) else {
        return;
    };
    let (Ok(id), Some(me)) = (u32::try_from(entity), game.native_acting_player()) else {
        return;
    };
    if !on(game, &c.mod_id, c.setting.as_deref()) {
        return;
    }
    let Some(handle) = game.native_entity(id) else {
        return;
    };
    *claimed = true;
    let owner = m::owner_from_tags(&game.native_entity_tags(handle).unwrap_or_default());
    let item = stack.map(|s| s.item.to_string()).unwrap_or_default();
    let Some(at) = game
        .mobs
        .iter()
        .find(|mob| mob.id == id)
        .map(|mob| [mob.position.x, mob.position.y + 0.5, mob.position.z])
    else {
        return;
    };
    match owner {
        None if c.tame_with.contains(&item) => {
            game.consume_held();
            let dimension = game.dim.id().to_owned();
            let roll = game.native_level_random(&dimension).map_or(1.0, |r| {
                use horizon_core::random::RandomSource;
                r.next_float()
            });
            if roll < c.tame_chance {
                // `TamableAnimal.tame` and a wolf's `setOrderedToSit(true)`.
                game.native_entity_add_tag(handle, &m::owner_tag(me.uuid));
                game.native_entity_add_tag(handle, m::SITTING_TAG);
                if let Some(mob) = game.mobs.iter_mut().find(|mob| mob.id == id) {
                    mob.extra.tame = true;
                    mob.extra.owner = Some(m::uuid_words(me.uuid));
                    mob.extra.sitting = true;
                    mob.ai.nav.stop();
                    mob.ai.target = None;
                }
                puff(game, "minecraft:heart", at);
            } else {
                puff(game, "minecraft:smoke", at);
            }
        }
        Some(o) if o == me.uuid => {
            let hurt = game
                .mobs
                .iter()
                .find(|mob| mob.id == id)
                .is_some_and(|mob| mob.health < mob.attrs.max_health);
            if hurt && c.heal_with.contains(&item) {
                game.consume_held();
                if let Some(mob) = game.mobs.iter_mut().find(|mob| mob.id == id) {
                    mob.health = (mob.health + c.heal).min(mob.attrs.max_health);
                }
                puff(game, "minecraft:heart", at);
            } else if let Some(mob) = game.mobs.iter_mut().find(|mob| mob.id == id) {
                // `TamableAnimal.setOrderedToSit`, as a wolf's owner's click does.
                mob.extra.sitting = !mob.extra.sitting;
                mob.ai.nav.stop();
                mob.ai.target = None;
                let sitting = mob.extra.sitting;
                if sitting {
                    game.native_entity_add_tag(handle, m::SITTING_TAG);
                } else {
                    game.native_entity_remove_tag(handle, m::SITTING_TAG);
                }
            }
        }
        _ => {}
    }
}

/// Seven particles of `name` around `at`, as a taming try shows them.
fn puff(game: &mut Game<'_>, name: &str, at: [f64; 3]) {
    let options = ParticleOptions::named(name);
    game.native_send_particles_in_place(&options, None, at, [0.4, 0.4, 0.4], 0.02, 7);
}

/// Its summoning item used on a block: a companion appears on top of it, already its
/// user's, and the item is used up.
fn summon(game: &mut Game<'_>, c: &m::CompanionDef, pos: [i32; 3], claimed: &mut bool) {
    let Some(me) = game.native_acting_player() else {
        return;
    };
    let at = [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 1.0, f64::from(pos[2]) + 0.5];
    let dimension = game.dim.id().to_owned();
    let Some(id) = game.native_spawn_mob(&dimension, &c.kind, at, &NativeSpawn::default()) else {
        return;
    };
    *claimed = true;
    game.consume_held();
    if let Some(handle) = game.native_entity(id) {
        game.native_entity_add_tag(handle, &m::owner_tag(me.uuid));
    }
    if let Some(mob) = game.mobs.iter_mut().find(|mob| mob.id == id) {
        mob.extra.tame = true;
        mob.extra.owner = Some(m::uuid_words(me.uuid));
        mob.attrs.attack_damage = mob.attrs.attack_damage.max(c.attack_damage);
        mob.persistent = true;
    }
    m::first_sight(&dimension, id);
    game.native_play_sound_in_place("minecraft:entity.iron_golem.repair", at, NEUTRAL, 1.0, 0.8);
    puff(game, "minecraft:cloud", [at[0], at[1] + 0.5, at[2]]);
}

// ---- Harvest sweep ---------------------------------------------------------------------

/// `native_use_item_on`: a right click with `stack` on the block at `pos`. A companion's
/// summoning item summons it. A hoe with a harvest rule on a grown crop harvests and replants
/// the grown crops around it, one durability each, as long as the hoe has more than one
/// point left.
pub(super) fn on_use_item_on(
    game: &mut Game<'_>,
    stack: &ItemStack,
    pos: [i32; 3],
    claimed: &mut bool,
) {
    if *claimed || !game.native_authoritative() {
        return;
    }
    let gear = m::gear();
    if let Some(c) = gear.summoned_by(&stack.item) {
        if on(game, &c.mod_id, c.setting.as_deref()) {
            summon(game, c, pos, claimed);
        }
        return;
    }
    let Some(rule) = gear.harvest(&stack.item) else {
        return;
    };
    if !on(game, &rule.mod_id, rule.setting.as_deref()) || grown_crop(game, pos).is_none() {
        return;
    }
    let radius = rule.radius;
    *claimed = true;
    let dimension = game.dim.id().to_owned();
    for at in m::field(pos, radius) {
        let Some(crop) = grown_crop(game, at) else {
            continue;
        };
        let Some(hoe) = game.held().cloned() else {
            break;
        };
        let max = game.items().get(&hoe.item).map_or(0, |i| i.max_damage);
        if hoe.item != stack.item || !m::spare_durability(max, hoe.damage, 1) {
            break;
        }
        let Some(mut drops) = game.native_machine_destroy_block(at, Some(&hoe), true) else {
            continue;
        };
        let replant = m::take_one(
            drops.iter_mut().map(|d| (&*d.item, &mut d.count)),
            crop.seed,
        );
        for stack in drops.into_iter().filter(|d| d.count > 0) {
            game.pop_resource(at, stack);
        }
        if replant && let Some(seedling) = game.native_default_state(crop.block) {
            game.native_set_block(&dimension, at, seedling, horizon_world::level::flags::UPDATE_ALL);
        }
        game.damage_held(1);
    }
}

fn grown_crop(game: &Game<'_>, at: [i32; 3]) -> Option<&'static m::Crop> {
    let crop = m::crop(&game.native_block(at))?;
    m::grown(crop, game.native_block_property(at, "age").as_deref()).then_some(crop)
}

// ---- Weapon effects --------------------------------------------------------------------

/// `native_living_incoming_damage_v2`: a player's weapon is about to hit the mob `entity`
/// (melee, sweep or spear), with that player swapped in. A weapon's rule may make it hit
/// harder from behind, against some mobs or in some dimension; a splashing weapon's hit is
/// kept for its splash.
#[allow(clippy::too_many_arguments)]
pub(super) fn on_weapon_hit(
    game: &mut Game<'_>,
    entity: i32,
    kind: &str,
    source_position: Option<[f64; 3]>,
    victim_position: [f64; 3],
    victim_view: [f64; 3],
    weapon: Option<&ItemStack>,
    amount: &mut f32,
    canceled: &mut bool,
) {
    if *canceled {
        return;
    }
    let Some(w) = weapon.and_then(|s| m::gear().weapon(&s.item)) else {
        return;
    };
    if !on(game, &w.mod_id, w.setting.as_deref()) {
        return;
    }
    let attacker = source_position.or_else(|| game.native_acting_player().map(|p| p.position));
    let from_behind = attacker.is_some_and(|a| m::behind(victim_position, victim_view, a));
    *amount *= m::hit_multiplier(&w, kind, game.dim.id(), from_behind);
    if w.splash_radius > 0.0
        && let Ok(victim) = u32::try_from(entity)
    {
        m::note_hit(victim, *amount, victim_position);
    }
}

/// `native_hurt_enemy`: the player's hit with `stack` landed on the mob `entity`. A burning
/// weapon sets it alight; a splash is queued for the player's tick.
pub(super) fn on_hurt_enemy(game: &mut Game<'_>, stack: &ItemStack, entity: u32) {
    let Some(w) = m::gear().weapon(&stack.item) else {
        return;
    };
    let hit = (w.splash_radius > 0.0).then(|| m::take_hit(entity)).flatten();
    if !on(game, &w.mod_id, w.setting.as_deref()) {
        return;
    }
    if w.fire_ticks > 0
        && let Some(mob) = game.mobs.iter_mut().find(|mob| mob.id == entity)
    {
        mob.fire_ticks = mob.fire_ticks.max(w.fire_ticks);
    }
    if let (Some((amount, at)), Some(me)) = (hit, game.native_acting_player()) {
        m::queue_splash(m::Splash {
            attacker: me.uuid,
            victim: entity,
            at,
            amount: amount * w.splash_share,
            radius: w.splash_radius,
        });
    }
}

/// The splash of a weapon's hit: the other mobs around its target take their share, from
/// the attacking player.
fn deal_splash(game: &mut Game<'_>, dimension: &str, attacker: i32, splash: &m::Splash) {
    let caught: Vec<u32> = game
        .native_mobs(dimension)
        .into_iter()
        .filter(|mob| mob.id != splash.victim && m::in_splash(splash.at, splash.radius, mob.pos))
        .map(|mob| mob.id)
        .collect();
    let by = u32::try_from(attacker).ok();
    for id in caught {
        game.native_hurt_entity(dimension, id, splash.amount, "minecraft:player_attack", by);
    }
}

/// `native_mob_killed`: the player's blow killed the mob `entity` of type `kind`, with the
/// player swapped in. A weapon's rule may heal its wielder, or take the mob's head.
pub(super) fn on_mob_killed(game: &mut Game<'_>, player: u128, entity: u32, kind: &str) {
    let Some(w) = game
        .native_equipment(EquipmentSlot::MainHand)
        .and_then(|s| m::gear().weapon(&s.item))
    else {
        return;
    };
    if !on(game, &w.mod_id, w.setting.as_deref()) {
        return;
    }
    if w.heal_on_kill > 0.0
        && let Some(me) = game.native_acting_player()
        && me.uuid == player
        && me.health > 0.0
    {
        let health = me.health + w.heal_on_kill;
        game.native_set_player_status(player, Some(health), None, None);
    }
    if w.heads
        && let Some((head, chance)) = m::head_of(kind)
    {
        let dimension = game.dim.id().to_owned();
        let roll = game.native_level_random(&dimension).map(|r| {
            use horizon_core::random::RandomSource;
            r.next_float()
        });
        if roll.is_some_and(|r| r < chance) {
            m::note_head(entity, head);
        }
    }
}

/// `native_loot_drops`: a mob killed for its head drops it with its loot (once, and not a
/// second one where its own loot rolled it).
pub(super) fn on_loot_drops(
    context: &NativeLootDrops<'_>,
    drops: &mut Vec<ItemStack>,
) {
    let Some((entity, _)) = context.entity else {
        return;
    };
    let Some(head) = m::take_head(entity) else {
        return;
    };
    if !drops.iter().any(|d| &*d.item == head) {
        drops.push(ItemStack::new(head, 1));
    }
}
