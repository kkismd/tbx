# Evaluation support archive

This directory contains completed evaluation support scripts. They were archived after the Python inventory in #1336 and the follow-up decision in #1340.

The files are kept for past evaluation reproduction and reference only. They are not current runtime modules, current gameplay specification authorities, or active operator-facing tools.

## File mapping

| Original path | Archived path |
|---|---|
| `experiments/galactic_exodus/create_manual_sessions_csv.py` | `experiments/galactic_exodus/archive/evaluation/manual_sessions/create_manual_sessions_csv.py` |
| `experiments/galactic_exodus/evaluate_manual_sessions.py` | `experiments/galactic_exodus/archive/evaluation/manual_sessions/evaluate_manual_sessions.py` |
| `experiments/galactic_exodus/run_manual_sessions.py` | `experiments/galactic_exodus/archive/evaluation/manual_sessions/run_manual_sessions.py` |
| `experiments/galactic_exodus/test_run_manual_sessions.py` | `experiments/galactic_exodus/archive/evaluation/manual_sessions/test_run_manual_sessions.py` |
| `experiments/galactic_exodus/evaluate_policies.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/evaluate_policies.py` |
| `experiments/galactic_exodus/fuel_metrics.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/fuel_metrics.py` |
| `experiments/galactic_exodus/metrics.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/metrics.py` |
| `experiments/galactic_exodus/play.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/play.py` |
| `experiments/galactic_exodus/test_play.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/test_play.py` |
| `experiments/galactic_exodus/validate_phase1_spec.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/validate_phase1_spec.py` |
| `experiments/galactic_exodus/validate_phase1b_results.py` | `experiments/galactic_exodus/archive/evaluation/phase1_lrs/validate_phase1b_results.py` |
| `experiments/galactic_exodus/srs/evaluate_policies.py` | `experiments/galactic_exodus/archive/evaluation/srs/evaluate_policies.py` |
| `experiments/galactic_exodus/srs/validate_phase2_decisions.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_decisions.py` |
| `experiments/galactic_exodus/srs/validate_phase2_initial_model.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_initial_model.py` |
| `experiments/galactic_exodus/srs/validate_phase2_results.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_results.py` |
| `experiments/galactic_exodus/srs/validate_phase2_srs_elements.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_srs_elements.py` |
| `experiments/galactic_exodus/srs/validate_phase2_srs_generation.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_srs_generation.py` |
| `experiments/galactic_exodus/srs/validate_phase2_srs_movement.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_srs_movement.py` |
| `experiments/galactic_exodus/srs/validate_phase2_srs_spec.py` | `experiments/galactic_exodus/archive/evaluation/srs/validate_phase2_srs_spec.py` |

## Active exclusions

`simulate.py` stays in the active tree because it is imported directly by `engine.py`. Moving it would change the current runtime dependency graph and should be handled only by a dedicated refactor that separates map generation from evaluation helpers.

`integrated_play.py`, `srs/run_manual_eval.py`, and `srs/run_fixture.py` stay active because they are current operator or fixture entrypoints. `run_manual_sessions.py` moved into this archive because it is coupled to the archived Phase 1 LRS CLI and manual-session evaluation flow.
