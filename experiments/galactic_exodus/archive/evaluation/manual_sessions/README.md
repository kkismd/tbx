# Manual session evaluation archive

This directory contains Phase 1 manual-session evaluation support.

These scripts are retained for historical replay and recovery only. They are not current runtime modules, current gameplay specification sources, or active operator-facing tools.

| File | Role |
|---|---|
| `run_manual_sessions.py` | Run archived Phase 1 LRS `play.py`, collect per-seed feedback, and append `prototype_manual_sessions.csv` rows |
| `test_run_manual_sessions.py` | Regression tests for the archived manual-session runner |
| `evaluate_manual_sessions.py` | Validate historical `prototype_manual_sessions.csv` rows against GameLog v3 JSON logs |
| `create_manual_sessions_csv.py` | Rebuild scaffold CSV rows from stored JSON logs |

`run_manual_sessions.py` defaults to `experiments/galactic_exodus/archive/evaluation/phase1_lrs/play.py`, so it stays archived with the Phase 1 LRS-only CLI rather than remaining as an active operator tool.
