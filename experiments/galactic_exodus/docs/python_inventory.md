# Galactic Exodus Python file inventory

Source issue: #1336
Parent: #1309
Related: #1308, #1313, #1317, #1318
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

Classification summary:

| Classification | Count | Notes |
|---|---:|---|
| `KEEP` | 55 | Current implementation modules plus current regression tests/support |
| `KEEP_AS_TOOL` | 6 | Manual CLIs and fixture runners that are still useful even when not imported as core modules |
| `EVALUATION_SUPPORT` | 15 | Evaluation batch runners, metrics helpers, and artifact validators |
| `LEGACY_REFERENCE` | 0 | No file needed this label in the current tree |
| `ARCHIVE_CANDIDATE` | 1 | Old helper likely superseded by a newer workflow |
| `DELETE_CANDIDATE` | 0 | No file was marked safe-to-delete in this audit |
| `UNKNOWN` | 0 | No file required additional ownership clarification to classify |

## 2. Inventory table

| File | Kind | Role | Referenced by | Test / command coverage | Classification | Recommended action | Notes |
|---|---|---|---|---|---|---|---|
| `create_manual_sessions_csv.py` | generated support | Bootstrap blank manual-session CSV rows from JSON logs | No current README/docs command; no in-repo imports or tests | Not covered by unittest discover beyond import smoke | `ARCHIVE_CANDIDATE` | Keep only if legacy CSV bootstrap from raw logs is still needed | Superseded in practice by `run_manual_sessions.py`, but may help reconstruct old logs |
| `display.py` | implementation | Render current Phase 1 / integrated map output | Imported by `play.py` and `integrated_play.py`; cited by specs | Covered by `test_display.py` and snapshot tests | `KEEP` | Retain as active rendering module | Current CLI rendering path |
| `display_reference.py` | generated support | Build snapshot reference states and expected render strings for tests | Imported by display snapshot tests | Covered by `test_display.py`, `test_display_snapshot.py`, `srs/test_render.py`, `srs/test_display_snapshot.py` | `KEEP` | Retain as active regression support | Current snapshot oracle support, not dead legacy |
| `engine.py` | implementation | Phase 1 non-interactive engine and log generation | Imported by `play.py`, evaluation scripts, tests, and docs | Covered by `test_engine.py` and dependent tests | `KEEP` | Retain as active core implementation | Foundation for Phase 1 prototype behavior |
| `evaluate_manual_sessions.py` | evaluation / analysis | Validate `prototype_manual_sessions.csv` against JSON logs | Used by `test_evaluate_manual_sessions.py`; results consumed by evaluation docs | Covered by `test_evaluate_manual_sessions.py` | `EVALUATION_SUPPORT` | Retain as artifact-contract validator | Protects manual-eval CSV integrity |
| `evaluate_policies.py` | evaluation / analysis | Run deterministic Phase 1 policy batches and emit CSV/summary | Documented in README and `phase1_prototype_playtest.md`; verified by tests | Covered by `test_evaluate_policies.py` | `EVALUATION_SUPPORT` | Retain as evaluation reproduction entrypoint | Needed to reproduce Phase 1B automated evidence |
| `event_format.py` | implementation | Format prototype events for display output | Imported by `play.py` and `integrated_play.py`; cited by display spec | Covered by `test_event_format.py` | `KEEP` | Retain as active formatting layer | Current display/event contract |
| `fuel_metrics.py` | evaluation / analysis | Compare Phase 1 fuel/resource tuning across seed ranges | Documented in README and evaluation reports | Covered by `test_fuel_metrics.py` and CLI tests in `test_simulate.py` | `EVALUATION_SUPPORT` | Retain as evaluation reproduction tool | Generates published fuel comparison evidence |
| `hud.py` | implementation | Render compact HUD/status summaries | Imported by `play.py`, `integrated_play.py`, `srs/run_manual_eval.py` | Covered by `test_hud.py` and CLI tests | `KEEP` | Retain as active UI support | Shared display helper |
| `integrated_play.py` | CLI / manual tool | Interactive integrated LRS/SRS command-response prototype | Documented in README and integrated CLI spec; covered by dedicated tests | Covered by `test_integrated_play.py` | `KEEP_AS_TOOL` | Retain as current manual CLI entrypoint | Not imported as library core, but active operator-facing tool |
| `metrics.py` | evaluation / analysis | Aggregate Phase 1 map metrics over seed ranges | Documented in README; imported by `fuel_metrics.py` and policy evaluation scripts | Covered by `test_metrics.py` and CLI tests in `test_simulate.py` | `EVALUATION_SUPPORT` | Retain as evaluation support module | Feeds multiple reports and tuning scripts |
| `play.py` | CLI / manual tool | Interactive Phase 1 LRS prototype CLI | Documented in README and referenced by manual-session pipeline | Covered by `test_play.py` | `KEEP_AS_TOOL` | Retain as current manual CLI entrypoint | Primary prototype play surface |
| `replay_phase1_reference.py` | fixture runner | Replay `phase1_reference.json` against current engine | Documented in README and archive plan; used by fixture test | Covered by `test_phase1_reference_fixtures.py` | `KEEP_AS_TOOL` | Retain as manual regression runner | Useful when checking Phase 1 reference fixture drift |
| `run_manual_sessions.py` | CLI / manual tool | Run interactive manual sessions and append scored feedback CSV rows | Covered by dedicated tests; output referenced by evaluation docs | Covered by `test_run_manual_sessions.py` | `KEEP_AS_TOOL` | Retain as manual playtest operator tool | Current manual-evaluation workflow |
| `simulate.py` | evaluation / analysis | Deterministic Phase 1 map generation sample and CLI helpers | Documented in README; imported by engine/evaluation stack | Covered by `test_simulate.py` | `EVALUATION_SUPPORT` | Retain as active generation/evaluation support | Shared low-level helper for prototype analyses |
| `srs/__init__.py` | implementation | Expose SRS package convenience exports | Imported by `integrated_play.py` and tests via package API | Covered indirectly by unittest discovery and `run_fixture` callers | `KEEP` | Retain as active package surface | Current package entrypoint |
| `srs/contracts.py` | implementation | Define/load SRS contracts and shared constants | Imported throughout SRS engine/generate/evaluate/test paths | Covered by `srs/test_contracts.py` and wider SRS suite | `KEEP` | Retain as active SRS core module | Central contract boundary |
| `srs/encounter.py` | implementation | Compute SRS encounter rules and rolls | Imported by `srs/run_fixture.py` and referenced by specs/tests | Covered by `srs/test_encounter.py` and `srs/test_encounter_balance.py` | `KEEP` | Retain as active SRS core module | Current encounter implementation |
| `srs/engine.py` | implementation | Apply SRS commands and state transitions | Imported by fixture runner, evaluation scripts, `integrated_play.py`, and tests | Covered by combat/movement/interaction/warp suites | `KEEP` | Retain as active SRS core module | Current SRS state machine |
| `srs/evaluate_policies.py` | evaluation / analysis | Run deterministic SRS policy batches and emit reports | Referenced by Phase 2 evaluation docs and verified by tests | Covered by `srs/test_evaluate_policies.py` | `EVALUATION_SUPPORT` | Retain as evaluation reproduction entrypoint | Needed for published SRS policy evidence |
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
| `srs/validate_phase2_decisions.py` | evaluation / analysis | Validate `phase2_decisions.csv` artifact structure | Used by dedicated validator test | Covered by `srs/test_validate_phase2_decisions.py` | `EVALUATION_SUPPORT` | Retain as artifact validator | Protects decision-log contract |
| `srs/validate_phase2_initial_model.py` | evaluation / analysis | Validate initial-model markdown/json artifacts | Used by dedicated validator test | Covered by `srs/test_validate_phase2_initial_model.py` | `EVALUATION_SUPPORT` | Retain as artifact validator | Protects archived/current Phase 2 model evidence |
| `srs/validate_phase2_results.py` | evaluation / analysis | Validate Phase 2 reference fixture/results contracts | Used by dedicated validator test and archive docs | Covered by `srs/test_validate_phase2_results.py` | `EVALUATION_SUPPORT` | Retain as artifact validator | Checks replayable SRS evidence |
| `srs/validate_phase2_srs_elements.py` | evaluation / analysis | Validate terrain/object placement artifact contracts | Used by dedicated validator test | Covered by `srs/test_validate_phase2_srs_elements.py` | `EVALUATION_SUPPORT` | Retain as artifact validator | Protects SRS element artifact consistency |
| `srs/validate_phase2_srs_generation.py` | evaluation / analysis | Validate generation artifact contracts and summaries | Used by dedicated validator test | Covered by `srs/test_validate_phase2_srs_generation.py` | `EVALUATION_SUPPORT` | Retain as artifact validator | Protects generation evidence artifacts |
| `srs/validate_phase2_srs_movement.py` | evaluation / analysis | Validate movement artifact contracts | Used by dedicated validator test | Covered by `srs/test_validate_phase2_srs_movement.py` | `EVALUATION_SUPPORT` | Retain as artifact validator | Protects movement evidence artifacts |
| `srs/validate_phase2_srs_spec.py` | evaluation / analysis | Validate Phase 2 SRS spec cross-reference contract | Used by dedicated validator test | Covered by `srs/test_validate_phase2_srs_spec.py` | `EVALUATION_SUPPORT` | Retain as artifact validator | Spec consistency checker |
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
| `test_play.py` | test | Regression/unit test for Phase 1 CLI behavior | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_run_manual_sessions.py` | test | Regression/unit test for manual-session runner | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_simulate.py` | test | Regression/unit test for simulation helpers and CLIs | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_validate_phase1_spec.py` | test | Regression/unit test for Phase 1 artifact validator | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `test_validate_phase1b_results.py` | test | Regression/unit test for Phase 1B result validator | Unittest discovery under `experiments/galactic_exodus` | Directly executed by `python -m unittest discover experiments/galactic_exodus` | `KEEP` | Retain as current regression coverage | Current automated test coverage |
| `validate_phase1_spec.py` | evaluation / analysis | Validate Phase 1 decision/spec/fixture artifacts | Used by dedicated validator test and fixture replay script | Covered by `test_validate_phase1_spec.py` | `EVALUATION_SUPPORT` | Retain as artifact validator | Protects Phase 1 artifact contract |
| `validate_phase1b_results.py` | evaluation / analysis | Validate Phase 1B evaluation outputs and findings | Documented in `phase1_prototype_playtest.md`; covered by dedicated test | Covered by `test_validate_phase1b_results.py` | `EVALUATION_SUPPORT` | Retain as artifact validator | Protects published evaluation outputs |

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

All `test_*.py` files are currently in `KEEP`.

Reasoning:

- both `python -m unittest discover experiments/galactic_exodus` and `python -m unittest discover experiments/galactic_exodus/srs` rely on this layout
- many tests cover operator tools and artifact validators, not only runtime modules
- several specs and evaluation docs rely on these tests as the executable proof that current prototype behavior and artifact contracts still hold

## 5. KEEP_AS_TOOL: manual CLI / development tools

Files in `KEEP_AS_TOOL` are still worth retaining even when they are not foundational import targets:

- `play.py`
- `integrated_play.py`
- `replay_phase1_reference.py`
- `run_manual_sessions.py`
- `srs/run_fixture.py`
- `srs/run_manual_eval.py`

These are the current human-facing or operator-facing entrypoints for manual play, fixture replay, or fixture review.

## 6. EVALUATION_SUPPORT

Files in `EVALUATION_SUPPORT` mainly exist to produce or validate evidence:

- Phase 1 batch evaluators and metrics scripts:
  - `evaluate_policies.py`
  - `metrics.py`
  - `fuel_metrics.py`
  - `simulate.py`
- manual-evaluation and artifact validators:
  - `evaluate_manual_sessions.py`
  - `validate_phase1_spec.py`
  - `validate_phase1b_results.py`
- Phase 2/SRS evaluators and validators:
  - `srs/evaluate_policies.py`
  - `srs/validate_phase2_*.py`

These files are not gameplay specification authorities, but they are still part of the reproducibility chain for reports, findings, fixtures, and spec-alignment checks.

## 7. LEGACY_REFERENCE / ARCHIVE_CANDIDATE

`LEGACY_REFERENCE` was not needed for any current file.

`ARCHIVE_CANDIDATE`:

- `create_manual_sessions_csv.py`

Why it was not placed in `DELETE_CANDIDATE`:

- it still has a coherent purpose: rebuilding scaffold CSV rows from stored JSON logs
- its output format matches the manual-session CSV contract
- deleting it now would remove a plausible recovery path for older manual logs

Why it was not placed in `KEEP_AS_TOOL`:

- the current workflow is already covered by `run_manual_sessions.py`, which both launches play sessions and persists feedback
- no current README or evaluation document points to `create_manual_sessions_csv.py` as an expected operator command
- there is no dedicated regression test anchoring it as an actively maintained entrypoint

## 8. DELETE_CANDIDATE

None in this audit.

Rationale:

- the tree does not currently contain an obvious cluster of orphaned Python files
- even lightly referenced files are usually tied to evaluation reproducibility, artifact validation, or current unittest coverage
- this issue intentionally does not delete anything; stronger deletion confidence should come from a follow-up that rechecks docs, results workflows, and any remaining manual operator knowledge

## 9. UNKNOWN / needs follow-up

None in this audit.

The only borderline file was `create_manual_sessions_csv.py`, and it was classified as `ARCHIVE_CANDIDATE` rather than `UNKNOWN` because its role is understandable from the code and surrounding CSV/log workflow.

## 10. Recommended follow-up issues

1. Decide whether `create_manual_sessions_csv.py` should be archived, documented as a recovery tool, or removed after confirming that `run_manual_sessions.py` fully replaced it.
2. If manual playtest workflows are still expected, add explicit operator documentation for `run_manual_sessions.py` and `evaluate_manual_sessions.py` in `experiments/galactic_exodus/README.md` or an evaluation operations note.
3. Consider whether artifact validators should be grouped under a clearer subdirectory in a later refactor, without changing behavior in this inventory-only PR.
4. If future cleanup is desired, re-audit generated results and non-Python operator assets together with this inventory so delete/archive decisions are made with the full workflow in view.
