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
    if let Some(rule) = m::creature_rule(&mob.kind.id) {
        if m::mod_on(&rule.mod_id) { creature_ai::install(mob, &rule); }
    }
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

mod creature_ai {
//! Small, opt-in goal sets for content-authored creatures. Vanilla mobs are untouched.
use crate::mobs::{Mob, goals::{self, Cx, Goal}};
use crate::armory_lib_bridge::CreatureRule as MobRules;

pub fn install(mob: &mut Mob, rule: &MobRules) {
    let Some(behavior) = rule.behavior.as_deref() else { return; };
    let kind = std::sync::Arc::make_mut(&mut mob.kind);
    if let Some([w,h]) = rule.dimensions { kind.width=w;kind.height=h;kind.eye_height=h*0.8; }
    kind.flags.burns_in_day=false;
    kind.flags.fire_immune=rule.fire_proof;
    if behavior=="neutral" { kind.attack_damage=5.0; }
    mob.ai.goals.clear();mob.ai.targets.clear();
    mob.ai.targets.add(1,goals::HurtByTarget::new());
    if behavior!="neutral" { mob.ai.targets.add(2,goals::NearestTarget::new(goals::Prey::Player,true)); }
    if behavior=="flying" {
        kind.flags.flying=true;kind.flags.fall_immune=true;
        mob.no_gravity=true;
        let origin=[mob.position.x,mob.position.y+1.5,mob.position.z];
        mob.ai.goals.add(0,Flight { origin,cooldown:30 });
    } else {
        mob.ai.goals.add(0,goals::Float);
        mob.ai.goals.add(1,goals::MeleeAttack::new(1.0,true).interval(30));
        mob.ai.goals.add(5,goals::Stroll::new(0.8));
    }
}

#[derive(Clone,Debug)]
struct Flight { origin:[f64;3],cooldown:i32 }
impl Goal for Flight {
    fn flags(&self)->u8 { goals::MOVE|goals::LOOK }
    fn can_use(&mut self,_:&mut Mob,_:&mut Cx)->bool { true }
    fn every_tick(&self)->bool { true }
    fn box_clone(&self)->Box<dyn Goal> { Box::new(self.clone()) }
    fn tick(&mut self,m:&mut Mob,cx:&mut Cx) {
        m.no_gravity=true;m.nav_stop();
        let phase=m.age as f64*0.022+m.id as f64;
        let mut target=[self.origin[0]+phase.sin()*3.0,self.origin[1]+(phase*0.7).sin()*0.5,self.origin[2]+phase.cos()*3.0];
        self.cooldown=(self.cooldown-1).max(0);
        if let Some(who)=m.ai.target {
            if let Some(t)=cx.w.target_info(who).filter(|t| t.alive && !t.creative) {
                target=[t.position.x,t.position.y+0.5,t.position.z];
                m.look_at_speed([t.position.x,t.eye_y,t.position.z],20.0,20.0);
                if self.cooldown==0 && m.within_melee_range(cx.w,&t) && m.sees(cx.w,who) {
                    m.swing();crate::mobs::ai::do_hurt_target(m,who,cx);self.cooldown=35;
                }
            } else { m.ai.target=None; }
        }
        let d=[target[0]-m.position.x,target[1]-m.position.y,target[2]-m.position.z];
        let length=d.iter().map(|x|x*x).sum::<f64>().sqrt().max(1.0);
        let speed=if m.ai.target.is_some(){0.13}else{0.06};
        m.velocity.x=m.velocity.x*0.6+d[0]/length*speed;
        m.velocity.y=m.velocity.y*0.6+d[1]/length*speed;
        m.velocity.z=m.velocity.z*0.6+d[2]/length*speed;
        // Use ordinary entity collision; rise when a wall interrupts the flight path.
        if m.horizontal_collision { m.velocity.y=0.12; }
        if m.ai.target.is_none() {
            m.yaw=(-d[0].atan2(d[2]).to_degrees()) as f32;
            m.body_yaw=m.yaw;m.head_yaw=m.yaw;
        }
    }
}

}
