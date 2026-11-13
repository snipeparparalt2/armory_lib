// Hook helper: native_item_bar (HEAD). A soul jar's fill bar.
fn armory_lib_soul_item_bar(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    bar: &mut Option<(i32, u32)>,
) {
    crate::game::armory_lib_packs::on_item_bar(game, stack, bar);
}
