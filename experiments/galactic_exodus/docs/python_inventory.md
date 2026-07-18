# Galactic Exodus Python file inventory

Source issue: #1336
Parent: #1309
Related: #1308, #1313, #1317, #1318, #1340
Base branch: `integration/882-galactic-exodus`

This document records the inventory and classification of Python files under `experiments/galactic_exodus/`. It is an integration-readiness audit document, not a gameplay specification.

## 1. Summary

- Scope audited: `experiments/galactic_exodus/**/*.py`
- File count: 77 tracked files
- `git ls-files 'experiments/galactic_exodus/**/*.py' | sort` and `find experiments/galactic_exodus -name '*.py' | sort` matched; no untracked Python files were found under this tree during the audit.
- Validation viewpoints used in this inventory:
  - import and package entrypoints inside `experiments/galactic_exodus/`
  - `python -m unittest discover experiments/galactic_exodus`
  - `python -m unittest discover experiments/galactic_exodus/srs`
  - current README / spec / evaluation document command references

Classification summary after #1340 archive movement:

| Classification | Count | Notes |
|---|---:|---|
| `KEEP` | 53 | Current implementation modules plus current regression tests/support |
| `KEEP_AS_TOOL` | 4 | Manual CLIs and fixture runners that are still useful even when not imported as core modules |
| `EVALUATION_SUPPORT` | 1 | Evaluation helper still imported by current runtime |
| `LEGACY_REFERENCE` | 0 | No file needed this label in the current tree |
| `ARCHIVED_EVALUATION` | 19 | Completed evaluation support scripts retained under `archive/evaluation/` |
| `ARCHIVE_CANDIDATE` | 0 | No file remains only as a candidate after #1340 |
| `DELETE_CANDIDATE` | 0 | No file was marked safe-to-delete in this audit |
| `UNKNOWN` | 0 | No file required additional ownership clarification to classify |

## 1.1 Archive movement from #1340

The following files were moved without deletion. The archived files are past evaluation reference material, not current runtime modules, current gameplay specification authorities, or active operator-facing tools.

| Original path | Archived path | Classification change |
|---|---|---|
| `experiments/galactic_exodus/create_manual_sessions_csv.py` | `experiments/galactic_exodus/archive/evaluation/manual_sessions/create_manual_sessions_csv.py` | `ARCHIVE_CANDIDATE` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/evaluate_manual_sessions.py` | `experiments/galactic_exodus/archive/evaluation/manual_sessions/evaluate_manual_sessions.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/run_manual_sessions.py` | `experiments/galactic_exodus/archive/evaluation/manual_sessions/run_manual_sessions.py` | `KEEP_AS_TOOL` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/test_run_manual_sessions.py` | `experiments/galactic_exodus/archive/evaluation/manual_sessions/test_run_manual_sessions.py` | `KEEP` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/evaluate_policies.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/evaluate_policies.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/fuel_metrics.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/fuel_metrics.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/metrics.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/metrics.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/play.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/play.py` | `KEEP_AS_TOOL` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/test_play.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/test_play.py` | `KEEP` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/validate_phase1_spec.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/validate_phase1_spec.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/validate_phase1b_results.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/validate_phase1b_results.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/srs/evaluate_policies.py` | `experiments/galactic_exodus/archive/evaluation/srs/evaluate_policies.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/srs/validate_phase2_decisions.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_decisions.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/srs/validate_phase2_initial_model.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_initial_model.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/srs/validate_phase2_results.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_results.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/srs/validate_phase2_srs_elements.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_srs_elements.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/srs/validate_phase2_srs_generation.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_srs_generation.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/srs/validate_phase2_srs_movement.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_srs_movement.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |
| `experiments/galactic_exodus/srs/validate_phase2_srs_spec.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_srs_spec.py` | `EVALUATION_SUPPORT` -> `ARCHIVED_EVALUATION` |

`simulate.py` remains at `experiments/galactic_exodus/simulate.py` because `engine.py` imports it directly for current runtime map generation. If it is archived later, the runtime map generation dependency should be split into a separate active module first.

