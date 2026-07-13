# Galactic Exodus SRS combat specification

Source issue: #1319
Parent issue: #1314
Decision inputs: #1178, #1194
Related: #1195, #1259, #1275, #1276, #1296, #1303, #1304, #1318
Base branch: `integration/882-galactic-exodus`

This document is the CURRENT_SOURCE for Galactic Exodus SRS combat.

- issue comment, legacy spec, fixtures, and implementation are evidence or regression surfaces
- when they diverge, update them from this document rather than treating them as competing authorities
- advanced hit probability, evasion probability, and final balance tuning remain deferred
- this document does not invent new combat rules beyond what current implementation and regression already fix

## 1. Status and authority

This file is the current source of truth for Phase 2 combat state, command flow, enemy action, reaction, and combat event payloads.

Authority order:

1. merged decision issues and current docs under `experiments/galactic_exodus/docs/specs/`
2. current implementation and regression tests
3. legacy `experiments/galactic_exodus/docs/archive/phase2_srs_spec.md`

Referenced evidence:

- legacy baseline: `experiments/galactic_exodus/docs/archive/phase2_srs_spec.md`
- implementation: `experiments/galactic_exodus/srs/model.py`, `experiments/galactic_exodus/srs/engine.py`
- regression: `experiments/galactic_exodus/srs/test_engine_combat.py`, `experiments/galactic_exodus/srs/test_fixture_regression.py`, `experiments/galactic_exodus/srs/fixtures/combat_*.json`

## 2. Scope and boundaries

Included:

- combat state and phase progression
- `COMBAT_STEP` command contract
- player attack resolution
- enemy action order, attack, and movement
- `COUNTERATTACK` / `DEFEND` reaction handling
- combat resource consumption and recovery timing
- enemy destruction and action skipping
- combat event payloads
- combat comparison fields used by fixture regression

Delegated to other current specs:

- encounter roll, danger score, and spawn composition: `docs/specs/srs_encounter.md`
- dropped `SALVAGE` object lifecycle, pickup, and reward effects: `docs/specs/srs_objects.md`
- non-combat SRS movement: `docs/specs/srs_movement.md`
- integrated CLI parsing and rendering: `docs/specs/integrated_cli.md`
- map generation and placement: `docs/specs/srs_map_generation.md`

Non-scope:

- encounter chance rebalance
- enemy `SALVAGE` drop probability rebalance
- pickup effect redesign
- base upgrade cost tuning
- final weapon and enemy balance
- new hit or evasion probability systems
- new weapon types, enemy tiers, or reaction types

## 3. Combat state model

Current combat state is `SrsCombatState` with these fields:

- `player`
- `enemies`
- `weapon_profiles`
- `phase`
- `combat_turn`
- `player_attack_target_id`

Derived booleans:

- `enemy_presence = bool(enemies)`
- `target_available = player_attack_target_id references an existing enemy`

Current phase enum names:

```text
PLAYER_MOVEMENT
PLAYER_ATTACK
ENEMY_ACTION
```

Baseline transition cycle:

```text
PLAYER_MOVEMENT -> PLAYER_ATTACK -> ENEMY_ACTION -> PLAYER_MOVEMENT
```

Transition rules:

- `PLAYER_MOVEMENT` accepts `COMBAT_STEP` as a phase advance only
- if `enemy_presence` is true and the selected target is attackable by at least one player weapon, the next phase is `PLAYER_ATTACK`
- otherwise `PLAYER_MOVEMENT` skips directly to `ENEMY_ACTION`
- `PLAYER_ATTACK` advances to `ENEMY_ACTION`
- `ENEMY_ACTION` advances to `PLAYER_MOVEMENT`

State invariants:

- rejected combat actions do not change phase, `combat_turn`, player resources, or enemy state
- destroying the current target clears `player_attack_target_id`
- `enemy_presence` becomes false when the last enemy is removed from `enemies`
- current implementation keeps an empty `combat_state` with `enemy_presence = false`; it does not auto-replace it with `None` during `COMBAT_STEP`

## 4. Combat command contract

Combat uses `SrsCommand(command_type="COMBAT_STEP", ...)`.

Current accepted combat-related fields:

- `player_attack_action`: `ATTACK` or `SKIP`
- `player_attack_weapon`: `PHOTON_TORPEDO` or `PHASER`
- `enemy_reactions`: mapping from enemy id to `COUNTERATTACK` or `DEFEND`
- `salvage_choice`: accepted by the command model, but current combat resolution does not consume it because dropped `SALVAGE` reward resolution is delegated to `INTERACT`

Command meaning by phase:

