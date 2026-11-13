// Hook helper: native_menu_closed (HEAD). A travel menu closed.
fn armory_lib_ways_menu_closed(uuid: u128, window: i32) {
    crate::game::armory_lib_ways::on_menu_closed(uuid, window);
}
