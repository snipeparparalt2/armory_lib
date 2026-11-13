// Hook helper: native_right_click_block (HEAD). A soul forge's menu.
fn armory_lib_forge_right_click(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    canceled: &mut bool,
) {
    crate::game::armory_lib_forge::on_right_click_block(game, pos, canceled);
}
