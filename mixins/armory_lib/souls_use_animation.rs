// Hook helper: native_use_animation (HEAD). A charged weapon is raised while it charges.
fn armory_lib_souls_use_animation(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    animation: &mut horizon_world::consumable::UseAnimation,
) {
    crate::game::armory_lib_souls::on_use_animation(game, stack, animation);
}
