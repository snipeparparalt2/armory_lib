// Hook helper: Mob::voice_pitch (RETURN). A type with sounds of its own speaks at its pitch.
fn armory_lib_voice_pitch(value: f32, mob: &crate::mobs::Mob) -> f32 {
    crate::armory_lib_goals::voice_pitch(mob, value)
}
