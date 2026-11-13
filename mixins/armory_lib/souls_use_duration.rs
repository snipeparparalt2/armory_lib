// Hook helper: native_use_duration (HEAD). A charged weapon is held until let go.
fn armory_lib_souls_use_duration(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    duration: &mut i32,
) {
    crate::game::armory_lib_souls::on_use_duration(game, stack, duration);
}
