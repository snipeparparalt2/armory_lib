// Hook helper: native_menu_clicked (HEAD). A click on a soul forge's preview forges.
fn armory_lib_forge_menu_clicked(
    game: &mut crate::game::Game<'_>,
    uuid: u128,
    window: i32,
    slot: i32,
    mode: i32,
) {
    crate::game::armory_lib_forge::on_menu_clicked(game, uuid, window, slot, mode);
}
