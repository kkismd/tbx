# Phase 1 reference fixture implementation plan

## 1. Scope

This plan adds deterministic fixture injection and replay to the Python reference implementation without changing normal generated-game behavior.

## 2. `engine.py` changes

### 2.1 Add `validate_actual_map`

```python
def validate_actual_map(actual_map: ActualMap, settings: GameSettings) -> None:
    ...
```

It must reject:

- missing or extra board coordinates
- symbols outside `. N A @ B R S H`
- S/H not at the fixed settings positions
- `base_position` whose cell is not `B`
- `resource_positions` whose cells are not `R`
- duplicate resources
- rift edges that are non-adjacent, out of bounds, duplicated, or not normalized

### 2.2 Add `create_game_from_actual_map`

```python
def create_game_from_actual_map(
    actual_map: ActualMap,
    *,
    settings: GameSettings = DEFAULT_SETTINGS,
    requested_seed: int,
    effective_seed: int,
    reroll_count: int,
) -> GameState:
    ...
```

It must:

1. validate settings and the map
2. create the same initial state as `create_game`
3. reveal H and the S-centered 3x3 neighborhood
4. initialize path with S
5. call `determine_game_status`

Refactor `create_game` to generate/select a map and then delegate to this function.

### 2.3 Add `run_state_commands`

```python
def run_state_commands(
    state: GameState,
    commands: Iterable[str],
    *,
    max_turns: int = 256,
) -> GameLog:
    ...
```

Move the existing command loop from `run_commands` into this function. `run_commands` must retain its current signature and behavior, create the generated state, and delegate.

### 2.4 Keep evaluated loss behavior

Do not change `determine_game_status` or `can_continue`.

The evaluated Phase 1B behavior is:

- H arrival wins first
- otherwise, if actual map has no adjacent non-rift destination whose terrain cost is payable with remaining fuel, status becomes `LOST_FUEL`
- this can occur with positive remaining fuel

The UI must not reveal the hidden actual-map reason.

### 2.5 Generation dependency injection

Refactor candidate selection to accept defaults that preserve normal behavior:

```python
CandidateGenerator = Callable[[int, int, float], simulate.GalacticMap]
ReachabilityPredicate = Callable[[simulate.GalacticMap], bool]


def create_playable_map(
    requested_seed: int,
    settings: GameSettings,
    *,
    generate_candidate: CandidateGenerator = simulate.generate_map,
    is_reachable: ReachabilityPredicate = is_goal_reachable,
) -> tuple[simulate.GalacticMap, int, int]:
    ...
```

This is used only to replay deterministic generation-error fixtures. Existing callers need no changes.

## 3. New replay module

Add:

```text
experiments/galactic_exodus/replay_phase1_reference.py
```

Required functions:

```python
load_fixture_file(path) -> dict
settings_from_dict(value) -> GameSettings
actual_map_from_dict(value) -> ActualMap
replay_fixture(fixture) -> GameLog
assert_partial_match(expected, actual, path="$") -> None
replay_all(path) -> None
```

CLI:

```bash
python experiments/galactic_exodus/replay_phase1_reference.py \
  --fixtures experiments/galactic_exodus/fixtures/phase1_reference.json
```

It must exit non-zero on the first mismatch and identify fixture name and JSON path.

## 4. Fixture map format

Use the same representation as `engine.actual_map_to_dict`:

```json
{
  "cells": [
    {"position": {"x": 1, "y": 1}, "symbol": "S"}
  ],
  "rift_edges": [
    [
      {"x": 1, "y": 1},
      {"x": 2, "y": 1}
    ]
  ],
  "base_position": {"x": 4, "y": 4},
  "resource_positions": []
}
```

All 64 cells must be recorded. `base_position` is never null because the current reference map always contains B. Every edge is normalized and sorted.

## 5. Fixture modes

### `generated`

- run `create_game(requested_seed, settings)`
- assert requested/effective/reroll metadata
- assert generated actual map exactly equals `initial_actual_map`
- then run commands from the generated state

### `injected`

- load `initial_actual_map`
- call `create_game_from_actual_map`
- run commands with `run_state_commands`

### `generation_error`

Use fixture data:

```json
"generation_stub": {
  "reachable_sequence": [false]
}
```

For seed-overflow coverage, use `requested_seed = 9223372036854775807`. Inject a deterministic candidate generator returning a canonical valid map and an `is_reachable` predicate that returns `false` for the first candidate. The next candidate calculation must produce the actual `SEED_OVERFLOW` GenerationError.

## 6. Correct the 12 fixtures

### no-reroll initial board

- find a confirmed `reroll_count=0` seed by executing the current engine
- serialize the complete selected actual map
- do not hand-write effective seed

### reroll requested/effective seed

- scan deterministic seeds until `reroll_count>0`
- serialize the complete selected actual map and actual metadata
- do not assume seed 123 rerolls

### normal move / rift / B / R fixtures

- fixed S=(1,1), H=(8,8)
- include a valid B cell and non-null `base_position` in every injected map
- serialize all 64 cells

### zero-fuel H arrival

- keep S=(1,1), H=(8,8)
- all traversed cells cost 1
- use `initial_fuel=14`
- commands: seven `E`, then seven `N`
- final H move leaves fuel 0 and status `WON`

### fuel-loss fixture

- use a fixed map and command sequence that the current `determine_game_status` actually classifies as `LOST_FUEL`
- assert both remaining fuel and the lack of any payable actual adjacent move

### generation error

- use the injected reachability sequence described above
- assert reason, attempts, requested seed, and last candidate seed

### turn limit

- use an injected map, `max_turns=1`, and at least two valid commands
- assert one event and `ABORTED_TURN_LIMIT`

## 7. Comparison rules

`expected_initial`, each `expected_turns[i]`, and `expected_final` are recursive partial matches:

- every expected key must exist and equal the actual value
- extra actual keys are allowed
- arrays are ordered and compared by index
- no `*_include` ad-hoc keys such as `known_cells_include`
- when subset behavior is needed, record the exact expected array from the reference implementation

## 8. Tests

Add:

```text
experiments/galactic_exodus/test_phase1_reference_fixtures.py
```

Required tests:

- replay all 12 fixtures successfully
- wrong fixture schema is rejected
- malformed actual map is rejected
- generated actual-map mismatch is rejected
- expected turn mismatch reports fixture and path
- generation error is produced by the injected dependency path
- fixed-start zero-fuel H fixture wins
- positive-fuel no-move state remains covered as `LOST_FUEL`

Keep `validate_phase1_spec.py` as static structure validation. The replay test provides semantic validation.
