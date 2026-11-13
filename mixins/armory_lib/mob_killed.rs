// Hook helper: native_mob_killed (HEAD). A weapon's heal on a kill and its head roll.
fn armory_lib_mob_killed(game: &mut crate::game::Game<'_>, player: u128, entity: u32, kind: &str) {
    crate::game::armory_lib_gear::on_mob_killed(game, player, entity, kind);
}
