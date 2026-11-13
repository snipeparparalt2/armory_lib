// Hook helper: native_mods (HEAD). A row in Options > Mods for each content mod's page.
fn armory_lib_mods(
    game: &mut crate::game::Game<'_>,
    mods: &mut Vec<horizon_client::mod_api::NativeMod>,
) {
    crate::game::armory_lib::on_mods(game, mods);
}
