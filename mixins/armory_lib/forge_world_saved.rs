// Hook helper: native_world_saved (HEAD). What open soul forges hold is saved with the world.
fn armory_lib_forge_world_saved(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_forge::on_world_saved(game);
}
