// Hook helper: native_mob_submit (HEAD, before the look is drawn). A hovering creature is lifted.
fn armory_lib_boss_hover(
    game: &crate::game::Game<'_>,
    mob: &crate::game::NativeMobRender<'_>,
    draw: &mut crate::game::BlockEntityDraw,
) {
    crate::game::armory_lib_bosses::on_mob_submit(game, mob, draw);
}
