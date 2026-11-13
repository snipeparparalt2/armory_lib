// Hook helper: native_settings_click (HEAD). A click flips a switch of a content mod's page.
fn armory_lib_settings_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    crate::game::armory_lib::on_settings_click(game, mod_id, button);
}