- `PLAYER_MOVEMENT`: advance the combat phase; no movement payload is consumed here
- `PLAYER_ATTACK`: resolve player attack or explicit skip
- `ENEMY_ACTION`: resolve all remaining enemies in tier order, then reactions

Reject conditions fixed by current implementation:

- no combat state: `REJECTED_NO_COMBAT_STATE`
- target missing: `REJECTED_TARGET_UNAVAILABLE`
- attack selected without weapon: `REJECTED_ATTACK_WEAPON_REQUIRED`
- invalid attack weapon: `REJECTED_INVALID_ATTACK_WEAPON`
- target outside range or blocked by line of sight: `REJECTED_TARGET_NOT_ATTACKABLE`
- insufficient torpedo ammo: `REJECTED_INSUFFICIENT_TORPEDO_AMMO`
- insufficient phaser energy: `REJECTED_INSUFFICIENT_PHASER_ENERGY`
`COMBAT_REJECTED` leaves `srs_turn` unchanged.

## 5. Player combat resources and capacities

Current default player combat state:

| Field | Default |
|---|---:|
| `durability` | 100 |
| `durability_capacity` | 100 |
| `defense` | 0 |
| `evasion` | 0 |
| `movement_power` | 4 |
| `photon_torpedo_ammo` | 6 |
| `photon_torpedo_ammo_capacity` | 6 |
| `photon_torpedo_power` | 0 |
| `energy` | 6 |
| `energy_capacity` | 6 |
| `phaser_power` | 0 |
| `energy_recovery` | 1 |
| `salvage` | 0 |

Current combat timing:

- torpedo ammo is consumed only on a successful torpedo attack execution
- phaser energy is consumed only on a successful phaser attack execution or successful counterattack resolution
- player energy recovers by `energy_recovery` only after `ENEMY_ACTION` resolves and `combat_turn` increments
- `COMBAT_STEP` itself does not consume SRS fuel and does not advance `srs_turn`

Current implementation stores combat upgrades directly in player state:

- `defense`
- `evasion`
- `phaser_power`
- `photon_torpedo_power`
- expanded capacities for energy and torpedo ammo

Current combat resolution does not yet apply `defense`, `evasion`, `phaser_power`, or `photon_torpedo_power` modifiers. They remain part of persistent player combat state and comparison state.

## 6. Weapon profiles

Current fixed weapon profiles:

| Weapon | Base damage | Range | Resource type | Resource cost | Power modifier field |
|---|---:|---:|---|---:|---|
| `PHASER` | 1 | 2 | `ENERGY` | 1 | `phaser_power` |
| `PHOTON_TORPEDO` | 3 | 3 | `PHOTON_TORPEDO_AMMO` | 1 | `photon_torpedo_power` |
| `ENEMY_WEAPON` | tier-based | 2 | none | 0 | none |

Enemy base damage is resolved from enemy tier defaults:

| Enemy tier | Durability | Attack damage | Movement power |
|---|---:|---:|---:|
| `TIER1` | 3 | 6 | 3 |
| `TIER2` | 5 | 7 | 3 |
| `TIER3` | 8 | 8 | 3 |
| `TIER4` | 12 | 10 | 3 |

## 7. Targeting, range, and line of sight

Current targeting contract:

- the player target is identified by `player_attack_target_id`
- the target must exist in `combat_state.enemies`
- attackability is checked from the player position to the enemy position
- enemy attackability is checked from enemy position to player position

Current distance metric:

```text
combat_range_distance = max(abs(dx), abs(dy))
```

Current line-of-sight contract:

- line of sight uses Bresenham cells between attacker and target
- attacker and target cells themselves do not block line of sight
- intermediate impassable cells block line of sight
- impassable terrain or object rules come from `srs_movement.md`

Attack rejection contract:

- out-of-range or blocked line of sight resolves as `REJECTED_TARGET_NOT_ATTACKABLE`
- rejected attacks consume no ammo and no energy

## 8. Player attack resolution

Current resolution order in `PLAYER_ATTACK`:

1. read `player_attack_action`; default to `SKIP`
2. if action is `SKIP`, emit accepted `player_action` payload with no resource change
3. validate that `target_available` is true
4. validate that a player weapon was supplied and that the weapon is one of the current player attack weapons
5. validate range and line of sight against the current target enemy
6. validate the required resource for the selected weapon
7. consume ammo or energy
8. apply fixed weapon damage to enemy durability
9. if the enemy is destroyed, remove it from `enemies`
10. if the destroyed enemy has `drop_salvage = true`, call the dropped `SALVAGE` object helper and attach `salvage_drop` payload
11. if the enemy survives, keep it with reduced durability
12. if the destroyed enemy was `player_attack_target_id`, clear the target id before storing the next combat state

