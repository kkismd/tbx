# Galactic Exodus LRS gameplay仕様

Source issue: #1323
Parent issue: #1314
Depends on: #1313, #1314, #1317, #1321, #1322
Related: #1040, #1047, #1058, #1076, #1259, #1307, #1318, #1350, #1353, #1354
Base branch: `integration/882-galactic-exodus`

この文書は、Galactic Exodus における LRS gameplay contract の `CURRENT_SOURCE` である。

- 対象は `experiments/galactic_exodus/docs/archive/phase1_spec.md` の `## 1-10` に含まれていた current gameplay / UI / logging / fixture contract である
- `phase1_spec.md` の `## 11-14` は current gameplay 本文へ移植せず、履歴情報として分離する
- 実装、fixture、tests、evaluation reports、decision register は根拠と回帰面であり、競合する正本ではない
- 新しい gameplay rule、数値、field、event type、fixture 名、UI behavior は追加しない

## 1. 文書の位置付けと正本性

この文書は、LRS gameplay contract、LRS UI meaning contract、`GameLog` schema v3、reference fixture schema v1 を固定する `CURRENT_SOURCE` である。

この文書が正本として扱う範囲:

- LRS board / coordinate / sector symbol
- `actual_map` / `known_cells` / `visited_cells` / `known_routes`
- LRS movement resolution
- board edge / `OPEN` edge / actual `RIFT` edge
- fuel / `BASE` / `RESOURCE`
- observation
- game status
- LRS UI meaning contract
- `GameLog` schema
- reference fixture schema

authority 優先順位:

1. merged decision issue と `experiments/galactic_exodus/docs/specs/` 配下の current docs
2. current implementation / tests / fixtures
3. `experiments/galactic_exodus/docs/archive/phase1_spec.md` の `## 1-10`
4. evaluation reports
5. `experiments/galactic_exodus/docs/archive/phase1_spec.md` の `## 11-14`

参照根拠:

- 実装: `experiments/galactic_exodus/engine.py`, `experiments/galactic_exodus/display.py`, `experiments/galactic_exodus/hud.py`, `experiments/galactic_exodus/event_format.py`
- archive reference: `experiments/galactic_exodus/archive/evaluation/phase1_lrs/play.py`, `experiments/galactic_exodus/archive/evaluation/phase1_lrs/evaluate_policies.py`
- fixture / validator: `experiments/galactic_exodus/fixtures/phase1_reference.json`, `experiments/galactic_exodus/archive/evaluation/phase1_lrs/validate_phase1_spec.py`
- decision register: `experiments/galactic_exodus/phase1_decisions.csv`
- evaluation evidence: `experiments/galactic_exodus/docs/evaluations/phase1_prototype_playtest.md`, `experiments/galactic_exodus/docs/evaluations/phase1_fuel_comparison_low_initial_seed_1_1000.md`, `experiments/galactic_exodus/docs/evaluations/phase1_fuel_comparison_seed_1_1000.md`

`GameLog` schema version は `3`、reference fixture schema version は `1` である。これらは別の version であり、同一意味ではない。

## 2. 対象範囲と SRS / integrated仕様との責務境界

この文書が固定するもの:

- fixed 8x8 LRS board
- `known_cells` / `visited_cells` / `known_routes` の意味
- `N` / `E` / `S` / `W` command の正規化と移動解決
- `RIFT` の未知試行と既知再試行の境界
- fuel、`BASE`、`RESOURCE`、win / `LOST_FUEL` / abort
- seed reroll と fixture injection 方針
- LRS normal UI に要求する意味契約
- `GameLog` schema v3 と reference fixture schema v1

他仕様へ委譲するもの:

- current display wording / glyph / renderer behavior の詳細: [`display.md`](display.md)
- integrated command loop や `EXIT` / `INTERACT` を含む LRS-SRS 統合挙動: [`integrated_cli.md`](integrated_cli.md)
- SRS movement: [`srs_movement.md`](srs_movement.md)
- SRS map / warp / object / combat / encounter: [`srs_map_generation.md`](srs_map_generation.md), [`srs_warp.md`](srs_warp.md), [`srs_objects.md`](srs_objects.md), [`srs_combat.md`](srs_combat.md), [`srs_encounter.md`](srs_encounter.md)

non-scope:

- gameplay 実装変更
- schema version 変更
- fixture / snapshot / evaluation result の再生成
- LRS gameplay 数値の再調整
- SRS / integrated 仕様の再記述
- archive 化された legacy 文書本文の現行仕様化

## 3. schema version と参照入力

current implementation が持つ外部契約:

- `GameLog.schema_version == 3`
- `fixtures/phase1_reference.json` root `schema_version == 1`
- `fixtures/phase1_reference.json` root `game_log_schema_version == 3`

