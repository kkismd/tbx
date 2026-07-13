# Galactic Exodus Phase 2 SRS移動解決規則

> **文書区分:** 履歴資料 — 旧仕様
>
> この文書は過去の仕様・設計経緯を保存するための archive です。現行の gameplay specification ではありません。現行仕様は `experiments/galactic_exodus/docs/specs/` を参照してください。

- Former path: `experiments/galactic_exodus/srs/phase2_srs_movement.md`
- Former role: `SRS` movement解決仕様
- Superseded by: `experiments/galactic_exodus/docs/specs/srs_movement.md`, `experiments/galactic_exodus/docs/specs/srs_warp.md`
- Archived by: #1318

## 1. 目的

本書は #1089 の成果物として、SRS内の移動command、経路列挙、地形cost、衝突、観測更新、turn/fuel、interaction、warp/exit の解決規則を固定する。

正本値は `phase2_srs_movement.json` とし、本書は実装者向け説明を担当する。Terrain/Objectの属性は `phase2_srs_elements.json`、生成profileは `phase2_srs_generation.json`、baselineと比較条件は `phase2_initial_values.json` を参照する。

## 2. baseline

```text
cost_mode = TURN_ONLY
movement_rule = MOVEMENT_POINTS
movement_points_per_turn = 4
path_input_mode = ROUTE_PREVIEW
interaction_mode = EXPLICIT_INTERACT
collision_behavior = STOP_BEFORE
observation_mode = LOCAL_MOVEMENT
max_srs_turns = 40
```

baselineでは、SRS内の移動はLRS fuelを直接消費しない。SRS内では accepted command が `SRS turn` を進め、地形costは1 command内で進める距離、経路選択、到達turn数へ反映する。

`SHARED_FUEL` はC3/Q9の比較条件として残す。

## 3. cost単位

既存の地形仕様では、縦横移動を10、斜め移動を14のraw costで表す。

```text
orthogonal_raw_cost = 10
diagonal_raw_cost = 14
movement_points_per_turn = 4
movement_cost_budget_raw = 40
```

つまり、`MOVEMENT_POINTS` baselineの4 movement pointsは、raw cost 40に対応する。

```text
FLOOR 縦横1セル = 10 raw cost = 1 point
DEBRIS / NEBULA 縦横1セル = 20 raw cost = 2 points
ASTEROID_FIELD 縦横1セル = 30 raw cost = 3 points
```

`SHARED_FUEL`では、実際に通過したraw movement costを10で割り上げた値をfuel消費として扱う。

## 4. commandとturn

無効入力、実行前に拒否されたcommandはSRS turnを消費しない。

accepted commandは次のように扱う。

```text
movement command:
  1 SRS turn

INTERACT:
  1 SRS turn

WARP / EXIT:
  1 SRS turn
```

`max_srs_turns` の判定は accepted command の解決後に行う。

## 5. MOVEMENT_POINTS

`MOVEMENT_POINTS` はbaselineの移動方式である。

```text
allowed_step_directions = N / E / S / W
diagonal_allowed = false
direction_changes_within_turn = FREE
unused_budget_carryover = false
budget_raw = 40
```

`MOVE_ROUTE` は方向列を受け取り、raw cost 40以内で実行できる最長prefixを実行する。prefixが1セルも実行できない場合は、実行前拒否としてturnを消費しない。

`MOVE_TO` を使う場合はknown stateのみでpathを作る。tie-breakは以下の順に固定する。

```text
1. total raw costが最小
2. step数が最小
3. N/E/S/W順の方向列として辞書順最小
```

## 6. VECTOR_COMMAND

> **旧仕様に関する注記**
>
> 状態: `SUPERSEDED`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/srs_movement.md`
> 置換内容: `## 6-7` の `VECTOR_COMMAND` / `DIRECTIONAL_THRUST` は baseline ではなく、current docs では historical comparison candidate としてのみ扱う。
> 履歴として残す内容: 比較用 command surface、angle / distance、ray / supercover line の旧比較条件。
> 関連issue: #1321

`VECTOR_COMMAND` は比較対象である。

```text
0度 = N
時計回り
angle = 0..359 の整数
distance = 1..4 の整数
```

endpointは次で求める。

```text
dx = round_half_away_from_zero(sin(angle) * distance)
dy = round_half_away_from_zero(cos(angle) * distance)
```

