// Hook helper: native_loot_drops (HEAD). A head a weapon took joins the mob's loot.
fn armory_lib_loot_drops(
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
    drops: &mut Vec<horizon_client::items::ItemStack>,
) {
    crate::game::armory_lib_gear::on_loot_drops(context, drops);
}
