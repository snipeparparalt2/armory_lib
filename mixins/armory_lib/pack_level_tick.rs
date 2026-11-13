// Hook helper: native_level_tick_post (HEAD). The backpacks asked for open; their sounds play.
fn armory_lib_pack_level_tick(game: &mut crate::game::Game<'_>, dimension: &str) {
    crate::game::armory_lib_packs::on_level_tick_post(game, dimension);
}
