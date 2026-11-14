// Hook helper: native_living_incoming_damage (HEAD), which runs for every hit with or without a
// trait mod (`_v2` and `_v3` don't). Fire-proof creatures and hits that give effects; weapon
// effects, sharpened and set cuts; warded.
fn armory_lib_incoming_damage(
    entity: i32,
    kind: &str,
    damage_type: &str,
    causing: i32,
    amount: &mut f32,
    canceled: &mut bool,
) {
    crate::game::armory_lib::on_living_incoming_damage(entity, kind, damage_type, causing, canceled);
    crate::game::armory_lib_gear::on_incoming_damage(entity, kind, damage_type, amount, canceled);
    crate::game::armory_lib_forge::on_incoming_damage(entity, kind, damage_type, amount, canceled);
}
