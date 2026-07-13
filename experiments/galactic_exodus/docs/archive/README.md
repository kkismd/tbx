# Galactic Exodus archived documents

## Authority

- archive配下は履歴資料であり current source ではない
- gameplay specification の CURRENT_SOURCE は `../specs/`
- active implementation plan として使用しない
- archive と current spec が異なる場合は `docs/specs` を優先する

## Phase 1

### `phase1_spec.md`

- former role: `Phase 1` TBX移植仕様の統合正本
- former path: `experiments/galactic_exodus/phase1_spec.md`
- related issue / PR: #1058 / #1323 / #1318
- superseded by: `../specs/phase1.md`
- status: archived
- reason retained: `Phase 1` の統合仕様と履歴 appendix を分離した経緯を保存するため

### `phase1_reference_fixture_plan.md`

- former role: PR #1077 向け reference fixture 実装計画
- related issue / PR: #1059 / #1077
- status: implemented and archived
- reason retained: fixture注入・再生設計の実装経緯を保存するため
- current references: code / fixtures / tests

## Phase 2

### `phase2_srs_elements.md`

- former role: `SRS` terrain / object / actor 要素仕様
- former path: `experiments/galactic_exodus/srs/phase2_srs_elements.md`
- related issue / PR: #1085 / #1086 / #1321 / #1318
- superseded by: `../specs/srs_map_generation.md`, `../specs/srs_objects.md`, `../specs/srs_combat.md`
- status: archived
- reason retained: `Phase 2` の要素体系と current docs へ分割する前の前提を履歴として保存するため

### `phase2_srs_spec.md`

- former role: `Phase 2` SRS統合仕様
- former path: `experiments/galactic_exodus/srs/phase2_srs_spec.md`
- related issue / PR: #1178 / #1194 / #1319 / #1320 / #1321 / #1322 / #1318
- superseded by: `../specs/srs_map_generation.md`, `../specs/srs_movement.md`, `../specs/srs_objects.md`, `../specs/srs_warp.md`, `../specs/srs_combat.md`, `../specs/srs_encounter.md`, `../specs/display.md`
- status: archived
- reason retained: split migration 前の統合仕様と section 単位の判断履歴を保持するため

### `phase2_srs_movement.md`

- former role: `SRS` movement解決仕様
- former path: `experiments/galactic_exodus/srs/phase2_srs_movement.md`
- related issue / PR: #1089 / #1321 / #1318
- superseded by: `../specs/srs_movement.md`, `../specs/srs_warp.md`
- status: archived
- reason retained: baseline 比較候補と移動解決規則の旧整理を参照できるようにするため

### `phase2_initial_model.md`

- former role: `Phase 2` 初期データモデル・設計前提
- former path: `experiments/galactic_exodus/srs/phase2_initial_model.md`
- related issue / PR: #1080 / #1321 / #1318
- superseded by: `../specs/README.md`, `../specs/srs_map_generation.md`, `../specs/srs_movement.md`, `../specs/srs_warp.md`
- status: archived
- reason retained: evaluation-stage 前提と初期比較条件の由来を失わないため

### `phase2_srs_generation.md`

- former role: `SRS` generation設計
- former path: `experiments/galactic_exodus/srs/phase2_srs_generation.md`
- related issue / PR: #1088 / #1321 / #1318
- superseded by: `../specs/srs_map_generation.md`, `../specs/srs_warp.md`, `../specs/srs_encounter.md`
- status: archived
- reason retained: full terrain-count profile と retry/report schema を含む旧生成設計を履歴として保持するため

## Retention policy

- 完了済みで履歴価値のある設計・実装計画を保持する
- 未完了のactive planはarchiveへ置かない
- archive文書を仕様正本として参照しない
- 削除判断は重複性と履歴価値を別途確認して行う
