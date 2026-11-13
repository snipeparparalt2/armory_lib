// Hook helper: native_server_data_loaded (HEAD). The mods' armory_lib.json files are read again.
fn armory_lib_server_data_loaded(game: &mut crate::game::Game<'_>, reload: bool) {
    crate::game::armory_lib::on_server_data_loaded(game, reload);
}
