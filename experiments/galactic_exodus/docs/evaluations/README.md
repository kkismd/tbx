# Galactic Exodus evaluation documents

## Authority

- このディレクトリは評価結果・比較・観察・再現手順を保持する。
- gameplay specification の CURRENT_SOURCE は `../specs/` である。
- evaluation document と current specification が異なる場合、現行仕様は `docs/specs/` を参照する。
- 評価結果を仕様へ反映する場合は、対応する仕様 issue / PR を経由する。

## Phase 1

1. [`phase1_prototype_playtest.md`](phase1_prototype_playtest.md)
   - 対象: Phase 1B の統合プレイテスト
   - 文書の役割: 手動評価と自動評価を統合した評価レポート
   - 主な関連issue: #1067, #1068, #1059
2. [`phase1_fuel_comparison_low_initial_seed_1_1000.md`](phase1_fuel_comparison_low_initial_seed_1_1000.md)
   - 対象: 低 initial fuel 候補の比較
   - 文書の役割: 燃料・補給パラメータ比較の評価レポート
   - 主な関連issue: #1039, #1040
3. [`phase1_fuel_comparison_seed_1_1000.md`](phase1_fuel_comparison_seed_1_1000.md)
   - 対象: 高 initial fuel 候補の比較
   - 文書の役割: 燃料候補の baseline 比較レポート
   - 主な関連issue: #1040

## Phase 2

1. [`phase2_srs_playtest.md`](phase2_srs_playtest.md)
   - 対象: Phase 2 SRS の統合評価メモ
   - 文書の役割: 手動根拠と自動根拠を decision issue へ渡す評価メモ
   - 主な関連issue: #1162, #1081, #1163

## Reproduction policy

- Markdown は評価レポート本体である。
- CSV / JSON は既存 `results/` 配下の機械可読成果物である。
- Markdown の移動に伴って CSV / JSON は移動しない。
- 再生成時の Markdown 出力先は `docs/evaluations/` を使う。