## 2. Inventory table

| File | Kind | Role | Referenced by | Test / command coverage | Classification | Recommended action | Notes |
|---|---|---|---|---|---|---|---|
| `archive/evaluation/manual_sessions/create_manual_sessions_csv.py` | generated support | Bootstrap blank manual-session CSV rows from JSON logs | Archived reference only; no current README/docs command | Not part of current active unittest surface | `ARCHIVED_EVALUATION` | Retain under archive for manual session recovery reference | Superseded by the archived manual-session runner, but may help reconstruct old logs |
| `display.py` | implementation | Render current Phase 1 / integrated map output | Imported by `play.py` and `integrated_play.py`; cited by specs | Covered by `test_display.py` and snapshot tests | `KEEP` | Retain as active rendering module | Current CLI rendering path |
| `display_reference.py` | generated support | Build snapshot reference states and expected render strings for tests | Imported by display snapshot tests | Covered by `test_display.py`, `test_display_snapshot.py`, `srs/test_render.py`, `srs/test_display_snapshot.py` | `KEEP` | Retain as active regression support | Current snapshot oracle support, not dead legacy |
| `engine.py` | implementation | Phase 1 non-interactive engine and log generation | Imported by current tools, tests, docs, and archived evaluation scripts | Covered by `test_engine.py` and dependent tests | `KEEP` | Retain as active core implementation | Foundation for Phase 1 prototype behavior |
| `archive/evaluation/manual_sessions/evaluate_manual_sessions.py` | evaluation / analysis | Validate `prototype_manual_sessions.csv` against JSON logs | Archived reference; tested through moved import path | Covered by `test_evaluate_manual_sessions.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed manual-session validator | Protects past manual-eval CSV integrity without presenting it as a current operator command |
| `archive/evaluation/phase1_lrs/evaluate_policies.py` | evaluation / analysis | Run deterministic Phase 1 policy batches and emit CSV/summary | Archived reference; reproduction docs point to archive path | Covered by `test_evaluate_policies.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed Phase 1B evidence generator | Needed to inspect published Phase 1B automated evidence |
| `event_format.py` | implementation | Format prototype events for display output | Imported by `play.py` and `integrated_play.py`; cited by display spec | Covered by `test_event_format.py` | `KEEP` | Retain as active formatting layer | Current display/event contract |
| `archive/evaluation/phase1_lrs/fuel_metrics.py` | evaluation / analysis | Compare Phase 1 fuel/resource tuning across seed ranges | Archived reference; reproduction docs point to archive path | Covered by `test_fuel_metrics.py` and CLI tests in `test_simulate.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed fuel comparison evidence generator | Generates published fuel comparison evidence |
| `hud.py` | implementation | Render compact HUD/status summaries | Imported by `play.py`, `integrated_play.py`, `srs/run_manual_eval.py` | Covered by `test_hud.py` and CLI tests | `KEEP` | Retain as active UI support | Shared display helper |
| `integrated_play.py` | CLI / manual tool | Interactive integrated LRS/SRS command-response prototype | Documented in README and integrated CLI spec; covered by dedicated tests | Covered by `test_integrated_play.py` | `KEEP_AS_TOOL` | Retain as current manual CLI entrypoint | Not imported as library core, but active operator-facing tool |
| `archive/evaluation/phase1_lrs/metrics.py` | evaluation / analysis | Aggregate Phase 1 map metrics over seed ranges | Archived reference; imported by archived evaluation scripts | Covered by `test_metrics.py` and CLI tests in `test_simulate.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed evaluation support module | Feeds archived reports and tuning scripts |
| `archive/evaluation/phase1_lrs/play.py` | CLI / manual tool | Interactive Phase 1 LRS prototype CLI | Archived reference; archived `run_manual_sessions.py` points to it for old manual-session flow | Covered by archived `archive/evaluation/phase1_lrs/test_play.py` when run directly | `ARCHIVED_EVALUATION` | Retain with `test_play.py` under archive | Phase 1 LRS-only play surface is no longer the current README entrypoint |
| `replay_phase1_reference.py` | fixture runner | Replay `phase1_reference.json` against current engine | Documented in README and archive plan; used by fixture test | Covered by `test_phase1_reference_fixtures.py` | `KEEP_AS_TOOL` | Retain as manual regression runner | Useful when checking Phase 1 reference fixture drift |
| `archive/evaluation/manual_sessions/run_manual_sessions.py` | CLI / manual tool | Run archived Phase 1 LRS manual sessions and append scored feedback CSV rows | Archived with Phase 1 manual-session evaluation flow | Covered by archived `archive/evaluation/manual_sessions/test_run_manual_sessions.py` when run directly | `ARCHIVED_EVALUATION` | Retain under archive as completed manual-session runner | Coupled to archived Phase 1 LRS-only `play.py` |
| `simulate.py` | evaluation / analysis | Deterministic Phase 1 map generation sample and CLI helpers | Documented in README; imported directly by `engine.py` and archived evaluation stack | Covered by `test_simulate.py` | `EVALUATION_SUPPORT` | Retain in active tree | Excluded from archive because current runtime imports it directly |
| `srs/__init__.py` | implementation | Expose SRS package convenience exports | Imported by `integrated_play.py` and tests via package API | Covered indirectly by unittest discovery and `run_fixture` callers | `KEEP` | Retain as active package surface | Current package entrypoint |
| `srs/contracts.py` | implementation | Define/load SRS contracts and shared constants | Imported throughout SRS engine/generate/evaluate/test paths | Covered by `srs/test_contracts.py` and wider SRS suite | `KEEP` | Retain as active SRS core module | Central contract boundary |
| `srs/encounter.py` | implementation | Compute SRS encounter rules and rolls | Imported by `srs/run_fixture.py` and referenced by specs/tests | Covered by `srs/test_encounter.py` and `srs/test_encounter_balance.py` | `KEEP` | Retain as active SRS core module | Current encounter implementation |
| `srs/engine.py` | implementation | Apply SRS commands and state transitions | Imported by fixture runner, evaluation scripts, `integrated_play.py`, and tests | Covered by combat/movement/interaction/warp suites | `KEEP` | Retain as active SRS core module | Current SRS state machine |
| `archive/evaluation/srs/evaluate_policies.py` | evaluation / analysis | Run deterministic SRS policy batches and emit reports | Archived reference; verified by tests through moved import path | Covered by `srs/test_evaluate_policies.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed SRS policy evidence generator | Needed to inspect published SRS policy evidence |
| `srs/event_format.py` | implementation | Format SRS events and display-facing summaries | Imported by `srs/run_manual_eval.py` and cited by display specs | Covered by `srs/test_event_format.py` | `KEEP` | Retain as active SRS formatting layer | Current display/event contract |
| `srs/generate.py` | implementation | Create SRS sectors from descriptors/contracts | Imported by engine/evaluate/run_fixture paths and cited by specs | Covered by `srs/test_generate.py` and dependent suites | `KEEP` | Retain as active SRS core module | Current sector generation implementation |
| `srs/log.py` | implementation | Define SRS log/event payload helpers | Imported broadly across SRS engine/evaluation/tests | Covered by `srs/test_log.py` and dependent suites | `KEEP` | Retain as active SRS core module | Shared event vocabulary |
| `srs/model.py` | implementation | Define SRS domain model/state objects | Imported throughout SRS implementation, render, validators, and tests | Covered by `srs/test_model.py` and wider suite | `KEEP` | Retain as active SRS core module | Shared SRS domain types |
| `srs/render.py` | implementation | Render SRS maps and display rows | Imported by `srs/run_fixture.py`, `integrated_play.py`, `hud.py`, and tests | Covered by `srs/test_render.py` and display snapshot tests | `KEEP` | Retain as active SRS rendering module | Current SRS render path |
| `srs/run_fixture.py` | fixture runner | Replay JSON SRS fixtures and assert expected turns/final state | Documented in README and used by validators/tests | Covered by `srs/test_fixtures.py`, `srs/test_fixture_regression.py`, and direct CLI smoke in unittest | `KEEP_AS_TOOL` | Retain as current manual/CI fixture runner | Core regression entrypoint for SRS fixtures |
| `srs/run_manual_eval.py` | CLI / manual tool | Interactive fixture review tool for SRS display/manual evaluation | Referenced by display spec and Phase 2 playtest notes; covered by tests | Covered by `srs/test_run_manual_eval.py` | `KEEP_AS_TOOL` | Retain as manual evaluation helper | Still useful for fixture-by-fixture review even though outputs are not checked in |
| `srs/test_contracts.py` | test | Regression/unit test for contracts | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_display_snapshot.py` | test | Regression/unit test for display snapshot output | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_encounter.py` | test | Regression/unit test for encounter rules | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_encounter_balance.py` | test | Regression/unit test for encounter balance | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_engine_combat.py` | test | Regression/unit test for combat state transitions | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_engine_interaction.py` | test | Regression/unit test for interact/object lifecycles | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_engine_movement.py` | test | Regression/unit test for movement behavior | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_engine_warp.py` | test | Regression/unit test for warp behavior | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_evaluate_policies.py` | test | Regression/unit test for SRS policy evaluation | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_event_format.py` | test | Regression/unit test for SRS event formatting | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_fixture_regression.py` | test | Regression/unit test for stored fixture regressions | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_fixtures.py` | test | Regression/unit test for fixture runner behavior | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_generate.py` | test | Regression/unit test for sector generation | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_log.py` | test | Regression/unit test for SRS log payloads | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_model.py` | test | Regression/unit test for SRS model invariants | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_observation.py` | test | Regression/unit test for observation reveal behavior | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_render.py` | test | Regression/unit test for SRS rendering | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_run_manual_eval.py` | test | Regression/unit test for manual evaluation runner | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_validate_phase2_decisions.py` | test | Regression/unit test for decision artifact validator | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_validate_phase2_initial_model.py` | test | Regression/unit test for initial-model validator | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_validate_phase2_results.py` | test | Regression/unit test for results validator | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_validate_phase2_srs_elements.py` | test | Regression/unit test for SRS element validator | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_validate_phase2_srs_generation.py` | test | Regression/unit test for generation validator | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_validate_phase2_srs_movement.py` | test | Regression/unit test for movement validator | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `srs/test_validate_phase2_srs_spec.py` | test | Regression/unit test for spec validator | Unittest discovery under `experiments/galactic_exodus` and SRS-specific suites | Directly executed by `python -m unittest discover experiments/galactic_exodus/srs` | `KEEP` | Retain as current regression coverage | Current SRS automated test coverage |
| `archive/evaluation/srs/validate_phase2_decisions.py` | evaluation / analysis | Validate `phase2_decisions.csv` artifact structure | Archived reference; tested through moved import path | Covered by `srs/test_validate_phase2_decisions.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed artifact validator | Protects decision-log contract for historical evidence |
| `archive/evaluation/srs/validate_phase2_initial_model.py` | evaluation / analysis | Validate initial-model markdown/json artifacts | Archived reference; tested through moved import path | Covered by `srs/test_validate_phase2_initial_model.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed artifact validator | Protects archived/current Phase 2 model evidence |
| `archive/evaluation/srs/validate_phase2_results.py` | evaluation / analysis | Validate Phase 2 reference fixture/results contracts | Archived reference; tested through moved import path | Covered by `srs/test_validate_phase2_results.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed artifact validator | Checks replayable SRS evidence |
| `archive/evaluation/srs/validate_phase2_srs_elements.py` | evaluation / analysis | Validate terrain/object placement artifact contracts | Archived reference; tested through moved import path | Covered by `srs/test_validate_phase2_srs_elements.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed artifact validator | Protects SRS element artifact consistency |
| `archive/evaluation/srs/validate_phase2_srs_generation.py` | evaluation / analysis | Validate generation artifact contracts and summaries | Archived reference; tested through moved import path | Covered by `srs/test_validate_phase2_srs_generation.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed artifact validator | Protects generation evidence artifacts |
| `archive/evaluation/srs/validate_phase2_srs_movement.py` | evaluation / analysis | Validate movement artifact contracts | Archived reference; tested through moved import path | Covered by `srs/test_validate_phase2_srs_movement.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed artifact validator | Protects movement evidence artifacts |
| `archive/evaluation/srs/validate_phase2_srs_spec.py` | evaluation / analysis | Validate Phase 2 SRS spec cross-reference contract | Archived reference; tested through moved import path | Covered by `srs/test_validate_phase2_srs_spec.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed artifact validator | Spec consistency checker |
| `test_display.py` | test | Regression/unit test for Phase 1 display rendering | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_display_snapshot.py` | test | Regression/unit test for stored display snapshots | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_engine.py` | test | Regression/unit test for Phase 1 engine behavior | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_evaluate_manual_sessions.py` | test | Regression/unit test for manual-session validator | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_evaluate_policies.py` | test | Regression/unit test for Phase 1 policy evaluation | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_event_format.py` | test | Regression/unit test for event formatting | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_fuel_metrics.py` | test | Regression/unit test for fuel metrics tooling | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_hud.py` | test | Regression/unit test for HUD rendering helpers | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_integrated_play.py` | test | Regression/unit test for integrated CLI behavior | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_metrics.py` | test | Regression/unit test for metrics aggregation | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_phase1_reference_fixtures.py` | test | Regression/unit test for Phase 1 reference fixtures | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `archive/evaluation/phase1_lrs/test_play.py` | test | Regression/unit test for archived Phase 1 CLI behavior | Archived with `play.py` | Directly runnable by path or explicit unittest target | `ARCHIVED_EVALUATION` | Retain with archived `play.py` | Archived together so the old CLI behavior remains inspectable |
| `archive/evaluation/manual_sessions/test_run_manual_sessions.py` | test | Regression/unit test for archived manual-session runner | Archived with `run_manual_sessions.py` | Directly runnable by path or explicit unittest target | `ARCHIVED_EVALUATION` | Retain with archived `run_manual_sessions.py` | Archived together so the old manual-session flow remains inspectable |
| `test_simulate.py` | test | Regression/unit test for simulation helpers and CLIs | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_validate_phase1_spec.py` | test | Regression/unit test for Phase 1 artifact validator | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_validate_phase1b_results.py` | test | Regression/unit test for Phase 1B result validator | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `archive/evaluation/phase1_lrs/validate_phase1_spec.py` | evaluation / analysis | Validate Phase 1 decision/spec/fixture artifacts | Archived reference; tested through moved import path | Covered by `test_validate_phase1_spec.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed artifact validator | Protects Phase 1 artifact contract |
| `archive/evaluation/phase1_lrs/validate_phase1b_results.py` | evaluation / analysis | Validate Phase 1B evaluation outputs and findings | Archived reference; reproduction docs point to archive path | Covered by `test_validate_phase1b_results.py` | `ARCHIVED_EVALUATION` | Retain under archive as completed artifact validator | Protects published evaluation outputs |

