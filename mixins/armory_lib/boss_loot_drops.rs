// Hook helper: native_loot_drops (HEAD). A dying boss's experience.
fn armory_lib_boss_loot_drops(
    game: &mut crate::game::Game<'_>,
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
) {
    crate::game::armory_lib_bosses::on_loot_drops(game, context);
}
