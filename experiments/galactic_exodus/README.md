# Galactic Exodus Phase 0 実験環境

このディレクトリには、`Galactic Exodus` の Phase 0 における数理・ゲームデザイン検証用の小規模な Python 実験環境が含まれています。

これは正式な TBX アプリ実装ではありません。将来の TBX 側の実装は、`examples/galactic_exodus/` など別の場所に置く想定です。

## Phase 1A prototype engine

`engine.py` は、標準入力や画面表示に依存しない Phase 1A 用の非対話ゲームエンジンです。
マップ生成は既存の `simulate.generate_map()` をそのまま利用し、到達不能マップだけを deterministic に再抽選します。

公開 API:

```python
from experiments.galactic_exodus import engine

state = engine.create_game(42)
event = engine.apply_command(state, "E")
log = engine.run_commands(42, ["E", "N", "E"])
```

- `create_game(requested_seed, settings=DEFAULT_SETTINGS) -> GameState`
  - 到達可能な `S -> H` 経路を持つ候補が見つかるまで `requested_seed + attempt` を試す
  - `attempt` は `0..99`
  - `effective_seed` と `reroll_count` を state に保持する
- `apply_command(state, command) -> TurnEvent`
  - `state` をその場で更新する
  - `command` は `N / E / S / W`
  - 標準入力・標準出力は使わない
- `run_commands(requested_seed, commands, settings=DEFAULT_SETTINGS, max_turns=256) -> GameLog`
  - 非対話でコマンド列を最後まで実行する
  - 勝敗、turn limit、コマンド枯渇、generation error をログ化する

### state fields

`GameState` は次の状態を保持します。

- `actual_map`
- `known_cells`
- `visited_cells`
- `known_routes`
- `player_position`
- `remaining_fuel`
- `used_resource_positions`
- `base_visit_count`
- `base_refuel_count`
- `resource_visit_count`
- `resource_refuel_count`
- `last_supply_source`
- `turn_count`
- `game_status`
- `requested_seed`
- `effective_seed`
- `reroll_count`

開始時は `visited_cells = {S}`、`known_routes = {}` です。`known_cells` は `H` に加えて `S` を中心とした 3x3 範囲の盤面内セルで始まります。

観測ルール:

- 開始時に `S` を中心とした 3x3 範囲の盤面内セルを `known_cells` に追加します
- 成功移動後に、新しい現在地を中心とした 3x3 範囲の盤面内セルを `known_cells` に追加します
- 既知情報は累積し、離れても失われません
- 断層は周囲観測では開示されず、移動失敗時にだけ `known_routes` へ記録されます

### movement contract

- 成功移動: 移動先地形コストを消費し、`known_routes` に `OPEN` を記録する
- 成功移動: 移動後の現在地を中心とした周囲 8 マスの地形と `B` / `R` を開示する
- 未知断層への試行: 位置不変、`fuel -1`、`turn +1`、`known_routes` に `RIFT` を記録する
- 既知断層への再試行: 行動拒否、fuel/turn 不変
- 盤面外・無効コマンド・燃料不足: 状態不変
- `H` / `B` / `R` へ残量 0 ちょうどで到着してよい
- `B` は到着ごとに `max_fuel=16` まで即時補給する
- `B` は何度でも利用できる
- `R` は燃料が実際に増加した最初の到着時だけ消費される
- 満タンで未使用 `R` へ到着しても `R` は残る
- 使用済み `R` は再利用できない
- 補給は到着ターン内で即時適用され、追加 turn は発生しない

### turn outcomes

`TurnEvent.outcome` は次の固定値を使います。

- `MOVED`
- `BLOCKED_UNKNOWN_RIFT`
- `REJECTED_KNOWN_RIFT`
- `REJECTED_INSUFFICIENT_FUEL`
- `INVALID_COMMAND`
- `OUT_OF_BOUNDS`

### game status and final outcomes

`GameState.game_status`:

- `IN_PROGRESS`
- `WON`
- `LOST_FUEL`

`GameLog.final_summary.outcome`:

- `WON`
- `LOST_FUEL`
- `ABORTED_TURN_LIMIT`
- `ABORTED_NO_POLICY_ACTION`

