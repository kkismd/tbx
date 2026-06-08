# Galactic Exodus Phase 0 Experiments

This directory contains a small Python experiment harness for the Phase 0 mathematical and game-design validation work for `Galactic Exodus`.

It is not the formal TBX application implementation. The future TBX-side implementation is expected to live elsewhere, such as `examples/galactic_exodus/`.

## Run

```bash
python experiments/galactic_exodus/simulate.py --seed 42 --rift-density 0.10
```

Run the multi-seed Phase 0 metrics report with:

```bash
python experiments/galactic_exodus/metrics.py --seed-start 1 --seed-count 1000 --rift-density 0.10 --resource-count 3
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

This experiment computes a full-information baseline with optional fault-line restrictions on movement edges.

## Fault-Line Rifts

The grid has 112 undirected adjacent edges in total. Rift edges are chosen deterministically from the seed:

```text
rift_count = round(112 * rift_density)
```

Use `--rift-density FLOAT` to control the density. The default is `0.10`.

Selected rift edges are impassable in both directions and are excluded from all shortest-path calculations:

- `S -> H`
- `S -> B`
- `B -> H`
- `S -> H` while forbidding `B`

## Report Output

`simulate.py` prints the following sections in order:

- `MAP ID`
- `OBJECTS`
- `PARAMETERS`
- `MAP`
- `COSTS`
- `VERDICT`

The `COSTS` section includes the shortest-path metrics used by the verdict classifier:

- `S_to_H_cost`
- `S_to_H_steps`
- `S_to_B_cost`
- `B_to_H_cost`
- `S_to_H_via_B_cost`
- `S_to_H_without_B_cost`
- `base_route_advantage_raw`
- `base_is_mandatory` (`yes` / `no`)

Unavailable metrics are rendered as `N/A`.

## Verdict Rules

The verdict priority order is:

1. `REJECT_TOO_HARD`
2. `REJECT_BASE_MANDATORY`
3. `ACCEPT`

Classification rules:

- `REJECT_TOO_HARD`: at least one of `S -> H`, `S -> B`, or `B -> H` is unreachable
- `REJECT_BASE_MANDATORY`: all required segments are reachable, but `S -> H` has no route that avoids `B`
- `ACCEPT`: any map not rejected by the higher-priority rules

`ACCEPT` is only a minimal candidate verdict. It does not mean the map is already proven fun, balanced, or final-quality.

## Batch Metrics

`metrics.py` evaluates a contiguous seed range under fixed `rift_density` and `resource_count`.
The output is deterministic for the same inputs because each map and rift set is derived from the numeric seed.

Required inputs:

- `--seed-start`: first seed in the contiguous range
- `--seed-count`: number of seeds to execute
- `--rift-density`: shared fault-line density for the whole batch
- `--resource-count`: shared resource count for the whole batch

The text report includes:

- verdict counts and ratios
- `S_to_H_cost` min / median / p90 / max
- `S_to_H_steps` min / median / p90 / max
- `base_is_mandatory` counts and ratios
- `base_route_advantage_raw` negative / zero / positive / unavailable counts and ratios

`S_to_H_cost` and `S_to_H_steps` exclude unreachable runs from the distribution statistics and show those runs as `excluded_unreachable`.

`base_route_advantage_raw` means:

- negative: the best route through `B` is worse than the best route that avoids `B`
- zero: routing through `B` and avoiding `B` tie on total cost
- positive: routing through `B` is cheaper than avoiding `B`
- unavailable: either the route via `B` or the route without `B` does not exist

`p90` uses the nearest-rank rule on the sorted reachable samples.

## Tests

Run the Python experiment tests with the standard library `unittest` runner:

```bash
python -m unittest experiments.galactic_exodus.test_simulate experiments.galactic_exodus.test_metrics
```
