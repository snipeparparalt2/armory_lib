// Hook helper: native_screen_key_pressed_pre (HEAD), the window's. The trinkets key in the
// player's own inventory is taken and queued for the next client tick.
fn armory_lib_trinket_screen_key(
    game: &crate::game::Game<'_>,
    screen: &horizon_client::mod_api::NativeScreenView<'_>,
    key: &str,
    repeat: bool,
    canceled: &mut bool,
) {
    crate::game::armory_lib_trinkets::on_screen_key(game, screen, key, repeat, canceled);
}