generation error は通常敗北と分離し、`GameLog.generation_error` に記録します。
`last_supply_source` は `{"kind": "B"|"R", "position": {"x": ..., "y": ...}}` の構造で保持し、
直近に実際の燃料増加が起きた補給元座標を参照できます。

### deterministic log schema

`GameLog.schema_version` は `3` 固定です。

```text
GameLog
  schema_version
  settings
  requested_seed
  effective_seed
  reroll_count
  initial_state
  events
  final_summary
  generation_error
```

```text
TurnEvent
  turn
  command
  outcome
  from_position
  attempted_position
  to_position
  fuel_before
  fuel_spent
  fuel_after
  discovered_cells
  discovered_rift
  supply_result
  supply_source
  fuel_before_supply
  fuel_after_supply
  supply_amount
  status_after
```

```text
final_summary
  outcome
  turn_count
  remaining_fuel
  max_fuel
  used_resource_positions
  base_visit_count
  base_refuel_count
  resource_visit_count
  resource_refuel_count
  last_supply_source
  rift_attempts
  invalid_or_rejected_actions
  path
```

```text
generation_error
  kind
  requested_seed
  attempts
  last_candidate_seed
  reason
  message
```

`GameLog.to_dict()` と `GameLog.to_json()` は key 順と配列順を固定し、同一 seed・同一 command 列から同一 JSON を生成します。

## Phase 1A2 interactive CLI prototype

`play.py` は Phase 1A2 用の対話 CLI です。盤面生成、移動、燃料、補給、勝敗判定は `engine.py` に委譲し、CLI 側は入力・ASCII 表示・JSON ログ出力だけを担当します。

起動:

```bash
python experiments/galactic_exodus/play.py --seed 42
python experiments/galactic_exodus/play.py --seed 42 --json-log .tmp/galactic-exodus-log.json
```

- `--seed <int>`
  - 必須。requested seed を指定します。
- `--json-log <path>`
  - 任意。終了時に `GameLog schema_version=3` を JSON で書き出します。

コマンド:

```text
N
E
S
W
Q
```

- 大文字小文字を区別しません
- 前後空白を無視します
- `Q` と EOF は状態を変えずに正常終了します

盤面記号:

```text
?  未知セル
.  既知 plain
N  既知 nebula
A  既知 asteroid
@  既知 anomaly
S  start
H  goal
B  既知 base
R  既知 resource
P  現在地
```

- `H` は開始時から表示されます
- `S` 周囲 8 マスの地形と `B` / `R` は開始時から表示されます
- 未知の地形・B・R は表示しません
- 表示優先順位は `P > S/H > B/R > terrain > ?` です

状態表示:

```text
SEED: requested=<n> effective=<n> rerolls=<n>
POSITION: (<x>,<y>)
FUEL: <remaining>/<max>
LAST SUPPLY: none | B(<x>,<y>) | R(<x>,<y>)
USED R: - | (x1,y1),(x2,y2),...
TURN: <n>
STATUS: IN PROGRESS | WON | LOST FUEL
BLOCKED: <known-rift-directions or ->
```

ターン結果文言:

```text
MOVED TO (x,y), COST n
RIFT BLOCKED <direction>, COST 1
KNOWN RIFT <direction>
OUT OF BOUNDS
INVALID COMMAND
INSUFFICIENT FUEL: NEED n, HAVE m
REFUELED AT B(x,y): <before> -> <after>
BASE ALREADY FULL AT B(x,y)
REFUELED AT R(x,y): +<amount> (<before> -> <after>)
RESOURCE ALREADY FULL AT R(x,y)
RESOURCE ALREADY USED AT R(x,y)
YOU REACHED H
NO FURTHER MOVE IS POSSIBLE
```

fuel / 断層 / 補給 / 勝敗:

- 各移動コストと断層判定は `engine.apply_command()` の結果をそのまま表示します
- `max_fuel=16`
- `B` は到着ごとに満タンまで回復し、何度でも利用できます
- `R` は最大 `+5` を供給し、燃料が増えた最初の到着時だけ消費されます
- `STATUS: WON` で勝利、`STATUS: LOST FUEL` で行動不能です