LRS gameplay で参照する入力:

- decision register: `experiments/galactic_exodus/phase1_decisions.csv`
- fixture validator: `experiments/galactic_exodus/archive/evaluation/phase1_lrs/validate_phase1_spec.py`
- replay fixture: `experiments/galactic_exodus/fixtures/phase1_reference.json`

この文書は field 名や scenario 名を current implementation と同じ token で記述する。日本語 alias や新しい英語 token は追加しない。

## 4. 盤面・座標・開始地点・目的地

盤面 contract:

- board size は固定 `8x8`
- 座標は lower-left origin の `1-origin`
- `x` は eastward に増加し、`y` は northward に増加する
- `start_position` は `(1,1)`
- `goal_position` は `(8,8)`

cell symbol contract:

- `S` は `start_position` にのみ存在する
- `H` は `goal_position` にのみ存在する
- `B` は base cell
- `R` は unused resource cell
- `.`、`N`、`A`、`@` は current implementation の terrain symbol として扱う

## 5. `known_cells` / `visited_cells` / `known_routes`

known-state contract:

- `known_cells`: player に開示済みの cell symbol を保持する
- `visited_cells`: 実際に到着した座標を保持する
- `known_routes`: player が既知にした edge を保持する

`known_routes` state:

- successful move で通過した edge は `OPEN`
- unknown blocked edge へ試行して失敗した edge は `RIFT`

secrecy contract:

- `actual_map` 全体はゲーム内部だけが保持する
- normal UI、command hint、help、通常の入力拒否メッセージは未発見 `RIFT` や未観測 cell を漏らしてはならない
- `LOST_FUEL` 判定のために `actual_map` を参照してよいが、その理由を通常 UI の行動候補として漏らしてはならない

## 6. 開始時・移動後の観測

開始時 observation:

- `H` はゲーム開始時から既知である
- `start_position` 周囲の `3x3` 範囲を開始時に開示する
- 開示対象には地形、`B`、`R`、`S`、`H` を含む

移動後 observation:

- successful move 後のみ、新しい `player_position` 周囲の `3x3` 範囲を累積開示する
- failed move、rejected move、invalid command では追加開示しない
- `RIFT` は通常 observation では見えず、航行試行により初めて既知になる

## 7. command 入力と無効・拒否入力

input contract:

- 有効な movement command は `N`、`E`、`S`、`W` のみ
- 入力は前後空白を除去し、大文字へ正規化する

invalid / rejected contract:

- invalid command は `INVALID_COMMAND`
- board 外への移動は `OUT_OF_BOUNDS`
- 既知 `RIFT` 再試行は `REJECTED_KNOWN_RIFT`
- fuel 不足は `REJECTED_INSUFFICIENT_FUEL`

これらの結果では次を変更しない:

- `player_position`
- `remaining_fuel`
- `turn_count`
- `known_cells`
- `visited_cells`
- `known_routes`

ただし `invalid_or_rejected_action_count` は 1 増やす。

## 8. `OPEN` route / unknown `RIFT` / known `RIFT`

successful move:

- edge を `OPEN` として `known_routes` へ記録する

unknown `RIFT` attempt:

- outcome は `BLOCKED_UNKNOWN_RIFT`
- 移動は失敗し、位置は変化しない
- fuel を 1 消費する
- `turn_count` を 1 増やす
- 該当 edge を `RIFT` として `known_routes` に追加する
- `discovered_rift` は `true`
- `rift_attempt_count` を 1 増やす

known `RIFT` retry:

- 実行前に拒否する
- fuel / turn を消費しない
- `discovered_rift` は `false`
- `invalid_or_rejected_action_count` を 1 増やす

## 8.5. LRS sector間edge topology

actual edge contract:

- sector間edgeの actual 状態は LRS `actual_map` が正本として保持する
- board 外方向には destination sector が存在せず、通過不可である
- actual `RIFT` edge は双方向通過不能である
- board 内かつ actual `RIFT` でない edge は通過可能である
- source から `dir` へ通過可能なら、destination から `opposite(dir)` へも同じ edge として通過可能である
- `known_routes` は発見・表示状態であり、actual な通行可否の正本ではない
- 未発見 `RIFT` も `actual_map` 上では blocked である

責務境界:

- LRS gameplay は actual edge 状態を決定する
- integrated adapter は actual edge 状態を `SectorDescriptor` の3方向集合へ写像する
- SRS generation / WARP は3方向集合を terrain / `warp_flags` / `RIFT_BARRIER` へ表現する

この文書では LRS 側の actual edge contract を固定する。写像先の SRS contract 自体は [`srs_warp.md`](srs_warp.md) と [`srs_map_generation.md`](srs_map_generation.md) を優先する。

## 9. movement resolution order

successful move resolution order:

