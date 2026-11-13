// Hook helper: native_mob_killed (HEAD). The killer's soul pool takes the mob's souls.
fn armory_lib_souls_mob_killed(game: &mut crate::game::Game<'_>, player: u128, entity: u32) {
    crate::game::armory_lib_souls::on_mob_killed(game, player, entity);
}
