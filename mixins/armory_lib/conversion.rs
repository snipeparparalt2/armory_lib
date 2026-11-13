// Hook helper: native_living_conversion_pre (HEAD). A content mod's creature keeps its type.
fn armory_lib_conversion(
    game: &mut crate::game::Game<'_>,
    entity: u32,
    kind: &str,
    outcome: &str,
    cancel: &mut bool,
) {
    crate::game::armory_lib::on_living_conversion_pre(game, entity, kind, outcome, cancel);
}
