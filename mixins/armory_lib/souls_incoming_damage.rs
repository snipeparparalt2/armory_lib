// Hook helper: native_living_incoming_damage (HEAD), which the game calls for every hit, trait
// mods or not. Soul weapons' hits, sets' immunities, creatures' blows that steal souls.
fn armory_lib_souls_incoming_damage(
    entity: i32,
    kind: &str,
    damage_type: &str,
    causing: i32,
    amount: &mut f32,
    canceled: &mut bool,
) {
    crate::game::armory_lib_souls::on_incoming_damage(
        entity,
        kind,
        damage_type,
        causing,
        amount,
        canceled,
    );
}
