<p align="center">
  <img src="docs/images/icon.png" alt="Armory Lib" width="128">
</p>

<h1 align="center">Armory Lib</h1>

<p align="center">
  <b>A native library mod for <a href="https://github.com/horizon-rust/horizon">Horizon</a>, the Rust rewrite of Minecraft: Java Edition.</b><br>
  Shared machinery for mods with their own creatures and gear. It does nothing on its own.
</p>

<p align="center">
  <img alt="Horizon" src="https://img.shields.io/badge/Horizon-fdc87ae%20(2026--11--08)-6b4fd8">
  <img alt="Game data" src="https://img.shields.io/badge/game%20data-26.3-3d8b5a">
  <img alt="Version" src="https://img.shields.io/badge/version-0.1.0%20in%20development-e0a81e">
  <img alt="Licence" src="https://img.shields.io/badge/licence-MPL--2.0-c0392b">
</p>

> **Status:** 0.1.0 is not released and has not been tested in a running game. It type-checks
> against Horizon `fdc87ae` built the way the launcher builds it, and its logic has unit tests.

## For players

Armory Lib adds the parts of other mods that need code. Install it when a mod you use offers
extras with it: [Useful Armory](https://github.com/snipeparparalt2/useful-armory-horizon) works
without it, and with it gets its coloured gear light, set bonuses, sneak abilities, weapon effects
and its creatures' looks.

1. Download `armory_lib-<version>.hnmod` from the [releases](../../releases), or install it from
   the mod browser.
2. Put it in `horizon-native-mods/` in your Horizon game folder (`%APPDATA%\horizon` on a default
   Windows install). The mods that use it stay where they are (`horizon-mods/`).
3. Press **Build** in the Horizon launcher and start the game.

A mod's switches are on its own row under **Options > Mods**.

Native mods are compiled into the game and are not sandboxed, so only install them from sources
you trust. The whole source is in [`mixins/armory_lib`](mixins/armory_lib).

## For mod authors

### What it does

| Piece | You declare | The library does |
|---|---|---|
| Settings page | A title and switches | Adds your mod's row to Options > Mods, draws the buttons, keeps the choices |
| Gear light | Items, a colour, a strength | Lights the world around whoever holds or wears one, and around it on the ground |
| Mob looks | An entity type, a body, a texture, an optional glowing texture | Draws the mob in place of its template's model, with the walk cycle, head turn, hurt flash and death fall |
| Mob rules | An entity type and its rules | Stops it converting (a zombie copy under water), stops daylight setting it alight, makes fire harmless to it, gives its melee hits effects or sets the player alight |
| Companions | A wandering-trader copy, what tames, summons and heals it | Gives it a tame wolf's goals: it follows its owner, fights what hurts them or what they hit, sits when told; tames it with an item, or summons it with one |
| Rare variants | A vanilla or mod type, one in how many, a look, a drop | Picks the rare ones from their UUIDs, draws them with their own look, drops their item now and then |
| Mob sounds | An entity type, three vanilla sound events, a pitch | Plays them, at that pitch, in place of its template's |
| Trinket slots | How many slots, a key | A row of trinket slots of each player's own, opened with the key (G) or `/trinkets`, kept per world and through death |
| Trinkets | Items, attribute modifiers, an experience bonus | Gives the modifiers while the item is in a trinket slot, and more experience from orbs |
| Backpacks | Items and a size in rows | Opens a chest menu of that size on a right click; the world keeps what each bag holds, so a dropped or copied bag never duplicates items |
| Soul jars | A jar, its full form, a capacity | Puts a soul in the player's jar for each mob they kill; a full jar becomes the full item. Shows a fill bar and a souls line |
| Waystones | Blocks, a cooldown, a level cost | A right click registers the waystone and opens a menu of the ones the player found; a click takes them there, across dimensions too. A name tag renames one |
| Recall items | Items, a level cost, a cooldown | A right click takes the player to the waystone it is bound to (sneak-click a waystone with it), else to their bed or respawn anchor, else the world spawn |
| Carried effects | Items, where they must be, an effect | Gives the effect while the item is there and takes it away after |
| Armour sets | Its pieces and what the whole set does | Effects, no fall damage, healing, less knockback or explosion damage, more experience |
| Tool abilities | Tools and an ability | While sneaking: vein mining, 3 x 3 digging or tree felling, as the player's own breaks |
| Harvest sweep | Hoes and a radius | Harvests and replants the grown crops around the one clicked |
| Weapon effects | Weapons and their effects | Backstab, splash damage, setting the target alight, healing on a kill, bonuses against mobs or in a dimension, head drops |

Everything you declare names your mod. While your mod is off in Options > Mods, or the switch a
rule names is off on your page, it does nothing.

There are two ways to declare it. **A content mod needs no code**: put a file in it. A native mod
can also call the library from Rust.

### From a content mod: `data/<mod id>/armory_lib.json`

The library reads this file from every content mod in `horizon-mods/`, when a world opens and on
`/reload`. Your mod stays one `.hmod` that players install from the mod browser; without Armory
Lib the file is simply not read. Every section is optional; ids are full ids.

```json
{
  "schema": 1,
  "page": {
    "title": "My Mod 1.0",
    "description": "Glowing swords and a cave lurker.",
    "switches": [{"key": "glow", "label": "Glow"}, {"key": "abilities", "label": "Abilities"}]
  },
  "gear_lights": [
    {"items": ["mymod:glow_sword", "mymod:glow_helmet"], "color": "#8cffe2", "level": 1.0, "setting": "glow"}
  ],
  "mob_looks": [
    {"kind": "mymod:lurker", "rig": "zombie", "texture": "mymod:entity/lurker",
     "emissive": "mymod:entity/lurker_e", "scale": 1.0}
  ],
  "mob_rules": [
    {"kind": "mymod:lurker", "no_conversion": true, "no_sunburn": true, "fire_proof": false,
     "on_hit": [{"effect": "minecraft:slowness", "amplifier": 0, "ticks": 100}],
     "ignite_seconds": 0}
  ],
  "companions": [
    {"kind": "mymod:hound", "tame_with": ["mymod:meat"], "tame_chance": 0.33,
     "heal_with": ["mymod:meat"], "heal": 6.0, "attack_damage": 5.0},
    {"kind": "mymod:golem", "summon_with": "mymod:golem_core", "attack_damage": 10.0}
  ],
  "mob_variants": [
    {"kind": "minecraft:chicken", "name": "gold_chicken", "one_in": 40,
     "drops": {"item": "minecraft:gold_nugget", "every_ticks": 6000},
     "look": {"rig": "chicken", "texture": "mymod:entity/gold_chicken"}}
  ],
  "mob_sounds": [
    {"kind": "mymod:hound", "ambient": "entity.wolf.growl", "hurt": "entity.wolf.hurt",
     "death": "entity.wolf.death", "pitch": 0.7}
  ],
  "trinket_slots": {"slots": 3, "key": "key.keyboard.g", "setting": "trinkets"},
  "trinkets": [
    {"items": ["mymod:ring"], "setting": "trinkets", "experience_bonus": 0.25,
     "modifiers": [{"attribute": "minecraft:max_health", "amount": 4.0, "operation": "add_value"}]}
  ],
  "backpacks": [{"items": ["mymod:small_pack"], "rows": 1, "setting": "backpacks"}],
  "soul_jars": [{"item": "mymod:soul_jar", "full": "mymod:soul_jar_full", "capacity": 50}],
  "waystones": [{"blocks": ["mymod:waystone"], "cooldown_seconds": 60, "cost_levels": 0}],
  "recall": [{"items": ["mymod:recall_charm"], "cost_levels": 1, "cooldown_seconds": 120}],
  "carried_effects": [
    {"items": ["mymod:swift_ring"], "place": "hotbar", "effect": "minecraft:speed", "amplifier": 0}
  ],
  "armor_sets": [
    {"pieces": ["mymod:helmet", "mymod:chestplate", "mymod:leggings", "mymod:boots"],
     "setting": "abilities",
     "effects": ["minecraft:night_vision", {"effect": "minecraft:speed", "amplifier": 1}],
     "no_fall_damage": false, "regen": {"every_ticks": 80, "health": 1.0},
     "knockback_taken": 0.5, "explosion_taken": 0.5, "experience_bonus": 0.25}
  ],
  "tools": [
    {"items": ["mymod:big_pickaxe"], "ability": "vein_mine", "setting": "abilities"}
  ],
  "ores": ["mymod:odd_ore_block"],
  "harvest": [{"items": ["mymod:scythe"], "radius": 2}],
  "weapons": [
    {"items": ["mymod:dagger"], "backstab": 2.0},
    {"items": ["mymod:greatsword"], "splash": {"radius": 1.5, "share": 0.5}},
    {"items": ["mymod:flame_sword"], "fire_ticks": 80, "heal_on_kill": 2.0, "heads": true,
     "bonus_against": {"kinds": ["minecraft:piglin"], "multiplier": 1.5},
     "bonus_in": {"dimension": "minecraft:the_nether", "multiplier": 1.5}}
  ]
}
```

| Field | Meaning |
|---|---|
| `schema` | 1. A file with a newer schema is left alone and logged |
| `setting` (any entry) | A switch key of your page that turns the entry off; leave it out for always on |
| `color` | `"#rrggbb"`, `"rrggbb"` or a number |
| `rig` | `zombie` (arms held out), `humanoid` (arms swing), `wolf` or `chicken` |
| `place` | `main_hand`, `off_hand`, `hotbar` (includes the off hand), `worn`, `all_worn` (every listed item worn) |
| `ability` | `vein_mine` (connected ores of one kind, up to 32), `wide_dig` (3 x 3 facing the player), `fell_tree` (connected logs, up to 64) |
| `ores` | Blocks that count as ores for vein mining though no ore tag lists them |
| `radius` | 1 for a 3 x 3 field, 2 for 5 x 5 (up to 4) |
| `knockback_taken`, `explosion_taken` | The share that still reaches the wearer, 0 to 1 |
| `heads` | Kills may drop the mob's head: zombie, skeleton and creeper 10%, wither skeleton 5% |
| `ignite_seconds` | The mob's own blow sets a player alight for that long |
| `tame_chance` | The chance one item tames a wild companion, 0 to 1 (0.33 if left out) |
| `summon_with` | An item that, used on a block, summons the companion on top of it, already the user's |
| `heal`, `attack_damage` | Health one healing item gives (4); the companion's melee damage (3) |
| `one_in`, `every_ticks` | A variant is one mob in `one_in` (50); it drops its item about every `every_ticks` (6000) |
| `look` (variant) | As a `mob_looks` entry without `kind`; `rig` defaults to the type's body |
| `ambient`, `hurt`, `death` | Vanilla sound events (`entity.wolf.growl`); a content mod can't add `minecraft:` ones |
| `pitch` | Times the mob's own voice pitch, 0.5 to 2 |
| `slots`, `key` | Trinket slots, 1 to 7 (3); every mod shares one row, the most any file declares. The key that opens them (`key.keyboard.g`); the first a file names is used |
| `modifiers` | `attribute` (`minecraft:max_health`), `amount`, `operation`: `add_value` (default), `add_multiplied_base` or `add_multiplied_total` |
| `experience_bonus` (trinket) | Extra experience from orbs, 0.25 for a quarter more (0 to 10); bonuses of different trinkets add up |
| `rows` | A backpack's size, 1 to 6 rows of 9 (3) |
| `item`, `full`, `capacity` | A soul jar while it fills, the item it becomes when full, and the souls that fill it (50) |
| `cooldown_seconds` | Waystones: seconds a player waits between two trips, whichever waystone (60). Recall: the item's cooldown after a use (120) |
| `cost_levels` | Experience levels a trip takes, none in creative (waystones 0, recall 1) |

Check your file before you ship it, in this repository:

```sh
ARMORY_LIB_FILE=path/to/armory_lib.json cargo test checks_a_mods_file
```

### From a native mod: Rust

Armory Lib is a plain mixin bundle, and its hooks are the public module
`horizon_client::game::armory_lib` in the game it builds. A native mod's mixin code can call its
`register_*` functions; the player then needs Armory Lib installed beside that mod. Register
once, from the first hook your mod gets (for example its `native_mods` helper). What you register
wins over a file entry for the same item or entity type.

```rust
// In a module your bundle adds to crates/horizon-client/src/game.rs.
use horizon_client::game::armory_lib::{
    self as lib, CarriedEffect, GearLight, MobLook, MobRules, Page, Place, Rig, Switch,
};

pub(crate) fn register() {
    lib::register_page(Page {
        mod_id: "mymod".into(),
        title: "My Mod 1.0".into(),
        description: "Glowing swords and a cave lurker.".into(),
        switches: vec![Switch::new("glow", "Glow")],
    });
    lib::register_gear_light(GearLight::new("mymod", "mymod:glow_sword", 0x8cffe2).setting("glow"));
    lib::register_mob_look(
        MobLook::new("mymod", "mymod:lurker", Rig::zombie(), "mymod:entity/lurker")
            .emissive("mymod:entity/lurker_e"),
    );
    lib::register_mob_rules(
        MobRules::new("mymod", "mymod:lurker")
            .no_conversion()
            .no_sunburn()
            .on_hit("minecraft:slowness", 0, 100),
    );
    lib::register_carried_effect(CarriedEffect::new(
        "mymod",
        &["mymod:swift_ring"],
        Place::Hotbar,
        "minecraft:speed",
    ));
}
```

A content mod gets all of this from its file, with no code, and that is the way to use the
library from a mod players install from the mod browser.

| Function | What it takes |
|---|---|
| `register_page(Page)` | `mod_id`, `title`, `description`, `switches: Vec<Switch>`. `Switch::new(key, label)`, `.off_by_default()` |
| `switch(host, mod_id, key) -> bool` | Reads a switch from your own hooks. At most a second old |
| `register_gear_light(GearLight)` | `GearLight::new(mod_id, item, rgb)`, `.level(0.0..=1.0)`, `.setting(key)` |
| `register_mob_look(MobLook)` | `MobLook::new(mod_id, kind, rig, texture)`, `.emissive(texture)`, `.scale(f32)` |
| `Rig` | `Rig::zombie()`, `Rig::humanoid()`, `Rig::wolf()`, `Rig::chicken()`, `Rig::custom(layer, parts)` with `Motion::Head`, `Motion::Swing { phase, reach }`, `Motion::Raised` |
| `register_mob_rules(MobRules)` | `MobRules::new(mod_id, kind)`, `.no_conversion()`, `.no_sunburn()`, `.fire_proof()`, `.on_hit(effect, amplifier, ticks)`, `.ignites(ticks)` |
| `register_carried_effect(CarriedEffect)` | `CarriedEffect::new(mod_id, items, place, effect)`, `.amplifier(u8)`, `.setting(key)` |

Armour sets, tool abilities, harvest sweeps, weapon effects, companions, rare variants and mob
sounds are declared in the file only.

### Details worth knowing

- **Textures** are sprite names: `mymod:entity/lurker` is `assets/mymod/textures/entity/lurker.png`
  in your content mod. A zombie or humanoid rig uses the 64x64 zombie layout, a wolf or chicken rig
  the 64x32 layout of that mob. The glowing texture holds only the glowing pixels.
- **A look needs an entity type of yours.** The library matches the type id the game reports, so
  it is for types your content mod adds (`horizon/entity_type/`), not for single vanilla mobs.
- **`no_sunburn` is for copies of undead mobs** (a zombie template): daylight no longer lights
  them. Other fire still does. **`fire_proof`** cancels burning damage from any source; the
  flames are still drawn when something lights the mob.
- **Hit effects reach players only**, one tick after the hit, and only for a mob's own blow.
  So does `ignite_seconds`.
- **A companion is a copy of the wandering trader** (`"template": "minecraft:wandering_trader"`):
  its behaviour is all goals, which the library replaces as each one is made. Its owner and
  whether it sits are kept as entity tags (`armory_lib.owner.<uuid>`, `armory_lib.sitting`), so
  they are saved with it. Wild ones act as wolves do (they hunt sheep, rabbits, foxes and
  skeletons, and fight back). The owner's click with anything but a healing item makes it sit
  or stand. Its click never opens a trader's menu.
- **Variants** are picked from each mob's UUID, the same on every side and every load. They need
  a texture in the layout of their rig.
- **Trinkets** live in a row of their own: the trinkets key (G) while playing or in the inventory,
  or `/trinkets`, opens it. Only items a file lists as trinkets stay in it, one to a slot; anything
  else goes back to the player when it closes. They are kept in
  `<world>/armory_lib/trinkets/<player uuid>.dat`, so they stay through death like an ender
  chest's items. Their modifiers are added as temporary ones (`armory_lib:trinket/<n>`); two
  copies of a trinket count once, and one held in the off hand gives only its item's own off-hand
  modifiers, though its experience bonus still counts.
- **A backpack holds an id, not items.** The first time it opens it gets `armory_lib:bag` in its
  custom data; what it holds is in `<world>/armory_lib/backpacks/<id>.dat`, saved when the menu
  closes and with each world save while it is open. Copies of one bag share what they hold, and
  one bag opens for one player at a time. Backpacks can't go inside backpacks. Give backpack and
  jar items `"max_stack_size": 1`.
- **Soul jars fill from a player's melee kills** of any mob: the first jar in the hotbar, left to
  right, then the off hand. The count is `armory_lib:souls` in the jar's custom data.
- **Waystones** are kept for each world in `<world>/armory_lib/waystones.json`. A new one is named
  after its biome. The menu lists only the waystones that player found, this dimension first and
  nearest first, up to 54, and opens a tick after the click. Travellers land on top of the block.
  The cooldown is per player and isn't saved. A waystone a player breaks leaves the list at once;
  one removed another way goes when someone tries to travel there. Sneaking with another item in
  hand leaves the block alone, so blocks can be placed against it.
- **Recall items** cool down as an ender pearl does. Sneak-click a waystone with one to bind it
  (`armory_lib:recall` in its custom data); when that waystone is gone it takes the player home.
- **Text** the library shows is English, with lang keys a mod can translate where it has them
  (`armory_lib.backpack.open_elsewhere`, `armory_lib.soul_jar.souls`).
- **Carried effects** are given for 16 seconds and renewed every 2, without particles, and taken
  away the tick the item leaves its place. The strongest level wins when two rules give the same
  effect.
- **Set effects** are given for 3 seconds and renewed every second (night vision for 13, so it
  doesn't flicker), and taken away when the set comes off. A set is whole when every listed piece
  is worn, in any slot.
- **Abilities and weapon effects** run on the server (single player's server thread, a LAN host,
  a dedicated server). Extra blocks are broken as the player's own breaks: drops for the tool,
  wear, statistics, protection. A tool stops one block before it would break.
- **Lights** are visual (Horizon's `visuals` hook): at most 8 a frame, the nearest ones, within 24
  blocks. They don't change the light level blocks and spawning see.
- **Your page replaces your content mod's own row** in Options > Mods, so players see one row for
  your mod, with its switches. Turning that row off turns your mod off, as before.

### Planned

Not in 0.1.0: block menus and screens (a soul forge), a button for the trinket slots in the
inventory screen, attribute modifiers as carried effects, a sitting pose for companions, and
bodies built in code rather than taken from vanilla.

## Compatibility

| | |
|---|---|
| Horizon | commit `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca` (Nightly 2026-11-08) |
| Game data | 26.3 |
| horizon-mixins | `e8cd0aa38cffef49cb2e088ef716baa107148c02` |

The library is a plain mixin bundle, with no `[api]` package, so the launcher builds the game as
it is (a trait mod would make it generate its own executable, which newer launchers write for a
newer Horizon). It adds `armory_lib_bridge` to the client library and its other modules to the
game, and hooks the head of the game's own catalog hook functions: `native_mods`,
`native_settings`, `native_settings_click`, `native_visuals`, `native_mob_submit`,
`native_living_conversion_pre`, `native_player_tick`, `native_server_data_loaded`,
`native_client_resources_reloaded` with the game as `Host`, and more for the gear, companions,
trinkets, backpacks, soul jars and waystones (item use, block clicks, kills, server menus,
commands, keys, world saves, the item bar and tooltip among them). Damage goes through
`native_living_incoming_damage` and a player's blow starts at `native_attack_entity`: the
`_v2` and `_v3` damage hooks and `native_living_knockback` run only when a trait mod is built
in, which a plain bundle never is, so the library doesn't use them. In the client library it adds `armory_lib_goals`, hooks the start of
`mobs::ai::burn_undead` for `no_sunburn` and of `mobs::ai::native_register_goals` for companions
and sounds, and the return of `Mob::voice_pitch`. It reads the game's state directly, so the
library is pinned to one Horizon commit.

## Build, check and package

```sh
cargo test                                   # the logic and the bridge, without the game
HORIZON_SRC=<horizon checkout> tools/check.sh   # type-check the game with the bundle applied, as the launcher builds it
python3 tools/package.py                     # dist/armory_lib-<version>.hnmod and SHA256SUMS.txt
cargo horizon-mod check dist/armory_lib-<version>.hnmod   # Horizon's own package check
```

`tools/check.sh` needs `horizon-mixins` on your PATH at the commit above. It leaves the checkout
alone and writes its staged copy under `target/`. `tools/gen_icon.py` redraws the icon. Releases are signed as described in
[docs/SIGNING.md](docs/SIGNING.md).

## Releases

Every update gets a GitHub release. Pushing a version tag makes it
(`.github/workflows/release.yml`): the workflow rebuilds the package, checks it against `dist/`
and attaches `armory_lib-<version>.hnmod`, its checksum and, once committed, its signature.

```sh
git tag v0.1.0-pre.1 && git push origin v0.1.0-pre.1   # a pre-release: a build still being tested
git tag v0.1.0 && git push origin v0.1.0               # the full release, after it has been played
```

Only full releases are meant for the Horizon mod browser. A published version never changes: fix
it with a new version number.

## Repository map

- `mixins/armory_lib/`: the bundle the launcher builds.
  - `hooks.rs`: the catalog hooks, the registry and reading the mods' files; `logic.rs`: their
    logic and tests.
  - `bridge.rs`: what all parts share: the gear rules, companions, variants and mob sounds,
    their logic and tests, switches, the daylight list. `gear.rs`: the gear rules, companions
    and variants in the game. `goals.rs`: companions' goals and mob sounds as each mob is made.
  - `trinkets.rs`, `packs.rs` (backpacks, soul jars), `ways.rs` (waystones, recall items): those
    features in the game; `*_logic.rs` beside each: what the files declare, the sums and tests.
    `store.rs`: the library's files in `<world>/armory_lib/` and item custom data.
  - `sunburn.rs` and the other `.rs` files: one small helper per hook.
- `tools/`: packaging, the type-check script and the icon generator
- `docs/`: the icon and the signing guide
- `dist/`: the latest built package and its checksum

## Licence

[Mozilla Public License 2.0](LICENSE). You may use the library from a mod under any licence. If
you change the library's own files and share the result, those files stay under MPL-2.0 with their
source available.