## 3. KEEP: current implementation

Current implementation modules stay in `KEEP` when they satisfy at least one of these:

- they are imported by the active prototype CLI or SRS runtime
- they are cited by current spec documents as the implementation path
- they are exercised by the current unittest suites as behavior under maintenance

This bucket includes:

- Phase 1 / integrated runtime modules such as `engine.py`, `display.py`, `event_format.py`, and `hud.py`
- SRS runtime modules such as `srs/contracts.py`, `srs/engine.py`, `srs/generate.py`, `srs/encounter.py`, `srs/model.py`, `srs/log.py`, and `srs/render.py`
- test-only support like `display_reference.py`, because it is still the active snapshot oracle for current regression tests

## 4. KEEP: current tests and regression support

Active `test_*.py` files outside `archive/` are currently in `KEEP`.

Reasoning:

- both `python -m unittest discover experiments/galactic_exodus` and `python -m unittest discover experiments/galactic_exodus/srs` rely on this active layout
- many tests cover operator tools and artifact validators, not only runtime modules
- several specs and evaluation docs rely on these tests as the executable proof that current prototype behavior and artifact contracts still hold
- `archive/evaluation/phase1_lrs/test_play.py` moved with `play.py` so that archived Phase 1 LRS CLI behavior remains inspectable with the archived CLI

