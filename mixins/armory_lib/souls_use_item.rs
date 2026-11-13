// Hook helper: native_use_item (HEAD). A charge starts: the soul pool is shown.
fn armory_lib_souls_use_item(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    claimed: &mut bool,
) {
    crate::game::armory_lib_souls::on_use_item(game, stack, claimed);
}
