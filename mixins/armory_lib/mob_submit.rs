// Hook helper: native_mob_submit (HEAD). A content mod's creature drawn with its own look.
fn armory_lib_mob_submit(
    game: &crate::game::Game<'_>,
    mob: &crate::game::NativeMobRender<'_>,
    draw: &mut crate::game::BlockEntityDraw,
    drawn: &mut bool,
) {
    crate::game::armory_lib::on_mob_submit(game, mob, draw, drawn);
}
