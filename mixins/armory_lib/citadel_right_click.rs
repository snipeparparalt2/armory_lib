// Hook helper: native_right_click_block (HEAD). A sealed door opened with its key.
fn armory_lib_citadel_right_click(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    canceled: &mut bool,
) {
    crate::game::armory_lib_citadel::on_right_click_block(game, pos, canceled);
}
