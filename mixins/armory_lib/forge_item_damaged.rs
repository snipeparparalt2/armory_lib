// Hook helper: native_item_damaged (HEAD). A reinforced item takes less wear.
fn armory_lib_forge_item_damaged(
    game: &mut crate::game::Game<'_>,
    item: &str,
    amount: &mut i32,
    cancel: &mut bool,
) {
    crate::game::armory_lib_forge::on_item_damaged(game, item, amount, cancel);
}
