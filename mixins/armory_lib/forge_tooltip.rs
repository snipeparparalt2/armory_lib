// Hook helper: native_tooltip (HEAD). An item's upgrades under its name.
fn armory_lib_forge_tooltip(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    lines: &mut Vec<(horizon_client::text::Text, u32)>,
) {
    crate::game::armory_lib_forge::on_tooltip(game, stack, lines);
}
