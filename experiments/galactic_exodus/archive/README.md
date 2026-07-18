# Galactic Exodus code archive

This archive keeps completed evaluation support scripts as historical reference.

Archived files are not current runtime modules, gameplay specification sources, or active operator-facing tools. They are retained so past evaluation flows can be inspected or reproduced without deleting their implementation history.

Current runtime and operator entrypoints remain outside this directory, including:

- `experiments/galactic_exodus/integrated_play.py`
- `experiments/galactic_exodus/run_manual_sessions.py`
- `experiments/galactic_exodus/srs/run_manual_eval.py`
- `experiments/galactic_exodus/srs/run_fixture.py`

Current implementation modules also remain outside this directory, including:

- `experiments/galactic_exodus/engine.py`
- `experiments/galactic_exodus/display.py`
- `experiments/galactic_exodus/event_format.py`
- `experiments/galactic_exodus/hud.py`
- `experiments/galactic_exodus/simulate.py`

`simulate.py` is intentionally not archived because `engine.py` imports it directly for current Phase 1 map generation behavior.

See [`evaluation/README.md`](evaluation/README.md) for the archived evaluation file mapping.
