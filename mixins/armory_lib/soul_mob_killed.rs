// Hook helper: native_mob_killed (HEAD). The killer's soul jar takes the mob's soul.
fn armory_lib_soul_mob_killed(game: &mut crate::game::Game<'_>, player: u128, entity: u32) {
    crate::game::armory_lib_packs::on_mob_killed(game, player, entity);
}
