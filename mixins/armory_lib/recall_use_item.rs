// Hook helper: native_use_item (HEAD). The recall charm.
fn armory_lib_recall_use_item(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    claimed: &mut bool,
) {
    crate::game::armory_lib_ways::on_use_item(game, stack, claimed);
}
