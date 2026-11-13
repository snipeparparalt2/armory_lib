// Hook helper: native_living_incoming_damage_v3 (HEAD). Set bonuses against falls and explosions.
fn armory_lib_incoming_damage(
    victim_uuid: Option<u128>,
    damage_type: &str,
    amount: &mut f32,
    cancel: &mut bool,
) {
    crate::game::armory_lib_gear::on_incoming_damage(victim_uuid, damage_type, amount, cancel);
}
