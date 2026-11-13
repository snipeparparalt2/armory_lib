// Hook helper: native_living_knockback (HEAD). A set's knockback resistance.
fn armory_lib_knockback(entity: i32, kind: &str, strength: &mut f64) {
    crate::game::armory_lib_gear::on_knockback(entity, kind, strength);
}
