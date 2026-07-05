## 概要

Closes #1226

SRS の internal lower-left 座標契約に合わせて、移動方向の `N` / `S` を同期しました。
あわせて、同じ方向契約を重複実装していた policy 側の step 計算、fixture、reference、関連テストを更新しました。

## 変更内容

- `engine.py` の `_step_position()` を `N = y + 1`, `S = y - 1` に修正
- `evaluate_policies.py` の重複 step 実装と y 優先 tie-break を lower-left 契約へ同期
- movement / warp / encounter / render / manual eval / policy 系の Python テスト期待値を新座標系へ更新
- `move_route_basic_9x9`、`turn_only_cost_9x9`、`shared_fuel_cost_9x9`、`nebula_observation_3x3_9x9`、`warp_exit_s_9x9` などの fixture expected を同期
- `turn_only_cost_9x9` / `shared_fuel_cost_9x9` / `move_to_known_9x9` は通路上 object を fixture override で明示的に除去し、経路期待を deterministic に固定
- `run_fixture.py` の `cell_overrides` に `object_id: null` を追加し、fixture から既存 object を明示的に除去できるようにした
- `phase2_reference.json` と `phase2_srs_spec.md` の required text / reference を更新

## 確認

- `python experiments/galactic_exodus/srs/validate_phase2_results.py experiments/galactic_exodus/srs/fixtures/phase2_reference.json`
- `python -m unittest discover experiments/galactic_exodus/srs`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