requested / effective seed の再現:

- playable map が見つからなかった候補は `requested_seed + attempt` で deterministic に再抽選されます
- 画面の `requested=<n> effective=<n> rerolls=<n>` を記録すれば同じ map を再現できます

JSON ログの用途:

- `--json-log` は CLI セッションを `GameLog schema_version=3` として保存します
- `Q` / EOF で終了したセッションは `final_summary.outcome = ABORTED_NO_POLICY_ACTION` になります
- generation error は通常敗北と分離し、`requested / attempts / last_candidate_seed / reason / message` を表示します

## Integrated command-response CLI prototype

`integrated_play.py` は、LRS / SRS / HUD を command response として表示する統合CLIプロトタイプです。

```bash
python experiments/galactic_exodus/integrated_play.py --seed 42
```

このCLIでは `N/E/S/W` は最終的に SRS内の移動として扱います。LRS座標は `EXIT N/E/S/W` が成功した場合だけ変化します。

この issue 時点では、`N/E/S/W` / `MOVE` / `INTERACT` / `EXIT` は parser のみ先行し、実行は後続 issue で追加します。

既存 `play.py` は LRS-only prototype として残しています。

これは Phase 1A のプロトタイプであり、完成ゲーム UI ではありません。

固定 seed のサンプル操作:

```text
$ python experiments/galactic_exodus/play.py --seed 42
MAP:
y=8 ? ? ? ? ? ? ? H
y=7 ? ? ? ? ? ? ? ?
y=6 ? ? ? ? ? ? ? ?
y=5 ? ? ? ? ? ? ? ?
y=4 ? ? ? ? ? ? ? ?
y=3 ? ? ? ? ? ? ? ?
y=2 ? ? ? ? ? ? ? ?
y=1 P ? ? ? ? ? ? ?
  1 2 3 4 5 6 7 8
SEED: requested=42 effective=42 rerolls=0
POSITION: (1,1)
FUEL: 16/16
LAST SUPPLY: none
USED R: -
TURN: 0
STATUS: IN PROGRESS
BLOCKED: -
COMMAND> q
```

## 実行方法

```bash
python experiments/galactic_exodus/simulate.py \
  --seed 42 \
  --resource-count 3 \
  --rift-density 0.10 \
  --initial-fuel 16 \
  --base-supply 8 \
  --resource-supply 5
```

複数 seed の Phase 0 統計をまとめて出す場合は、次を実行します。

```bash
python experiments/galactic_exodus/metrics.py \
  --seed-start 1 \
  --seed-count 10 \
  --rift-density 0.10 \
  --resource-count 3
```

航行力と B/R 補給候補を比較する場合は、次を実行します。

```bash
python experiments/galactic_exodus/fuel_metrics.py \
  --seed-start 1 \
  --seed-count 10 \
  --rift-density 0.10 \
  --initial-fuels 14,16,18 \
  --base-supplies 8,10 \
  --resource-supply 5 \
  --resource-counts 0,1,3 \
  --csv-output .tmp/readme-fuel-metrics.csv \
  --markdown-output .tmp/readme-fuel-metrics.md
```

上の 3 コマンドは README の標準実行例として、unit test からそのまま実行できる形にしてあります。大規模比較を再現したい場合は、後述の結果ファイル生成コマンドを使ってください。

## Phase 1 初期推奨値

Phase 0 の最終判断として、Phase 1 最小縦断スライスへ渡す初期値は次とします。

```text
board_width: 8
board_height: 8
start_position: (1, 1)
goal_position: (8, 8)

terrain_weights:
  plain (.): 0.60
  nebula (N): 0.20
  asteroid (A): 0.12
  anomaly (@): 0.08

terrain_costs:
  plain (.): 1
  nebula (N): 2
  asteroid (A): 3
  anomaly (@): 2
  base (B): 1
  resource (R): 1
  goal (H): 1

rift_density: 0.10

base_positions:
  (4,4) / (5,4) / (4,5) / (5,5)
base_placement:
  上記4候補から seed により 1 地点をランダム選択

initial_fuel: 16
base_supply: 8
resource_count: 3
resource_supply: 5
```

採用理由の要約:

- `rift_density=0.10`
  - 到達不能率 `2.3%`、`S->H cost median=16 / p90=18` で、断層影響を観測しつつ悪化を抑えられた。
- 地形比率・地形コスト
  - `terrain_extra_cost median=1 / p90=3`、positive ratio `73.3%` で地形差が十分に効いていたため現行値を維持する。
- B 配置規則
  - `B mandatory=0.0%`、`B avoid better=35.2%`、`tie=39.8%`、`B via better=25.0%` で固定解化していないため中央 4 候補ランダムを維持する。
- `initial_fuel=16`
  - `rift_density=0.10 / R=3 / B supply=10` で `direct=63.4%`、`any=97.7%`、`base rescue=34.3%`、`resource rescue=34.1%`。`14` は補給依存が強すぎ、`18` 以上は補給の役割が弱すぎた。
- `base_supply=8`
  - `8/10/12` の走破率差は小さく、増加分の主効果は残航行力だったため最小値を採用する。
- `resource_count=3`
  - 現行生成条件の観測比較で `1` より高い R 救済率を示した。これは同一マップへの追加配置の因果効果ではなく、地形・B/R 配置を含む母集団比較である。
- `resource_supply=5`
  - `initial_fuel=16 / resource_count=3` で R 救済が十分に立ち上がったため維持する。追加候補比較は未実施。

暫定許容基準:

```text
到達不能率
  0%〜3%: 許容
  3%超〜5%: 要注意
  5%超: 見直し対象

B必須率
  0%〜1%: 許容
  1%超〜5%: 要注意
  5%超: 見直し対象

B経由偏重
  B via better >= 70%: B経由が強すぎる可能性
  B avoid better >= 70%: Bが価値を持たない可能性
```

Phase 1 で再検証する項目:

- 到達率
- 残航行力
- 燃料切れ率
- B訪問率
- R訪問率
- B/R補給回数
- 直行率
- 迂回率
- R=3 の発見率・訪問率・盤面密度感
- `resource_supply=5` の強さ
- B補給による固定解化
- 到達不能盤面の扱い
- 単一地点・1回補給モデルで十分か

## 地形コスト

基準となる経路分析では、8x8 グリッド上を4方向（`N/E/S/W`）に移動します。
各移動では、移動先マスのコストを加算します。開始地点のコストは加算しません。

| 記号 | 意味 | コスト |
| --- | --- | --- |
| `.` | 安定宙域 | 1 |
| `N` | 星雲 | 2 |
| `A` | 小惑星帯 | 3 |
| `@` | 重力井戸 / 重力異常 | 2 |
| `B` | 辺境基地 | 1 |
| `R` | 資源天体 | 1 |
| `S` | 出発星系 | 0 |
| `H` | 故郷星系 | 1 |

この実験では、マップ全体の情報が既知であることを前提に、移動辺へ任意の断層制約を加えた基準計算を行います。

## 断層航路

グリッドには、無向の隣接辺が合計112本あります。断層辺は seed に基づいて決定的に選ばれます。

```text
rift_count = round(112 * rift_density)
```

密度は `--rift-density FLOAT` で指定します。既定値は `0.10` です。

選択された断層辺は双方向に通行不能となり、次のすべての最短路計算から除外されます。

- `S -> H`
- `S -> B`
- `B -> H`
- `B` を通行禁止にした `S -> H`

## レポート出力

`simulate.py` は、次のセクションをこの順序で出力します。

- `MAP ID`
- `OBJECTS`
- `PARAMETERS`
- `FUEL PARAMETERS`
- `FUEL ANALYSIS`
- `MAP`
- `COSTS`
- `COST CONTRIBUTIONS`
- `VERDICT`

`COSTS` セクションには、verdict 分類に使う次の最短路指標が含まれます。

- `S_to_H_cost`
- `S_to_H_steps`
- `S_to_B_cost`
- `B_to_H_cost`
- `S_to_H_via_B_cost`
- `S_to_H_without_B_cost`
- `base_route_advantage_raw`
- `base_is_mandatory`（`yes` / `no`）

利用できない指標は `N/A` と表示されます。

## 航行力モデル

`simulate.py` には、1 seed レポートと後続比較用に再利用する純粋関数 `analyze_fuel()` があります。

