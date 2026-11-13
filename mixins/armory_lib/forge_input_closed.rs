// Hook helper: native_menu_input_closed (HEAD). A soul forge closed; its items go back.
fn armory_lib_forge_input_closed(uuid: u128, window: i32) {
    crate::game::armory_lib_forge::on_menu_input_closed(uuid, window);
}
