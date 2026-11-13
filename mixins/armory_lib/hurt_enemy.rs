// Hook helper: native_hurt_enemy (HEAD). A weapon's fire, and its splash queued for the player tick.
fn armory_lib_hurt_enemy(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    entity: u32,
) {
    crate::game::armory_lib_gear::on_hurt_enemy(game, stack, entity);
}