入力パラメータ:

- `initial_fuel`
- `base_supply`
- `resource_supply`

CLI 既定値:

- `initial_fuel = 16`
- `base_supply = 8`
- `resource_supply = 5`

いずれも 0 以上の整数で、負数は `ValueError` です。

この分析では、既存の地形コストと断層制約をそのまま使います。

- `S` では消費しない
- 隣接マスへ移動すると、移動先マスの地形コストを消費する
- 断層辺は通行不能
- 燃料が負になる移動はできない
- `B` / `R` / `H` にちょうど 0 で到着してよい
- 補給は `B` または 1 つの `R` で最大 1 回だけ行う
- 補給量は到着後に加算する
- 燃料容量上限は設けない

評価対象の走行計画は次の 3 種類だけです。

- `direct`: 補給なし
- `via_base`: `B` で 1 回補給
- `via_resource`: 選んだ 1 つの `R` で 1 回補給

`FUEL ANALYSIS` セクションには次が含まれます。

- `fuel_feasible_direct`
- `fuel_feasible_via_base`
- `fuel_feasible_via_resource`
- `remaining_fuel_direct`
- `remaining_fuel_via_base`
- `remaining_fuel_via_resource`
- `remaining_fuel_at_goal`
- `required_supply`
- `best_cost_via_resource`
- `best_resource_position`

bool は `yes` / `no`、値なしは `N/A` で表示します。

## この issue で扱わないもの

次はこの燃料分析の対象外です。

- `B` と `R` の両方で補給する経路
- 複数の `R` で補給する経路
- 同じ地点で複数回補給する経路
- 補給地点の巡回順最適化
- 正式な燃料容量
- プローブ、敵、故障などの追加要素

続いて `COST CONTRIBUTIONS` セクションで、同じ `GalacticMap` に対する全情報あり基準分析を出力します。ここでは地形配置と `S/H/B/R` 配置を固定したまま、経路計算条件だけを切り替えます。

- A: `plain`
  - 断層なし
  - 移動先記号に関係なく、1 移動ごとにコスト 1
- B: `terrain_only`
  - 断層なし
  - 現行の地形コスト表を使用
- C: `full`
  - 生成済み断層あり
  - 現行の地形コスト表を使用

開始地点 `S` のコストは、A/B/C のどれでも加算しません。

`COST CONTRIBUTIONS` セクションには次の 5 指標が含まれます。

- `plain_cost`
  - A の `S -> H` 最小コスト
- `terrain_only_cost`
  - B の `S -> H` 最小コスト
- `full_cost`
  - C の `S -> H` 最小コスト
- `terrain_extra_cost`
  - `terrain_only_cost - plain_cost`
  - 地形コスト差だけで基準経路にどれだけ追加消費が生じるか
- `rift_detour_cost`
  - `full_cost - terrain_only_cost`
  - 現行地形上で断層を加えたことでどれだけ追加消費が生じるか

`full` 条件だけは断層により到達不能になり得ます。その場合は `full_cost` と `rift_detour_cost` を内部では `None` とし、レポートでは `N/A` と表示します。`plain_cost`、`terrain_only_cost`、`terrain_extra_cost` は、その場合でも数値のまま保持されます。

## verdict の判定規則

verdict の優先順位は次のとおりです。

1. `REJECT_TOO_HARD`
2. `REJECT_BASE_MANDATORY`
3. `ACCEPT`

分類規則:

- `REJECT_TOO_HARD`: `S -> H`、`S -> B`、`B -> H` のいずれかが到達不能
- `REJECT_BASE_MANDATORY`: 必要な各区間には到達できるが、`B` を回避する `S -> H` 経路が存在しない
- `ACCEPT`: 上位の棄却条件に該当しないマップ

`ACCEPT` は最低限の候補判定にすぎません。そのマップがすでに面白い、バランスが取れている、最終品質に達していることを意味しません。

## Batch Metrics

`metrics.py` は、`rift_density` と `resource_count` を固定したまま、連続した seed 範囲をまとめて評価します。
各マップと断層辺の選択は数値 seed から決定的に導出されるため、同じ入力なら出力は再現可能です。