1. attempted destination を決定する
2. destination が board 内かを確認する
3. edge が `RIFT` 既知かを確認する
4. unknown `RIFT` なら 1 fuel を消費して失敗を確定する
5. destination cell の fuel cost を計算する
6. fuel 不足なら実行前に拒否する
7. fuel を消費する
8. `player_position` を更新する
9. `turn_count` を 1 増やす
10. edge を `OPEN` として記録する
11. `visited_cells` と `path` を更新する
12. 新しい `3x3` observation を開示する
13. 到着 cell の supply を適用する
14. `game_status` を確定する

## 10. fuel contract

fixed values:

- `initial_fuel = 16`
- `max_fuel = 16`

movement fuel contract:

- normal move は destination terrain の cost を支払う
- unknown `RIFT` attempt は固定で `1` fuel を支払う
- `required_fuel` は fuel 不足で reject したときのみ `TurnEvent` へ記録する

本仕様では上記数値を固定する。

## 11. `BASE` / `RESOURCE` supply contract

`BASE` contract:

- `B` 到着時に自動で `max_fuel` まで即時 refuel する
- 追加 turn を消費しない
- 利用回数制限を持たない
- `base_visit_count` は `B` 到着ごとに増やす
- 実際に fuel が増えたときだけ `base_refuel_count` を増やす
- 満タン到着では `last_supply_source` を変更しない

`RESOURCE` contract:

- board 上の `R` は `3` 個である
- 各 `resource_position` は 1 回だけ使用できる
- successful supply は `max +5` ではなく `max_fuel` を上限として最大 `+5` 回復する
- 追加 turn を消費しない
- `resource_visit_count` は `R` 到着ごとに増やす
- 実際に fuel が増えたときだけ `resource_refuel_count` を増やす
- 実際に fuel が増えたときだけ `used_resource_positions` に追加する
- 満タン到着では unused `R` を消費しない
- used `R` へ再訪しても追加補給しない

current supply result token:

- `BASE_REFUELED`
- `BASE_ALREADY_FULL`
- `RESOURCE_REFUELED`
- `RESOURCE_ALREADY_FULL`
- `RESOURCE_ALREADY_USED`
- `NONE`

## 12. win / `LOST_FUEL` / abort

win contract:

- successful move により `goal_position` へ到着したら `WON`
- 移動直後の `remaining_fuel == 0` でも、`H` 到着を先に評価して `WON` とする

`LOST_FUEL` contract:

- `H` にいない状態で、actual 隣接 4 方向に `RIFT` ではなく現在 fuel で cost を支払える edge が 1 本もない場合に `LOST_FUEL`
- `remaining_fuel == 0` でなくても成立しうる
- この判定は current implementation の `actual_map` を参照する

abort contract:

- generation error
- `ABORTED_TURN_LIMIT`
- command sequence exhaustion による `ABORTED_NO_POLICY_ACTION`
- policy non-progress / action generation failure が `ABORTED_NO_POLICY_ACTION` に集約される経路

## 13. generation reachability / seed / reroll

generation contract:

- requested seed を最初の candidate とする
- candidate map が `S` から `H` へ到達不能なら seed を `+1` して再試行する
- 最大 `100` candidate を試す
- 100 candidate すべてが不採用なら generation error

記録必須項目:

- `requested_seed`
- `effective_seed`
- `reroll_count`

overflow contract:

- candidate seed が signed `64-bit` 範囲を超える場合は generation error
- generation error は通常敗北と別に `generation_error` object として記録する

## 14. Python / TBX RNG と fixture injection

RNG policy:

- Python と TBX の通常生成において seed-to-map identity は必須ではない
- 決定的 seed 再現性は各実装内で維持する

comparison policy:

- Python / TBX 比較は fixture injection を用いる
- 同一 `actual_map` と同一 initial state を注入し、状態遷移を比較する
- `experiments/galactic_exodus/fixtures/phase1_reference.json` はそのための external contract である

## 15. LRS UI 入力・表示契約

この章は LRS normal UI に必要な意味契約のみを固定する。current renderer の具体 layout や wording は [`display.md`](display.md) を優先する。

LRS normal UI が区別すべき意味:

- unknown / known / current position / `S` / `H` / `B`
- unused `R` と used `R`
- discovered `RIFT`
- normal HUD と debug / log 情報の責務境界

current implementation との責務分担:

- 本仕様は「何を区別できる必要があるか」を固定する
- glyph、legend wording、layout、fallback 表現の詳細は `display.md` と current implementation に委譲する
- legacy の glyph `P`、Braille、色、端末幅別 layout を current requirement として再導入しない

## 16. `GameLog` schema v3

root required fields:

- `schema_version`
- `settings`
- `requested_seed`
- `effective_seed`
- `reroll_count`
- `initial_state`
- `events`
- `final_summary`
- `generation_error`

