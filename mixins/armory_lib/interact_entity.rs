// Hook helper: native_interact_entity (HEAD). Taming, healing and sitting companions.
fn armory_lib_interact_entity(
    game: &mut crate::game::Game<'_>,
    entity: i32,
    kind: &str,
    stack: Option<&horizon_client::items::ItemStack>,
    claimed: &mut bool,
) {
    crate::game::armory_lib_gear::on_interact_entity(game, entity, kind, stack, claimed);
}
