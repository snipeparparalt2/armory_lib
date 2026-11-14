// Hook helper: native_attack_entity (HEAD), where a player's blow starts. The attack and its
// weapon's effects, and a sharpened weapon's bonus, noted for the damage hook.
fn armory_lib_attack_entity(
    game: &mut crate::game::Game<'_>,
    entity: i32,
    pos: [f64; 3],
    canceled: &mut bool,
) {
    crate::game::armory_lib_gear::on_attack_entity(game, entity, pos, *canceled);
    crate::game::armory_lib_forge::on_attack_entity(game);
}
