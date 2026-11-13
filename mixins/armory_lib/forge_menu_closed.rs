// Hook helper: native_menu_closed (HEAD). A soul forge's menu closed.
fn armory_lib_forge_menu_closed(uuid: u128, window: i32) {
    crate::game::armory_lib_forge::on_menu_closed(uuid, window);
}
