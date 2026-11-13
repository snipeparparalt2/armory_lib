// Hook helper: native_visuals (HEAD). The coloured light of glowing gear, once a world frame.
fn armory_lib_visuals(
    game: &crate::game::Game<'_>,
    rims: &mut Vec<horizon_client::mod_api::NativeBlockRim>,
    lights: &mut Vec<horizon_client::mod_api::NativeColoredLight>,
) {
    crate::game::armory_lib::on_visuals(game, rims, lights);
}
