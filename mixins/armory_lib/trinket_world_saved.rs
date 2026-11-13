// Hook helper: native_world_saved (HEAD). Open trinkets menus are saved with the world.
fn armory_lib_trinket_world_saved(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_trinkets::on_world_saved(game);
}
