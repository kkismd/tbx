## 概要

Closes #1093

Phase 2 SRS の初期モデルと初期値を、`phase2_srs_elements.*` / `phase2_srs_generation.*` を正本とする参照型契約へ更新しました。

## 変更内容

- `phase2_initial_model.md` を全面整理し、旧 7x7 / `WALL` / `STATION_STRUCTURE` / `BASE_NODE` / feature ベースの warp 契約を削除
- `SrsCell.warp_flags` と `SectorDescriptor.generation_profile_ref` を中心にした初期モデルへ更新
- `phase2_initial_values.json` を `schema_version = 3` / `generation_schema_version = 1` へ更新
- `contract_references`、9x9 baseline、C1〜C8、persistent fields を新しい生成契約に合わせて更新
- 旧重複データを前提にしていた `validate_phase2_initial_model.py` と unit test を新契約へ追従

## 確認

- `python3 experiments/galactic_exodus/srs/validate_phase2_initial_model.py --model experiments/galactic_exodus/srs/phase2_initial_model.md --questions experiments/galactic_exodus/srs/phase2_questions.csv --values experiments/galactic_exodus/srs/phase2_initial_values.json`
- `python3 -m unittest experiments.galactic_exodus.srs.test_validate_phase2_initial_model`

## 補足

- `phase2_questions.csv` の更新は issue の非対象どおり含めていません
- `cargo` 系チェックは未実行です。変更範囲が `experiments/galactic_exodus/srs` 配下の文書・JSON・Python validator に限定されるためです