notes:

- normal run では `generation_error` は `null`
- generation error run では `effective_seed` / `reroll_count` / `initial_state` / `final_summary` が `null` になりうる

## 17. `TurnEvent` contract

`TurnEvent` required fields:

- `turn`
- `command`
- `outcome`
- `from_position`
- `attempted_position`
- `to_position`
- `fuel_before`
- `fuel_spent`
- `fuel_after`
- `required_fuel`
- `discovered_cells`
- `discovered_rift`
- `supply_result`
- `supply_source`
- `fuel_before_supply`
- `fuel_after_supply`
- `supply_amount`
- `status_after`

`outcome` token は current implementation の次に限定する:

- `MOVED`
- `BLOCKED_UNKNOWN_RIFT`
- `REJECTED_KNOWN_RIFT`
- `REJECTED_INSUFFICIENT_FUEL`
- `INVALID_COMMAND`
- `OUT_OF_BOUNDS`

## 18. `final_summary` / continuous metrics

`final_summary` required fields:

- `outcome`
- `turn_count`
- `remaining_fuel`
- `max_fuel`
- `used_resource_positions`
- `base_visit_count`
- `base_refuel_count`
- `resource_visit_count`
- `resource_refuel_count`
- `last_supply_source`
- `rift_attempts`
- `invalid_or_rejected_actions`
- `path`

この summary は evaluation scripts が継続利用する telemetry contract でもある。

## 19. Python / TBX 一致契約

一致必須:

- fixture 注入時の `actual_map`
- `requested_seed`
- `effective_seed`
- `reroll_count`
- initial known state
- 各 turn の `command`、`outcome`、position、fuel
- `known_cells`
- `visited_cells`
- `known_routes`
- `supply_result` / `supply_source` / `supply_amount`
- `turn_count`
- `game_status`
- final outcome

一致不要:

- 内部配列レイアウト
- 型名、関数名、補助変数名
- 一時キャッシュ
- layout、色、装飾、空白の非本質差分
- 通常生成時の Python / TBX seed-to-map 完全一致

## 20. reference fixture schema v1

file:

- `experiments/galactic_exodus/fixtures/phase1_reference.json`

root contract:

- root `schema_version == 1`
- root `game_log_schema_version == 3`
- `coordinate_format == "object_xy_1_based"`
- `edge_format == "object_from_to_lexicographically_sorted"`
- `map_format == "explicit_cells"`

fixture object contract:

- `name` は一意
- `mode` は `generated` / `injected` / `generation_error`
- `initial_actual_map` は `cells` / `rift_edges` / `base_position` / `resource_positions` を持つ
- `expected_turns` は `TurnEvent` schema と同じ field contract を使う
- `expected_final` は normal run では `final_summary` 対応、generation error run では `generation_error` 対応を持つ

mandatory scenario inventory:

- `no_reroll_initial_board`
- `reroll_requested_effective_seed`
- `normal_terrain_move`
- `unknown_rift_failure`
- `known_rift_retry`
- `base_supply`
- `resource_supply`
- `resource_second_visit_no_supply`
- `zero_fuel_goal_arrival_wins`
- `fuel_depletion_loss`
- `generation_error`
- `turn_limit_abort`

## 21. comparison / evaluation との役割分担

役割分担:

- `docs/specs/`: current gameplay specification
- `docs/evaluations/`: evaluation evidence、比較レポート、再現ノート
- `fixtures/phase1_reference.json`: cross-implementation state-transition comparison
- tests: current contract と regression の検証

evaluation documents は LRS gameplay の `CURRENT_SOURCE` ではない。仕様に競合がある場合はこの文書と current implementation / tests を優先する。

## 22. historical appendix

`experiments/galactic_exodus/docs/archive/phase1_spec.md` の `## 11-14` は current gameplay contract ではなく、次の履歴情報として扱う。

- `#1040` / `#1047` から継承した baseline note
- `#1058` 時点の変更履歴
- rejected alternatives
- historical implementation order

この appendix を current gameplay 本文へ混在させない理由:

- current contract と historical rationale の normative level が異なる
- historical implementation order は現在の未完了計画ではない
- rejected alternatives は現行仕様の必須ルールではない

履歴の詳細は `experiments/galactic_exodus/docs/archive/phase1_spec.md` と関連 issue を参照する。

## 23. deferred / non-scope

deferred:

- Braille、色、端末幅別 layout、screen-reader 向け代替表示などの詳細表示設計は `display.md` と SRS / integrated 系 issue へ委譲する
- integrated LRS/SRS command loop は `integrated_cli.md` へ委譲する

non-scope:

- gameplay 数値の再決定
- `GameLog` schema version の更新
- field / event type / payload key / fixture 名の rename
- archive 文書の再編集
