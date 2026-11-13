// Hook helper: native_server_tick (HEAD). Soul forge menus open; their previews follow.
fn armory_lib_forge_server_tick(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_forge::on_server_tick(game);
}
