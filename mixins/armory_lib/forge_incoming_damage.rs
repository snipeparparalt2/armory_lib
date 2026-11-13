// Hook helper: native_living_incoming_damage_v3 (HEAD). Warded and sharpened between players.
fn armory_lib_forge_incoming_damage(
    victim_uuid: Option<u128>,
    attacker_uuid: Option<u128>,
    damage_type: &str,
    amount: &mut f32,
    cancel: &mut bool,
) {
    crate::game::armory_lib_forge::on_incoming_damage(
        victim_uuid,
        attacker_uuid,
        damage_type,
        amount,
        cancel,
    );
}
