# Galactic Exodus prototype

This directory contains the Python prototype and evaluation environment for Galactic Exodus before the TBX implementation. It is used to validate the project incrementally across Phase 1 LRS, Phase 2 SRS, and the integrated command-response CLI. It is not the final TBX application. The current gameplay specification lives under [`docs/specs/`](docs/specs/).

## Status

- LRS Phase 1 prototype: available
- SRS prototype / fixtures: available
- `integrated_play.py`: command-response prototype
- Phase 1 LRS-only `play.py` and completed evaluation scripts: archived under `archive/evaluation/`
- Current implementation is incremental and not the final TBX application

## Quick start

Run these commands from the repository root:

```bash
python experiments/galactic_exodus/integrated_play.py --seed 42
python -m unittest discover experiments/galactic_exodus
```

## Available entrypoints

### Integrated LRS / SRS CLI

- Script: [`integrated_play.py`](integrated_play.py)
- Summary: command-response prototype that combines LRS, SRS, and HUD flows
- Specification: [`docs/specs/integrated_cli.md`](docs/specs/integrated_cli.md)

Example:

```bash
python experiments/galactic_exodus/integrated_play.py --seed 42
```

### Non-interactive Phase 1 engine

- Module: `experiments.galactic_exodus.engine`
- Main APIs: `create_game`, `apply_command`, `run_commands`
- Specification: [`docs/specs/phase1.md`](docs/specs/phase1.md)

### Phase 1 reference fixture replay

- Script: [`replay_phase1_reference.py`](replay_phase1_reference.py)
- Fixtures: [`fixtures/phase1_reference.json`](fixtures/phase1_reference.json)
- Implementation history: [`docs/archive/phase1_reference_fixture_plan.md`](docs/archive/phase1_reference_fixture_plan.md)

Example:

```bash
python experiments/galactic_exodus/replay_phase1_reference.py \
  --fixtures experiments/galactic_exodus/fixtures/phase1_reference.json
```

### SRS fixture runner

- Module entrypoint: `experiments.galactic_exodus.srs.run_fixture`
- Fixtures: [`srs/fixtures/`](srs/fixtures/)

Example:

```bash
python -m experiments.galactic_exodus.srs.run_fixture \
  experiments/galactic_exodus/srs/fixtures/resource_cache_single_9x9.json
```

### Current generation helper and archived evaluation scripts

- [`simulate.py`](simulate.py): deterministic Phase 1 map generation sample
- Archived Phase 1 evaluation scripts: [`archive/evaluation/phase1_lrs/`](archive/evaluation/phase1_lrs/)
- Archived SRS evaluation scripts: [`archive/evaluation/srs/`](archive/evaluation/srs/)
- Archived manual-session runner, recovery, and validation scripts: [`archive/evaluation/manual_sessions/`](archive/evaluation/manual_sessions/)
- Evaluation reports: [`docs/evaluations/README.md`](docs/evaluations/README.md)

Examples:

```bash
python experiments/galactic_exodus/simulate.py --seed 42
python experiments/galactic_exodus/archive/evaluation/phase1_lrs/metrics.py --seed-start 1 --seed-count 10
python experiments/galactic_exodus/archive/evaluation/phase1_lrs/fuel_metrics.py \
  --seed-start 1 \
  --seed-count 10 \
  --rift-density 0.10 \
  --initial-fuels 14,16,18 \
  --base-supplies 8,10 \
  --resource-supply 5 \
  --resource-counts 0,1,3
```

## Tests

Primary prototype test command:

```bash
python -m unittest discover experiments/galactic_exodus
```

Repository-wide checks:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Documentation

### Specifications

- Index: [`docs/specs/README.md`](docs/specs/README.md)
- Integrated CLI: [`docs/specs/integrated_cli.md`](docs/specs/integrated_cli.md)
- SRS map generation: [`docs/specs/srs_map_generation.md`](docs/specs/srs_map_generation.md)
- SRS movement: [`docs/specs/srs_movement.md`](docs/specs/srs_movement.md)
- SRS objects: [`docs/specs/srs_objects.md`](docs/specs/srs_objects.md)
- SRS warp: [`docs/specs/srs_warp.md`](docs/specs/srs_warp.md)
- Phase 1: [`docs/specs/phase1.md`](docs/specs/phase1.md)
- SRS combat: [`docs/specs/srs_combat.md`](docs/specs/srs_combat.md)
- Display: [`docs/specs/display.md`](docs/specs/display.md)
- SRS encounter: [`docs/specs/srs_encounter.md`](docs/specs/srs_encounter.md)

### Evaluations

- [`docs/evaluations/README.md`](docs/evaluations/README.md)
- Evaluation documents are evidence and reproduction notes, not the current gameplay specification

### Design

- [`docs/design/galactic_exodus_display_samples.md`](docs/design/galactic_exodus_display_samples.md)
- Design references are not the gameplay specification

### Archive

- [`docs/archive/README.md`](docs/archive/README.md)
- Archive documents are implementation history, not the current source
- [`archive/README.md`](archive/README.md)
- Archived code is historical evaluation support, not current runtime or operator tooling

### Traceability

- [`docs/spec_traceability.md`](docs/spec_traceability.md)
- Use this to track issue, legacy document, and current spec mappings

## Repository layout

| Path | Purpose |
|---|---|
| `engine.py` / `integrated_play.py` | Main prototype engine and current interactive entrypoint |
| `simulate.py` | Current deterministic Phase 1 map generation helper |
| `fixtures/` | Phase 1 reference fixtures |
| `srs/` | Phase 2 SRS prototype code, fixtures, and tests |
| `archive/evaluation/` | Completed evaluation support scripts retained for reference |
| `docs/specs/` | Current gameplay specifications |
| `docs/evaluations/` | Evaluation reports and reproduction notes |
| `docs/design/` | Design references and display samples |
| `docs/archive/` | Archived implementation history |
| `docs/spec_traceability.md` | Traceability between issues, legacy docs, and current docs |
| `results/` / `srs/results/` | CSV, JSON, and other raw outputs |

## Authority

この README は実行方法と文書への導線を提供する entrypoint であり、gameplay 仕様の正本ではありません。現行仕様は `experiments/galactic_exodus/docs/specs/` を参照してください。評価根拠は `docs/evaluations/`、設計資料は `docs/design/`、履歴資料は `docs/archive/` に分離されています。
