// Hook helper: native_player_exp_change (HEAD). The trinkets' experience bonus.
fn armory_lib_trinket_experience(player: u128, amount: &mut i32) {
    crate::game::armory_lib_trinkets::on_experience(player, amount);
}