Current player attack payload fields:

- `selected_action`
- `selected_weapon`
- `target_enemy_id`
- `attack_executed`
- `damage_applied`
- `resource_cost`
- `resource_type`
- `target_destroyed`
- `target_remaining_durability` when the enemy survives
- `salvage_drop` when a dropped object is generated or skipped

## 9. Enemy action order and resolution

Current enemy action ordering:

- `SrsCombatState` normalizes enemy storage by ascending tier order
- iteration order during `ENEMY_ACTION` follows the normalized enemy mapping
- equal-tier ordering is whatever insertion order produced before normalization; current tests only fix tier ordering, not a secondary enemy-id sort

Current action flow for each remaining enemy:

1. skip the slot if that enemy was already removed by an earlier reaction
2. check whether the enemy can already attack the player with `ENEMY_WEAPON`
3. if it can attack, resolve attack and the chosen reaction immediately
4. otherwise compute attackable cells around the player, choose the lowest-cost reachable one, and move up to `movement_power` cells along the path
5. after movement, do not attack in the same `COMBAT_STEP`; the payload only records whether the final position is now attackable

Current enemy action payload order matches action resolution order.

## 10. Enemy movement during combat

Current movement contract:

- `movement_power` is 3 for every current enemy tier
- path search uses Dijkstra over orthogonal movement only
- the player cell is never entered
- impassable terrain and impassable object cells are excluded
- enemy-occupied cells are not specially blocked in current path search; current tests do not define a separate enemy-body collision rule
- the chosen target is the reachable attackable position with the lowest total movement cost
- equal-cost target cells break ties by position sort order `(y, x)`
- equal-cost path expansion uses direction order `N`, `W`, `E`, `S`
- if no attackable cell is reachable, the enemy stays in place and returns an empty path

Current movement payload fields:

- `target_attackable_position`
- `planned_path`
- `moved_path`
- `final_position`
- `movement_power`
- `movement_cost`
- `can_attack_before_move`
- `can_attack_after_move`

## 11. Reaction contract

Current baseline reactions:

```text
COUNTERATTACK
DEFEND
```

### COUNTERATTACK

Current counterattack requirements:

- selected reaction for that enemy is `COUNTERATTACK`
- player has enough phaser energy for `PHASER.energy_cost`
- the enemy is within phaser range and line of sight from the player position

Current counterattack behavior:

- consume 1 phaser energy from the combat player state
- deal fixed phaser damage 1 to the enemy
- do not reduce the enemy's outgoing attack damage
- the enemy may be destroyed by the counterattack before later enemy actions resolve
- if the enemy had `drop_salvage = true`, attach dropped `SALVAGE` payload and hand off lifecycle to `srs_objects.md`

### DEFEND

Current defend behavior:

- halve incoming enemy damage
- round up with `ceil(enemy.attack_damage * 0.5)`
- do not consume player energy or ammo
- current implementation does not apply `defense` as an additional modifier

### Fallback

Current fallback rule:

- if `COUNTERATTACK` was requested but energy, range, or line of sight requirements fail, resolve as `DEFEND`

Reaction payload fields:

- `selected_reaction`
- `resolved_reaction`
- `counterattack_available`
- `fallback_to_defend`
- `damage_to_player`
- `counterattack_damage`
- `enemy_destroyed`
- `salvage_drop` when counterattack destruction triggers a dropped object helper

## 12. Enemy destruction and action skipping

Current destruction contract:

- an enemy destroyed by player attack is removed before `ENEMY_ACTION`
- an enemy destroyed by counterattack is removed immediately during that enemy's action
- later iteration skips any enemy id already removed from the current `updated_enemies`
- destroyed enemies do not receive later actions in the same `COMBAT_STEP`
- `enemy_presence` is derived from the remaining enemy count
- after the last enemy is destroyed, the phase still advances according to the normal phase transition rule; current implementation does not special-case a terminal combat phase

Current regressions fixed by tests and fixtures include:

- torpedo kill produces no later enemy action for the destroyed enemy
- counterattack kill skips subsequent actions for that enemy
- dropped `SALVAGE` remains a combat-to-object boundary, not an immediate reward

## 13. Combat turn and SRS turn accounting

Current accounting contract:

- `COMBAT_STEP` does not advance `srs_turn`
- rejected combat actions do not advance `combat_turn`
- `combat_turn` increments only after `ENEMY_ACTION` resolves
- player energy recovery is applied after `ENEMY_ACTION` and after the increment, capped by `energy_capacity`
- `PLAYER_MOVEMENT -> PLAYER_ATTACK` and `PLAYER_ATTACK -> ENEMY_ACTION` transitions do not increment `combat_turn`

