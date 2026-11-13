// Hook helper: native_use_item (HEAD). A right click with a backpack asks for its menu.
fn armory_lib_pack_use_item(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    claimed: &mut bool,
) {
    crate::game::armory_lib_packs::on_use_item(game, stack, claimed);
}
