// Hook helper: native_use_item (HEAD). A structure finder (the faded map scrap) says the way.
fn armory_lib_citadel_use_item(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    claimed: &mut bool,
) {
    crate::game::armory_lib_citadel::on_use_item(game, stack, claimed);
}
