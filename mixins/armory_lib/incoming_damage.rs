// Hook helper: native_living_incoming_damage_v3 (HEAD). Fire-proof creatures, hits that give
// effects; set bonuses against falls and explosions.
fn armory_lib_incoming_damage(
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
    crate::game::armory_lib::on_living_incoming_damage_v3(
        victim,
        victim_uuid,
        kind,
        attacker,
        attacker_uuid,
        direct,
        damage_type,
        amount,
        cancel,
    );
    if !*cancel {
        crate::game::armory_lib_gear::on_incoming_damage(victim_uuid, damage_type, amount, cancel);
    }
}
