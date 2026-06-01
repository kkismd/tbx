# Galactic Exodus Phase 0 Experiments

This directory contains a small Python experiment harness for the Phase 0 mathematical and game-design validation work for `Galactic Exodus`.

It is not the formal TBX application implementation. The future TBX-side implementation is expected to live elsewhere, such as `examples/galactic_exodus/`.

The current script uses a full-information shortest-path search over the 8x8 grid with no fault lines. Movement cost is charged by the terrain of the destination cell, and the CLI reports the minimum-cost route from `S` to `H` plus the base-related costs requested by issue #906.

## Run

```bash
python experiments/galactic_exodus/simulate.py --seed 42
```

To show the coordinate sequence for the minimum-cost `S -> H` path, add `--show-path`.
