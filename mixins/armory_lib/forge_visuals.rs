// Hook helper: native_visuals (HEAD). The light of radiant items.
fn armory_lib_forge_visuals(
    game: &crate::game::Game<'_>,
    lights: &mut Vec<horizon_client::mod_api::NativeColoredLight>,
) {
    crate::game::armory_lib_forge::on_visuals(game, lights);
}
