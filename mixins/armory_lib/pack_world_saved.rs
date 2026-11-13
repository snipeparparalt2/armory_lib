// Hook helper: native_world_saved (HEAD). Open backpacks are saved with the world.
fn armory_lib_pack_world_saved(game: &mut crate::game::Game<'_>) {
    crate::game::armory_lib_packs::on_world_saved(game);
}
