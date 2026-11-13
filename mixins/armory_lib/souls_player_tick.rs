// Hook helper: native_player_tick (HEAD), with the ticking player swapped in. The soul pool's
// trinket and set bonuses, souls spent and taken, the pool's line.
fn armory_lib_souls_player_tick(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_souls::on_player_tick(game);
}
