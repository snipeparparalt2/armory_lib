// Hook helper: native_loot_drops (HEAD). A dying player keeps their soulbound items.
fn armory_lib_forge_loot_drops(
    game: &mut crate::game::Game<'_>,
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
    drops: &mut Vec<horizon_client::items::ItemStack>,
) {
    crate::game::armory_lib_forge::on_loot_drops(game, context, drops);
}