sourceとendpointが同一セルになる場合は実行前拒否とする。

pathはsourceを除きdestinationを含むsupercover lineで列挙する。terrain costによりraw budget 40を超える場合、実行可能な最長prefixで止まる。

## 7. DIRECTIONAL_THRUST

`DIRECTIONAL_THRUST` は比較対象である。

```text
allowed_directions = N / NE / E / SE / S / SW / W / NW
distance = 1..4 の整数
direction_changes_within_command = FORBIDDEN
```

指定方向へ直線rayを列挙し、raw budget 40以内の最長prefixを実行する。地形costが高い場合は、指定距離より短く止まる。

## 8. STOP_BEFORE

通行不能要素は、既存elements契約の `ASTEROID`、`RIFT_BARRIER`、`STAR`、`PLANET`、`STATION` である。

```text
STOP_BEFORE:
  最初の通行不能cellには進入しない
  通行不能cellの直前で停止する
  通行不能cell自体のmovement costは消費しない
```

最初の対象cellが通行不能だった場合:

```text
position = unchanged
movement_raw_cost = 0
SRS turn = +1
observation update = none
```

途中まで進んだ後に衝突した場合:

```text
position = last entered passable cell
movement_raw_cost = entered passable step cost sum
SRS turn = +1
observation update = each entered passable cell
```

斜め移動では、2つの直交隣接セルの両方が通行不能である角をすり抜けることを禁止する。

## 9. 観測

`FULL` はsector entry時にactual map全体を開示する。

`LOCAL_MOVEMENT` では、成功した各1セル移動の後に移動先terrainを基準に観測する。

```text
default terrain = 5x5
NEBULA = 3x3
known map = cumulative
failed / rejected command = no observation update
first blocked cell collision = no observation update
```

## 10. interaction

> **旧仕様に関する注記**
>
> 状態: `CONFLICTING`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/srs_movement.md`, `experiments/galactic_exodus/docs/specs/srs_objects.md`, `experiments/galactic_exodus/docs/specs/srs_map_generation.md`
> 旧記述: `RESOURCE_CACHE` を sector total `+5` split とし、`SALVAGE` の効果を `DEFERRED_PLACEHOLDER` としている。
> 現行仕様: `srs_objects.md` は `RESOURCE_CACHE` を fixed fuel `+3`、`SALVAGE` を `RECOVER_ENERGY` / `RECOVER_PHOTON_TORPEDO_AMMO` / `STORE_ONLY` の fixed Phase 2 initial behavior としている。
> 競合内容: legacy の interaction baseline を current contract として読むと、回復量と `SALVAGE` choice set を誤認する。
> 関連issue: #1321

baselineは `EXPLICIT_INTERACT` である。

### RESOURCE_CACHE

```text
range = SAME_CELL
effect = REFUEL_PARTIAL
sector total refuel = +5
persistent field = consumed_object_ids
```

RESOURCE sector内の全cache合計を、大マップR 1地点分、つまり最大+5相当として扱う。

```text
cache count 1: +5
cache count 2: +3 / +2
cache count 3: +2 / +2 / +1
```

max fuelを超えた分は切り捨てる。満タン時など実回復量が0の場合、cacheは消費済みにしない。

### STATION

```text
range = ADJACENT
effect = REFUEL_TO_MAX
reusable = true
persistent field = activated_object_ids
```

### SALVAGE

```text
range = SAME_CELL
effect = DEFERRED_PLACEHOLDER
persistent field = consumed_object_ids
```

戦闘・装備・修理などの効果は後続Issueへ送る。

## 11. warp / exit

warp/exitには、該当方向の `warp_flags` を持つcellが必要である。RIFT blocked edgeや銀河外縁へ向かうwarp/exitは実行前拒否とする。

accepted warp/exit は1 SRS turnを消費する。無効または拒否されたwarp/exitはturnを消費しない。

到着候補が存在しないmapは生成契約側のgeneration error/retry対象であり、実行時fallbackは持たない。

## 12. 後続へ送るもの

次は本契約では扱わない。

```text
enemy_spawn
threat_clock_formula
encounter_rate
salvage_combat_or_repair_effect
final_input_ui
```

ただし、将来接続できるように、SRS turn、現在terrain、移動/interaction/warp eventはGameLogへ記録する。
