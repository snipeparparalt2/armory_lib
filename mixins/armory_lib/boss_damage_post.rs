// Hook helper: native_living_damage_post (HEAD). A mob's blow on a player, kept for that player's
// tick.
fn armory_lib_boss_damage_post(entity: i32, kind: &str, damage_type: &str, causing: i32, amount: f32) {
    crate::game::armory_lib_bosses::on_damage_post(entity, kind, damage_type, causing, amount);
}
