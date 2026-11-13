// Hook helper: native_menu_closed (HEAD). A backpack's menu closed.
fn armory_lib_pack_menu_closed(uuid: u128, window: i32) {
    crate::game::armory_lib_packs::on_menu_closed(uuid, window);
}
