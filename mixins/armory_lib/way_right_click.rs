// Hook helper: native_right_click_block (HEAD). A waystone: renaming it, binding a recall charm
// to it, its travel menu.
fn armory_lib_ways_right_click(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    canceled: &mut bool,
) {
    crate::game::armory_lib_ways::on_right_click_block(game, pos, canceled);
}
