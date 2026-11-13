// Hook helper: native_perform_command (HEAD), with the player swapped in. /trinkets opens the
// player's trinket slots.
fn armory_lib_trinket_command(
    game: &mut crate::game::Game<'_>,
    player: &horizon_client::mod_api::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    crate::game::armory_lib_trinkets::on_perform_command(game, player, command, ran);
}
