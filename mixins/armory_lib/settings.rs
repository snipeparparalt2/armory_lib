// Hook helper: native_settings (HEAD). A content mod's page: one button per switch.
fn armory_lib_settings(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    crate::game::armory_lib::on_settings(game, mod_id, buttons);
}
