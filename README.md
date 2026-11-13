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
> against Horizon `fdc87ae` linked the way the launcher links it, and its logic has unit tests.

## For players

Install Armory Lib when a mod you use asks for it.
[Useful Armory](https://github.com/snipeparparalt2/useful-armory-horizon) needs it from 0.1.7 on.

1. Download `armory_lib-<version>.hnmod` from the [releases](../../releases).
2. Put it in `horizon-native-mods/` in your Horizon game folder (`%APPDATA%\horizon` on a default
   Windows install), next to the mod that needs it.
3. Build in the Horizon launcher and start the game.

If the library is missing or too old, the launcher's build stops and names both mods.

Native mods are compiled into the game and are not sandboxed, so only install them from sources
you trust. The whole source is in [`mixins/armory_lib/api/src`](mixins/armory_lib/api/src).

## For mod authors

### What it does

| Piece | You register | The library does |
|---|---|---|
| Mob looks | An entity type, a body (`Rig`), a texture, an optional glowing texture | Draws the mob in place of its template's model, with the walk cycle, head turn, hurt flash and death fall |
| Mob rules | An entity type and its rules | Stops it converting (a zombie copy under water), makes burning harmless to it, gives its melee hits effects |
| Carried-item effects | Items, where they must be, an effect | Gives the effect while the item is there and takes it away after |
| Coloured gear light | An item, a colour, a strength | Lights the world around whoever holds or wears it, and around it on the ground |
| Settings pages | A title and switches | Adds the row to Options > Mods, draws the buttons, keeps the choices |

Each registration names your mod's id. While your mod is off in Options > Mods, its registrations
do nothing.

### Depend on it

Your bundle needs an `[api]` package (Horizon's `docs/native-mods.md`, "Trait mods"), and this in
its `horizon-mixin.toml`:

```toml
[api]
path = "api"

[dependencies]
armory_lib = "^0.1"
```

Don't add `armory_lib` to your `Cargo.toml`: the launcher adds it when it builds. Then register
from your package's `hooks()`:

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
            .fire_proof()
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

### The API

| Function | What it takes |
|---|---|
| `register_page(Page)` | `mod_id`, `title`, `description`, `switches: Vec<Switch>`. `Switch::new(key, label)`, `.off_by_default()` |
| `switch(host, mod_id, key) -> bool` | Reads a switch from your own hooks. At most a second old |
| `register_gear_light(GearLight)` | `GearLight::new(mod_id, item, rgb)`, `.level(0.0..=1.0)`, `.setting(key)` |
| `register_mob_look(MobLook)` | `MobLook::new(mod_id, kind, rig, texture)`, `.emissive(texture)`, `.scale(f32)` |
| `Rig` | `Rig::zombie()`, `Rig::humanoid()`, `Rig::wolf()`, `Rig::chicken()`, `Rig::custom(layer, parts)` with `Motion::Head`, `Motion::Swing { phase, reach }`, `Motion::Raised` |
| `register_mob_rules(MobRules)` | `MobRules::new(mod_id, kind)`, `.no_conversion()`, `.fire_proof()`, `.on_hit(effect, amplifier, ticks)` |
| `register_carried_effect(CarriedEffect)` | `CarriedEffect::new(mod_id, items, place, effect)`, `.amplifier(u8)`, `.setting(key)`. `Place::MainHand`, `OffHand`, `Hotbar` (includes the off hand), `Worn`, `AllWorn` |

Details worth knowing:

- **Textures** are sprite names: `mymod:entity/lurker` is `assets/mymod/textures/entity/lurker.png`
  in your content mod. A zombie or humanoid rig uses the 64x64 zombie layout, a wolf or chicken rig
  the 64x32 layout of that mob. The glowing texture holds only the glowing pixels.
- **A look needs an entity type of yours.** The library matches the type id the game reports, so
  it is for types your content mod adds (`horizon/entity_type/`), not for single vanilla mobs.
- **`fire_proof` cancels burning damage.** It keeps a copy of an undead mob alive in daylight, but
  the flames are still drawn. Horizon has no hook at `fdc87ae` to stop the sun lighting it.
- **Hit effects reach players only**, one tick after the hit, and only for a mob's own blow.
- **Carried effects** are given for 16 seconds and renewed every 2, without particles, and taken
  away the tick the item leaves its place. The strongest level wins when two rules give the same
  effect.
- **Lights** are visual (Horizon's `visuals` hook): at most 8 a frame, the nearest ones, within 24
  blocks. They don't change the light level blocks and spawning see.

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

The library uses only Horizon's catalog hooks (`mods`, `settings`, `settings_click`, `visuals`,
`mob_submit`, `living_conversion_pre`, `living_incoming_damage_v3`, `player_tick`) and `Host`
helpers, with no mixin injections. It is pinned to one Horizon commit for now.

## Build, check and package

```sh
cargo test                                   # the logic, without the game
HORIZON_SRC=<horizon checkout> tools/check.sh   # type-check against Horizon, as the launcher links it
python3 tools/package.py                     # dist/armory_lib-<version>.hnmod and SHA256SUMS.txt
cargo horizon-mod check dist/armory_lib-<version>.hnmod   # Horizon's own package check
```

`tools/check.sh` adds the library to the checkout's workspace, so give it a copy you don't mind
changing. `tools/gen_icon.py` redraws the icon. Releases are signed as described in
[docs/SIGNING.md](docs/SIGNING.md).

## Repository map

- `mixins/armory_lib/`: the bundle the launcher builds. `api/src/lib.rs` holds the hooks,
  `api/src/logic.rs` the logic and its tests.
- `tools/`: packaging, the type-check script and the icon generator
- `docs/`: the icon and the signing guide
- `dist/`: the latest built package and its checksum

## Licence

[Mozilla Public License 2.0](LICENSE). You may use the library from a mod under any licence. If
you change the library's own files and share the result, those files stay under MPL-2.0 with their
source available.
