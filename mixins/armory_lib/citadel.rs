// Sealed doors and structure finders in the game, as `game::armory_lib_citadel`. What they are
// comes from the mods' files (`sealed_doors` and `structure_finders` in `armory_lib.json`), read
// by `armory_lib_citadel_logic` (citadel_logic.rs), which also holds the sums. Everything here
// runs on the server (single player's server thread, a LAN host, a dedicated server), in the
// hooks named in each function, with the acting player's state swapped in.
use super::armory_lib_citadel_logic as c;
use crate::game::Game;
use horizon_client::items::ItemStack;
use horizon_client::particles::{ParticleOptions, ParticleType};
use horizon_client::text::Text;

/// The `SoundSource` ordinals of blocks and players.
const BLOCKS: usize = 4;
const PLAYERS: usize = 7;
/// A respawn anchor going dark: the seal breaking, under the gate's own sound.
const SEAL_SOUND: &str = "minecraft:block.respawn_anchor.deplete";
/// Turning a page: the scrap being read.
const READ_SOUND: &str = "minecraft:item.book.page_turn";

fn on(game: &Game<'_>, mod_id: &str, setting: Option<&str>) -> bool {
    super::armory_lib_gear::on(game, mod_id, setting)
}

/// `hooks.rs load()`, for each content mod's file: its `sealed_doors` and `structure_finders`,
/// in place of what it declared before. What it could not use, one line each.
pub(super) fn read(mod_id: &str, file: &serde_json::Value) -> Vec<String> {
    c::read_and_keep(mod_id, file)
}

/// `native_right_click_block`: a player right clicks the block at `pos`. On a sealed door with
/// its key in the acting hand, the key is used up (not in creative) and every door block of the
/// gate joined to this one turns to air, with the gate's sound, the seal's and soul flames; the
/// click is taken. With anything else the door says it is sealed and the click goes on (so a
/// block can still be placed against it). The blocks are unbreakable, so this is the way in.
pub(super) fn on_right_click_block(game: &mut Game<'_>, pos: [i32; 3], canceled: &mut bool) {
    if *canceled || !game.native_authoritative() {
        return;
    }
    let all = c::citadel();
    let block = game.native_block(pos);
    let Some(door) = all.door(&block) else {
        return;
    };
    if !on(game, &door.mod_id, door.setting.as_deref()) {
        return;
    }
    let Some(me) = game.native_acting_player() else {
        return;
    };
    let keyed = game.held().is_some_and(|s| *s.item == *door.key);
    if !keyed {
        game.native_action_bar(me.uuid, Text::literal(door.locked.clone()));
        return;
    }
    *canceled = true;
    let Some(air) = game.native_default_state("minecraft:air") else {
        return;
    };
    let gate = c::gate(pos, door.max_blocks, |p| door.blocks.iter().any(|b| *b == game.native_block(p)));
    let dimension = game.dim.id().to_owned();
    game.consume_held();
    for p in &gate {
        game.native_set_block(&dimension, *p, air, horizon_world::level::flags::UPDATE_ALL);
    }
    let at = c::middle(&gate);
    game.native_play_sound_in_place(&door.sound, at, BLOCKS, 1.5, 0.8);
    game.native_play_sound_in_place(SEAL_SOUND, at, BLOCKS, 1.0, 0.6);
    let flame = ParticleOptions::simple(ParticleType::SoulFireFlame);
    let smoke = ParticleOptions::simple(ParticleType::LargeSmoke);
    for p in gate.iter().step_by(2) {
        let middle = [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5];
        game.native_send_particles_in_place(&flame, None, middle, [0.3, 0.3, 0.3], 0.02, 6);
        game.native_send_particles_in_place(&smoke, None, middle, [0.3, 0.3, 0.3], 0.01, 2);
    }
    game.native_action_bar(me.uuid, Text::literal("The seal breaks and the gate opens"));
}

/// `native_use_item`: a player uses `stack` (a right click). A finder item tells them the
/// direction and rough distance to the nearest start of its structure in the level they are
/// in, the way `/locate structure` finds it, and is kept; then it cools down a few seconds, as
/// the search builds the layouts of the starts it passes. Nothing within its reach (or another
/// dimension): it says so.
pub(super) fn on_use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if *claimed || !game.native_authoritative() {
        return;
    }
    let all = c::citadel();
    let Some(finder) = all.finder(&stack.item) else {
        return;
    };
    if !on(game, &finder.mod_id, finder.setting.as_deref()) {
        return;
    }
    let Some(me) = game.native_acting_player() else {
        return;
    };
    *claimed = true;
    let from = me.position.map(|v| v.floor() as i32);
    let names = [finder.structure.clone()];
    let found = game
        .end
        .locator()
        .and_then(|terrain| terrain.locate(&names, from, finder.radius as i32));
    let text = match found {
        Some((_, at)) => c::says(&finder.name, me.position, at),
        None => finder.not_found.clone(),
    };
    game.native_play_sound_in_place(READ_SOUND, me.position, PLAYERS, 1.0, 0.8);
    game.native_tell(me.uuid, Text::literal(text));
    if finder.cooldown_seconds > 0 {
        let group = horizon_client::cooldowns::group(stack, None);
        game.add_cooldown(group, finder.cooldown_seconds as i32 * 20);
    }
}
