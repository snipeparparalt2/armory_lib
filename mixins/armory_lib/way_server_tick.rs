// Hook helper: native_server_tick (HEAD). The travel menus right clicks asked for are opened.
fn armory_lib_ways_server_tick(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_ways::on_server_tick(game);
}
