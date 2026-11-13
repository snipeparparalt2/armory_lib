// Hook helper: native_on_use_tick (HEAD). A charge is full.
fn armory_lib_souls_use_tick(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    remaining: i32,
) {
    crate::game::armory_lib_souls::on_use_tick(game, stack, remaining);
}