## 5. KEEP_AS_TOOL: manual CLI / development tools

Files in `KEEP_AS_TOOL` are still worth retaining even when they are not foundational import targets:

- `integrated_play.py`
- `replay_phase1_reference.py`
- `srs/run_fixture.py`
- `srs/run_manual_eval.py`

These are the current human-facing or operator-facing entrypoints for manual play, fixture replay, or fixture review.

`play.py` was removed from this bucket in #1340 because the standalone Phase 1 LRS-only CLI is no longer the current README entrypoint. It moved to `archive/evaluation/phase1_lrs/play.py` with `test_play.py`.

`run_manual_sessions.py` was also removed from this bucket because it is coupled to the archived Phase 1 LRS-only CLI and completed manual-session evaluation workflow. It moved to `archive/evaluation/manual_sessions/run_manual_sessions.py` with `test_run_manual_sessions.py`.

## 6. EVALUATION_SUPPORT

The only active file still classified as `EVALUATION_SUPPORT` is:

- `simulate.py`

It remains active because `engine.py` imports it directly. The completed Phase 1, SRS, and manual-session evaluation scripts moved to `archive/evaluation/` and are now classified as `ARCHIVED_EVALUATION`.

## 7. ARCHIVED_EVALUATION / ARCHIVE_CANDIDATE

