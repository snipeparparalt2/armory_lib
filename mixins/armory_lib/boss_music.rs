// Hook helper: native_music (HEAD). The music of a boss fight nearby.
fn armory_lib_boss_music(
    game: &crate::game::Game<'_>,
    music: &mut Option<horizon_client::music::Music>,
) {
    crate::game::armory_lib_bosses::on_music(game, music);
}
