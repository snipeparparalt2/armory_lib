// Hook helper: native_player_tick (HEAD), with the ticking player swapped in. Carried-item
// effects and the effects of hits taken; set bonuses and weapon splashes.
fn armory_lib_player_tick(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib::on_player_tick(game);
    crate::game::armory_lib_gear::on_player_tick(game);
}
