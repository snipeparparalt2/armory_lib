// Hook helper: native_menu_input_closed (HEAD). A closing backpack's items go to its file.
fn armory_lib_pack_input_closed(
    uuid: u128,
    window: i32,
    slots: &mut Vec<Option<horizon_client::items::ItemStack>>,
) {
    crate::game::armory_lib_packs::on_menu_input_closed(uuid, window, slots);
}
