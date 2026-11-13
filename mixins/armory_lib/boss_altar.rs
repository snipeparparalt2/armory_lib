// Hook helper: native_use_item_on (HEAD). An altar's item raises its boss.
fn armory_lib_boss_altar(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    pos: [i32; 3],
    claimed: &mut bool,
) {
    crate::game::armory_lib_bosses::on_use_item_on(game, stack, pos, claimed);
}
