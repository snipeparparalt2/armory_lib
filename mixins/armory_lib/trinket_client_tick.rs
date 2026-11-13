// Hook helper: native_client_tick_pre (HEAD). Sends the /trinkets an inventory key press queued.
fn armory_lib_trinket_client_tick(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_trinkets::on_client_tick(game);
}
