# Changelog

## 0.1.0 (unreleased)

First version, for Horizon `fdc87ae` (Nightly 2026-11-08, game data 26.3).

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

Not tested in a running game yet.
