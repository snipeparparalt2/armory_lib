// Hook helper: native_destroy_block (HEAD). A broken waystone leaves the registry.
fn armory_lib_ways_destroy_block(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    state: horizon_world::registry::BlockStateId,
    canceled: &mut bool,
) {
    crate::game::armory_lib_ways::on_destroy_block(game, pos, state, canceled);
}
