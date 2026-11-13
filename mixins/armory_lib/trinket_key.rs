// Hook helper: native_key (HEAD), the window's. The trinkets key while playing sends /trinkets.
fn armory_lib_trinket_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::armory_lib_trinkets::on_key(game, key, pressed);
}
