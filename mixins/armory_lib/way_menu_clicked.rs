// Hook helper: native_menu_clicked (HEAD). A click on a waystone of a travel menu.
fn armory_lib_ways_menu_clicked(
    game: &mut crate::game::Game<'_>,
    uuid: u128,
    window: i32,
    slot: i32,
    mode: i32,
) {
    crate::game::armory_lib_ways::on_menu_clicked(game, uuid, window, slot, mode);
}