`LEGACY_REFERENCE` and `ARCHIVE_CANDIDATE` are not used after #1340.

`ARCHIVED_EVALUATION` contains completed evaluation support scripts kept for past evaluation reproduction and reference. This includes:

- Phase 1 LRS evaluation scripts under `archive/evaluation/phase1_lrs/`
- SRS evaluation scripts under `archive/evaluation/srs/`
- manual-session runner, recovery, and validation scripts under `archive/evaluation/manual_sessions/`

Why these files were not placed in `DELETE_CANDIDATE`:

- they still document how past evaluation reports and artifacts were produced or validated
- `create_manual_sessions_csv.py` still provides a plausible recovery path for older manual logs
- deleting them now would remove historical reproduction context without reducing current runtime surface

Why these files are not current runtime or operator tools:

- `integrated_play.py` is the current integrated CLI entrypoint
- `srs/run_manual_eval.py` and `srs/run_fixture.py` remain active operator or fixture tools
- `run_manual_sessions.py` moved to archive because it depends on the archived Phase 1 LRS-only CLI
- archived validators and batch evaluators are historical evidence support, not current gameplay specifications

## 8. DELETE_CANDIDATE

None in this audit.

Rationale:

- the tree does not currently contain an obvious cluster of orphaned Python files
- even lightly referenced files are usually tied to evaluation reproducibility, artifact validation, or current unittest coverage
- this issue intentionally does not delete anything; stronger deletion confidence should come from a follow-up that rechecks docs, results workflows, and any remaining manual operator knowledge

## 9. UNKNOWN / needs follow-up

None in this audit.

The only previous borderline file was `create_manual_sessions_csv.py`; it is now archived rather than unknown because its role is understandable from the code and surrounding CSV/log workflow.

## 10. Recommended follow-up issues

1. If `simulate.py` should be archived later, first split the current map generation dependency used by `engine.py` into an active implementation module.
2. If manual playtest workflows are restarted, decide whether to build them around `integrated_play.py` or restore a new active runner instead of reusing the archived Phase 1 LRS-only flow.
3. Consider whether artifact validators should be grouped under a clearer subdirectory in a later refactor, without changing behavior in this inventory-only PR.
4. If future cleanup is desired, re-audit generated results and non-Python operator assets together with this inventory so delete/archive decisions are made with the full workflow in view.
