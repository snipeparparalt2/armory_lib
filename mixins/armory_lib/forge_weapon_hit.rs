// Hook helper: native_living_incoming_damage_v2 (HEAD). A sharpened weapon's hit on a mob.
fn armory_lib_forge_weapon_hit(
    game: &mut crate::game::Game<'_>,
    weapon: Option<&horizon_client::items::ItemStack>,
    amount: &mut f32,
    canceled: &mut bool,
) {
    crate::game::armory_lib_forge::on_weapon_hit(game, weapon, amount, canceled);
}
