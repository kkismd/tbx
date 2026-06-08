# Galactic Exodus Phase 0 Experiments

This directory contains a small Python experiment harness for the Phase 0 mathematical and game-design validation work for `Galactic Exodus`.

It is not the formal TBX application implementation. The future TBX-side implementation is expected to live elsewhere, such as `examples/galactic_exodus/`.

## Run

```bash
python experiments/galactic_exodus/simulate.py --seed 42
```

## Terrain Costs

The baseline path analysis treats movement as four-directional (`N/E/S/W`) on the 8x8 grid.
Each move adds the cost of the destination cell. The starting cell does not add cost.

| Symbol | Meaning | Cost |
| --- | --- | --- |
| `.` | normal sector | 1 |
| `N` | nebula | 2 |
| `A` | asteroid field | 3 |
| `@` | gravity well / gravity anomaly | 2 |
| `B` | base | 1 |
| `R` | resource | 1 |
| `S` | start | 0 |
| `H` | home | 1 |

This experiment computes a baseline with full map information and no fault-line restrictions.

## COSTS Output

`simulate.py` now prints a `COSTS` section after the map:

- `reachable`: whether `S -> H` is reachable
- `best_cost`: minimum terrain cost from `S` to `H`
- `best_path_length`: minimum number of moves among paths with `best_cost`
- `cost_to_base`: minimum terrain cost from `S` to `B`
- `cost_base_to_goal`: minimum terrain cost from `B` to `H`
- `best_cost_via_base`: `cost_to_base + cost_base_to_goal`

Internally these values stay numeric or `None`. The output layer converts them to `yes` / `no` and `N/A`.

## Tests

Run the Python experiment tests with the standard library `unittest` runner:

```bash
python -m unittest experiments.galactic_exodus.test_simulate
```
