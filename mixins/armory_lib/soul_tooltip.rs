// Hook helper: native_tooltip (HEAD). A soul jar's souls under its name.
fn armory_lib_soul_tooltip(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    lines: &mut Vec<(horizon_client::text::Text, u32)>,
) {
    crate::game::armory_lib_packs::on_tooltip(game, stack, lines);
}
