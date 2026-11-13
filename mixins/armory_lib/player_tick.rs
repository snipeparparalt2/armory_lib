// Hook helper: native_player_tick (HEAD). Set bonuses and weapon splashes, per player.
fn armory_lib_player_tick(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_gear::on_player_tick(game);
}
