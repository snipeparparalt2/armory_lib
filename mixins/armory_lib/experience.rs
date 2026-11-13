// Hook helper: native_player_exp_change (HEAD). A set's experience bonus.
fn armory_lib_experience(game: &crate::game::Game<'_>, player: u128, amount: &mut i32) {
    crate::game::armory_lib_gear::on_experience(game, player, amount);
}
