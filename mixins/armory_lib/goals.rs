// Armory Lib's mob setup, added to the game's client library as the module `armory_lib_goals`.
// As each mob is made (vanilla's `Mob.registerGoals`, Horizon's `native_register_goals`, before
// its saved data is read), a content mod's companion type gets a tame wolf's goals and its
// melee damage in place of its template's, and a type with sounds of its own gets them; its
// voice is pitched as its mod says.
use crate::armory_lib_bridge as m;
use crate::mobs::{Mob, MobType};
use std::sync::{Arc, Mutex};

/// `native_register_goals`, for every new mob, on the server and on a client for its copies.
pub fn on_register_goals(mob: &mut Mob) {
    let gear = m::gear();
    if let Some(c) = gear.companion(&mob.kind.id)
        && m::mod_on(&c.mod_id)
    {
        // A copy of the wandering trader: all its behaviour is goals, all replaced. Tame or
        // not, sitting and the owner come from its tags (`gear.rs` keeps them in step).
        mob.ai.goals.clear();
        mob.ai.targets.clear();
        crate::mobs::tame::register(mob);
        mob.attrs.attack_damage = c.attack_damage;
    }
    if let Some(s) = gear.sounds_of(&mob.kind.id)
        && m::mod_on(&s.mod_id)
    {
        mob.kind = with_sounds(&mob.kind, s);
    }
}

/// The mob type `base` with the sounds of `sounds`: one copy per type, shared by its mobs,
/// made again only when the type or its sounds change.
fn with_sounds(base: &Arc<MobType>, sounds: &m::SoundsDef) -> Arc<MobType> {
    type Made = (Arc<MobType>, m::SoundsDef, Arc<MobType>);
    static MADE: Mutex<Vec<Made>> = Mutex::new(Vec::new());
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, _, copy)) = made
        .iter()
        .find(|(from, def, copy)| def == sounds && (Arc::ptr_eq(from, base) || Arc::ptr_eq(copy, base)))
    {
        return copy.clone();
    }
    let mut copy = MobType::clone(base);
    let [ambient, hurt, death] = &mut copy.sounds;
    for (slot, event) in [(ambient, &sounds.ambient), (hurt, &sounds.hurt), (death, &sounds.death)] {
        if let Some(event) = event {
            *slot = m::forever(event);
        }
    }
    let copy = Arc::new(copy);
    made.retain(|(from, _, _)| from.id != base.id);
    made.push((base.clone(), sounds.clone(), copy.clone()));
    copy
}

/// `Mob.voice_pitch` as it returns (vanilla's `LivingEntity.getVoicePitch`): a type with
/// sounds of its own speaks at its own pitch.
pub fn voice_pitch(mob: &Mob, pitch: f32) -> f32 {
    match m::gear().sounds_of(&mob.kind.id) {
        Some(s) if m::mod_on(&s.mod_id) => pitch * s.pitch,
        _ => pitch,
    }
}
