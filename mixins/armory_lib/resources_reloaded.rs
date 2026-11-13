// Hook helper: native_client_resources_reloaded (HEAD). The mods' files are read again.
fn armory_lib_resources_reloaded(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib::on_client_resources_reloaded(game);
}