Example from current fixture behavior:

```text
initial: phase=PLAYER_MOVEMENT, combat_turn=0
COMBAT_STEP -> PLAYER_ATTACK or ENEMY_ACTION, combat_turn=0
COMBAT_STEP at ENEMY_ACTION -> PLAYER_MOVEMENT, combat_turn=1, energy recovers
```

## 14. Event and payload contract

Current combat event types:

- `COMBAT_TRANSITIONED`
- `COMBAT_REJECTED`

`COMBAT_REJECTED.payload` contains:

- `command_type`
- `phase` when combat state existed
- `outcome`

`COMBAT_TRANSITIONED.payload` contains:

| Field | Meaning |
|---|---|
| `command_type` | currently always `COMBAT_STEP` |
| `phase_from` | previous combat phase |
| `phase_to` | next combat phase |
| `combat_turn_before` | previous turn counter |
| `combat_turn_after` | updated turn counter |
| `enemy_presence` | whether enemies existed before the transition payload was built |
| `target_available` | whether `player_attack_target_id` referenced a live enemy before resolution |
| `target_attackable` | whether the target was attackable at phase-advance time |
| `player_action` | player attack or skip payload, or `null` outside `PLAYER_ATTACK` |
| `enemy_actions` | ordered list of enemy action payloads, or empty list outside `ENEMY_ACTION` |
| `player_durability_before` / `after` | player durability delta |
| `player_energy_before` / `after` | player energy delta |
| `player_torpedo_ammo_before` / `after` | player torpedo ammo delta |
| `outcome` | currently `ACCEPTED` |

Representative payload shape:

```text
COMBAT_TRANSITIONED.payload:
  phase_from
  phase_to
  combat_turn_before
  combat_turn_after
  player_action or enemy_actions
  player_durability_before/after
  player_energy_before/after
  player_torpedo_ammo_before/after
```

## 15. Persistent and comparison state

Current combat comparison fields used by fixture regression:

- `combat_phase`
- `combat_turn`
- `enemy_presence`
- `combat_player_durability`
- `combat_player_energy`
- `combat_player_torpedo_ammo`
- `combat_enemy_positions`
- `combat_enemy_durabilities`

Current combat state also carries, and comparisons may inspect through event payloads:

- enemy id
- enemy tier
- enemy position
- enemy durability
- enemy `drop_salvage`
- player `salvage`
- player combat upgrade fields
- `player_attack_target_id`

Persistence boundary:

- `combat_state` is part of the live `SrsGameState`
- `SrsPersistentState` does not store combat phase, combat turn, or enemy roster
- current sector persistence model is therefore object/discovery focused, not a serialized combat snapshot model
- this document treats combat persistence across commands within one `SrsGameState` as current behavior, and does not define sector-revisit combat restoration beyond that

## 16. Reward boundary

Combat-owned boundary:

- determine whether an enemy was destroyed
- read `enemy.drop_salvage`
- call the dropped `SALVAGE` object helper when needed
- attach `salvage_drop` information to combat payloads

Delegated to `srs_objects.md`:

- dropped `SALVAGE` object schema
- pickup through `INTERACT`
- salvage value application to inventory and recovery
- occupied-cell skip and object-id collision handling details

Current combat spec does not reintroduce legacy immediate reward or `salvage_reward` behavior at kill time.

## 17. Implementation and regression references

Implementation:

- `experiments/galactic_exodus/srs/engine.py`
- `experiments/galactic_exodus/srs/model.py`

Regression tests:

- `experiments/galactic_exodus/srs/test_engine_combat.py`
- `experiments/galactic_exodus/srs/test_fixture_regression.py`

Fixtures:

- `experiments/galactic_exodus/srs/fixtures/combat_attack_blocked_los_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_attack_clear_los_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_attack_out_of_range_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_core_state_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_counterattack_salvage_drop_object_tier2_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_enemy_counterattack_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_enemy_counterattack_fallback_energy_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_enemy_defend_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_enemy_movement_tiebreak_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_energy_pressure_danger3_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_energy_pressure_danger4_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_phaser_attack_damage_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_salvage_drop_object_tier3_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_salvage_drop_occupied_cell_skip_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_salvage_drop_then_interact_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_salvage_no_drop_tier1_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_torpedo_destroy_no_counterattack_9x9.json`

Traceability issues and PR history:

- #1178
- #1194
- #1195
- #1259
- #1275
- #1276
- #1296
- #1303
- #1304
- #1319

## 18. Deferred items

Deferred:

- hit probability
- evasion probability
- defense / evasion final tuning
- advanced combat probability and tuning: #1195
- final enemy AI tuning
- final weapon balance
