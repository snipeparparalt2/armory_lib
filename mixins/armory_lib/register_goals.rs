// Hook helper: native_register_goals (HEAD). Companions get a tame wolf's goals, types with
// sounds of their own get them.
fn armory_lib_register_goals(mob: &mut crate::mobs::Mob) {
    crate::armory_lib_goals::on_register_goals(mob);
}
