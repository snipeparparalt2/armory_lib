# Changelog

## 0.1.0 (unreleased)

First version, for Horizon `fdc87ae` (Nightly 2026-11-08, game data 26.3).

- Content mods need no code: the library reads `data/<mod id>/armory_lib.json` from every
  content mod and carries out what it declares, when a world opens and on `/reload`.
- Gear rules, from the file: armour set bonuses (effects, no fall damage, healing, less knockback
  or explosion damage, more experience), sneak abilities (vein mining, 3 x 3 digging, tree
  felling), harvest sweeps, and weapon effects (backstab, splash, fire, healing on a kill,
  bonuses against mobs or in a dimension, head drops).
- Mob looks: `register_mob_look` draws a mod entity type with its own texture, and an optional
  glowing layer, on a vanilla body (zombie, humanoid, wolf, chicken or any other model layer),
  with the walk cycle, head turn, hurt flash and death fall.
- Mob rules: `register_mob_rules` keeps a type from converting, stops daylight setting it alight,
  makes fire harmless to it, and gives its melee hits effects.
- Carried-item effects: `register_carried_effect` gives an effect while an item is in the main
  hand, the off hand, the hotbar, an armour slot, or while a whole set is worn.
- Coloured gear light: `register_gear_light` makes an item light the world in its colour while
  held, worn or lying on the ground.
- Settings pages: `register_page` adds a row to Options > Mods with one switch per feature;
  `switch` reads one.
- A plain mixin bundle, with no `[api]` package: launchers newer than Horizon `fdc87ae` build it
  (a trait mod made them generate an executable for a newer Horizon, which failed with
  ``cannot find type `GameAllocator` in crate `horizon_threads` ``). The `register_*` functions
  are `horizon_client::game::armory_lib` in the game, for other mixin bundles.

Not tested in a running game yet.
