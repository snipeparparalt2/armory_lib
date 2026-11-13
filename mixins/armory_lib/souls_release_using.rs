// Hook helper: native_release_using (HEAD). A charged weapon let go slams or thrusts.
fn armory_lib_souls_release_using(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    time_left: i32,
) {
    crate::game::armory_lib_souls::on_release_using(game, stack, time_left);
}
