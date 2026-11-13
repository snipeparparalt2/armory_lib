// Hook helper: native_player_tick (HEAD), with the ticking player swapped in. The modifiers and
// the experience bonus of the trinkets in their slots.
fn armory_lib_trinket_tick(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_trinkets::on_player_tick(game);
}
