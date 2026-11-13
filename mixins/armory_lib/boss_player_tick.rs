// Hook helper: native_player_tick (HEAD), with the ticking player swapped in. The blows they took
// from bosses, the bars and phases of the bosses near them, the seals near them.
fn armory_lib_boss_player_tick(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_bosses::on_player_tick(game);
}
