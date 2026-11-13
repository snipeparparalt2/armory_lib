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
| Mob rules | An entity type and its rules | Stops it converting (a zombie copy under water), stops daylight setting it alight, makes fire harmless to it, gives its melee hits effects |
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
     "on_hit": [{"effect": "minecraft:slowness", "amplifier": 0, "ticks": 100}]}
  ],
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

Check your file before you ship it, in this repository:

```sh
ARMORY_LIB_FILE=path/to/armory_lib.json cargo test checks_a_mods_file
```

### From a native mod: Rust

Your bundle needs an `[api]` package (Horizon's `docs/native-mods.md`, "Trait mods"), and this in
its `horizon-mixin.toml`:

```toml
[api]
path = "api"

[dependencies]
armory_lib = "^0.1"
```

Don't add `armory_lib` to your `Cargo.toml`: the launcher adds it when it builds. Then register
from your package's `hooks()`. What you register wins over a file entry for the same item or
entity type.

```rust
use armory_lib::{CarriedEffect, GearLight, MobLook, MobRules, Page, Place, Rig, Switch};
use horizon_client::mod_api::Hooks;

pub fn hooks() -> Box<dyn Hooks> {
    armory_lib::register_page(Page {
        mod_id: "mymod".into(),
        title: "My Mod 1.0".into(),
        description: "Glowing swords and a cave lurker.".into(),
        switches: vec![Switch::new("glow", "Glow")],
    });
    armory_lib::register_gear_light(
        GearLight::new("mymod", "mymod:glow_sword", 0x8cffe2).setting("glow"),
    );
    armory_lib::register_mob_look(
        MobLook::new("mymod", "mymod:lurker", Rig::zombie(), "mymod:entity/lurker")
            .emissive("mymod:entity/lurker_e"),
    );
    armory_lib::register_mob_rules(
        MobRules::new("mymod", "mymod:lurker")
            .no_conversion()
            .no_sunburn()
            .on_hit("minecraft:slowness", 0, 100),
    );
    armory_lib::register_carried_effect(CarriedEffect::new(
        "mymod",
        &["mymod:swift_ring"],
        Place::Hotbar,
        "minecraft:speed",
    ));
    Box::new(MyMod)
}

struct MyMod;
impl Hooks for MyMod {}
```

| Function | What it takes |
|---|---|
| `register_page(Page)` | `mod_id`, `title`, `description`, `switches: Vec<Switch>`. `Switch::new(key, label)`, `.off_by_default()` |
| `switch(host, mod_id, key) -> bool` | Reads a switch from your own hooks. At most a second old |
| `register_gear_light(GearLight)` | `GearLight::new(mod_id, item, rgb)`, `.level(0.0..=1.0)`, `.setting(key)` |
| `register_mob_look(MobLook)` | `MobLook::new(mod_id, kind, rig, texture)`, `.emissive(texture)`, `.scale(f32)` |
| `Rig` | `Rig::zombie()`, `Rig::humanoid()`, `Rig::wolf()`, `Rig::chicken()`, `Rig::custom(layer, parts)` with `Motion::Head`, `Motion::Swing { phase, reach }`, `Motion::Raised` |
| `register_mob_rules(MobRules)` | `MobRules::new(mod_id, kind)`, `.no_conversion()`, `.no_sunburn()`, `.fire_proof()`, `.on_hit(effect, amplifier, ticks)` |
| `register_carried_effect(CarriedEffect)` | `CarriedEffect::new(mod_id, items, place, effect)`, `.amplifier(u8)`, `.setting(key)` |

Armour sets, tool abilities, harvest sweeps and weapon effects are declared in the file only.

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

Not in 0.1.0: item inventories (backpacks), waypoints with teleport costs, block menus and
screens, attribute modifiers as carried effects, a way to set a hurt player on fire, and bodies
built in code rather than taken from vanilla.

## Compatibility

| | |
|---|---|
| Horizon | commit `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca` (Nightly 2026-11-08) |
| Game data | 26.3 |
| horizon-mixins | `e8cd0aa38cffef49cb2e088ef716baa107148c02` |

The library has two halves. The `[api]` package uses Horizon's catalog hooks (`mods`,
`settings`, `settings_click`, `visuals`, `mob_submit`, `living_conversion_pre`,
`living_incoming_damage_v3`, `player_tick`, `server_data_loaded`, `client_resources_reloaded`) and
`Host` helpers. The mixin half adds `armory_lib_bridge` to the client library and `armory_lib_gear`
to the game, hooks ten catalog hook points for the gear, and hooks the start of
`mobs::ai::burn_undead` for `no_sunburn`. It reads the game's state directly and hooks that raw
target, so the library is pinned to one Horizon commit.

## Build, check and package

```sh
cargo test                                   # the logic and the bridge, without the game
HORIZON_SRC=<horizon checkout> tools/check.sh   # type-check against Horizon, staged as the launcher stages it
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
  - `api/src/lib.rs`: the `[api]` package's hooks, the registry and reading the mods' files;
    `api/src/logic.rs`: its logic and tests.
  - `bridge.rs`: shared with the mixin code: the gear rules, their logic and tests, switches,
    the daylight list. `gear.rs`: the gear rules in the game. `sunburn.rs` and the other `.rs`
    files: one small helper per hook.
- `tools/`: packaging, the type-check script and the icon generator
- `docs/`: the icon and the signing guide
- `dist/`: the latest built package and its checksum

## Licence

[Mozilla Public License 2.0](LICENSE). You may use the library from a mod under any licence. If
you change the library's own files and share the result, those files stay under MPL-2.0 with their
source available.
