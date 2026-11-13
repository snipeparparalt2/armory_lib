// Hook helper: native_menu_input_closed (HEAD). A trinkets menu closed: the trinkets go into the
// player's file, the rest back to the player.
fn armory_lib_trinket_menu_closed(
    uuid: u128,
    window: i32,
    slots: &mut Vec<Option<horizon_client::items::ItemStack>>,
) {
    crate::game::armory_lib_trinkets::on_menu_closed(uuid, window, slots);
}