必須入力:

- `--seed-start`: 連続範囲の開始 seed
- `--seed-count`: 実行する seed 数
- `--rift-density`: バッチ全体で共有する断層密度
- `--resource-count`: バッチ全体で共有する資源天体数

テキストレポートには次が含まれます。

- verdict 件数・割合
- `S_to_H_cost` min / median / p90 / max
- `S_to_H_steps` min / median / p90 / max
- `base_is_mandatory` 件数・割合
- `base_route_advantage_raw` の negative / zero / positive / unavailable 件数・割合

`S_to_H_cost` と `S_to_H_steps` は、到達不能ケースを分布統計から除外し、その件数を `excluded_unreachable` として併記します。

`base_route_advantage_raw` の意味:

- negative: `B` 経由の最良経路が `B` 回避の最良経路より悪い
- zero: `B` 経由と `B` 回避の総コストが同じ
- positive: `B` 経由のほうが `B` 回避より安い
- unavailable: `B` 経由または `B` 回避のどちらかの経路が存在しない

`p90` は、到達可能サンプルを昇順に並べたうえで nearest-rank 方式で求めます。

## Fuel Comparison Metrics

`fuel_metrics.py` は、`simulate.analyze_fuel()` を使って複数の燃料条件を同じ seed 範囲で比較します。

必須入力:

- `--seed-start`
- `--seed-count`
- `--rift-density`
- `--initial-fuels`
- `--base-supplies`
- `--resource-supply`
- `--resource-counts`

複数値引数はカンマ区切りです。`--rift-density` も `0.10,0.15` のように複数指定できます。大規模比較を再現する標準コマンドは次です。

```bash
python experiments/galactic_exodus/fuel_metrics.py \
  --seed-start 1 \
  --seed-count 1000 \
  --rift-density 0.10,0.15 \
  --initial-fuels 14,16,18,20,22,24 \
  --base-supplies 8,10,12 \
  --resource-supply 5 \
  --resource-counts 0,1,3 \
  --csv-output experiments/galactic_exodus/results/fuel_comparison_low_initial_seed_1_1000.csv \
  --markdown-output experiments/galactic_exodus/results/fuel_comparison_low_initial_seed_1_1000.md
```

configuration の並び順は常に次です。

```text
rift_density
resource_count
initial_fuel
base_supply
resource_supply
```

各 configuration について次を出力します。

- `direct_feasible_count / ratio`
- `via_base_feasible_count / ratio`
- `via_resource_feasible_count / ratio`
- `any_feasible_count / ratio`
- `still_infeasible_count / ratio`
- `rescued_by_base_count / ratio`
- `rescued_by_resource_count / ratio`
- `base_only_rescue_count / ratio`
- `resource_only_rescue_count / ratio`
- `both_supply_options_count / ratio`
- `base_rescue_rate_among_direct_failures`
- `resource_rescue_rate_among_direct_failures`
- `any_rescue_rate_among_direct_failures`
- `base_only_share_among_rescued`
- `any_feasible_ratio_delta_vs_r0`
- `still_infeasible_ratio_delta_vs_r0`

分布統計は #1033 と同じ `DistributionStats` を使い、次の 3 指標を集計します。

- `remaining_fuel_at_goal`
- `required_supply`
- `best_cost_via_resource`

各分布は次の列を持ちます。

- `sample_count`
- `excluded_count`
- `min`
- `median`
- `p90`
- `max`

規則:

- `None` は分布から除外する
- `remaining_fuel_at_goal` は走破不能 seed を除外する
- `required_supply` は direct 成功時の `0` を含む
- `best_cost_via_resource` は `resource_count=0` で全件 `N/A`

出力:

- 標準出力: 人が比較しやすい要約表
- `--csv-output`: 1 configuration 1 行の CSV
- `--markdown-output`: 再現コマンドと全 configuration 詳細を含む Markdown レポート

## テスト

Python 実験環境のテストは、標準ライブラリの `unittest` で実行します。

```bash
python -m unittest \
  experiments.galactic_exodus.test_simulate \
  experiments.galactic_exodus.test_metrics \
  experiments.galactic_exodus.test_fuel_metrics
```
