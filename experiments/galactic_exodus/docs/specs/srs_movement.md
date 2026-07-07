# Galactic Exodus SRS movement and exploration specification

Source issues: #1083, #1089
Related issues: #1078, #1081, #1082, #1162, #1163, #1164, #1165, #1166, #1167
Traceability audit: #1260
Follow-up: #1267

この文書は、Galactic Exodus Phase 2 SRS の移動・探索ルールの正本仕様である。

## 目的

SRSでは、LRS sector内に入った後のlocal movement、observation、object interaction、warp exit、persistent stateを扱う。

#1083 / #1089 により、Phase 2のbaselineは次とする。

```text
movement_rule = MOVEMENT_POINTS
cost_mode = TURN_ONLY
movement_points_per_turn = 4
SRS turn is kept as a log unit for future threat / encounter design
```

VECTOR_COMMAND と DIRECTIONAL_THRUST は、Phase 2 baseline では採用しない。
必要なら比較実験・後続案として残す。

## 座標系

SRS internal coordinate は 0-origin lower-left である。

```text
internal x increases eastward
internal y increases northward
```

表示座標は 1-origin lower-left である。

```text
display_x = internal_x + 1
display_y = internal_y + 1
```

engine / fixture / validator / tests / raw event payload は internal coordinate を使う。
render / manual eval / HUD / display samples は display coordinate を使う。

## map size

Phase 2 baselineのSRS mapは 9x9 とする。

```text
width = 9
height = 9
internal range = (0,0) ... (8,8)
display range = (1,1) ... (9,9)
```

## movement model

### baseline

Phase 2 baselineの移動方式は MOVEMENT_POINTS である。

```text
movement_points_per_turn = 4
directions = N, E, S, W
turning = free within one command
unused movement points are not carried over
```

1 movement command は、受理されると 1 SRS turn を消費する。

### MOVE_ROUTE

`MOVE_ROUTE` は、方向列を受け取り、順に解決する。

```text
command_type = MOVE_ROUTE
route = tuple[Direction, ...]
```

各stepでは次を行う。

```text
1. current positionからdirection方向のnext positionを求める
2. next positionがmap外または通行不能なら、その直前で停止する
3. destination terrainのmovement costを計算する
4. accumulated costが1turnのmovement budgetを超える場合、そのstepは実行しない
5. 実行したstepをentered_cellsへ追加する
6. current positionを更新する
```

### MOVE_TO

`MOVE_TO` は既知cellへの自動経路移動である。

```text
command_type = MOVE_TO
target = Position
```

`MOVE_TO` は、次の条件を満たすtargetにだけ解決できる。

```text
- targetがmap内である
- targetが現在位置ではない
- targetがknown_state.discovered_cellsに含まれる
- known/discovered cellだけを通るpathがある
- path上のcellが通行可能である
```

pathは4方向探索で解く。
同じcostの場合のtie-breakは、実装上安定したdirection / position orderを使う。

## collision / STOP_BEFORE

通行不能cellへ入ろうとした場合、playerはそのcellに入らず直前で停止する。

```text
STOP_BEFORE:
  first impassable cell is not entered
  player remains at the last valid position
  blocked_position records the first impassable position
```

最初のstepから通行不能で、entered_cellsが空の場合も、受理されたmovement attemptとして扱い、1 SRS turnを消費する。
ただし、route自体が空またはmovement budget不足で1stepも進めない場合は `REJECTED_ZERO_STEP` として扱う。

## passability

次のcellは通行不能である。

```text
- map外
- terrain ASTEROID
- terrain RIFT_BARRIER
- object STAR
- object PLANET
- object STATION
```

次は通行可能である。

```text
- FLOOR
- DEBRIS
- NEBULA
- ASTEROID_FIELD
- GRAVITY_FIELD_VERTICAL
- GRAVITY_FIELD_HORIZONTAL
- RIFT_DISTORTION
- RESOURCE_CACHE object cell
- SALVAGE object cell
```

RESOURCE_CACHE / SALVAGE は same-cell interaction対象であるため、そのcellへ進入できる。
STATION は adjacent interaction対象であるため、そのcellへ進入できない。

## terrain movement cost

terrainごとのmovement multiplierは次とする。

| Terrain | Multiplier |
|---|---:|
| FLOOR | 1 |
| DEBRIS | 2 |
| NEBULA | 2 |
| ASTEROID_FIELD | 3 |
| GRAVITY_FIELD_VERTICAL | 1 |
| GRAVITY_FIELD_HORIZONTAL | 1 |
| RIFT_DISTORTION | 1 |

ASTEROID / RIFT_BARRIER は通行不能であり、movement costを持たない。

## cost mode

### TURN_ONLY

Phase 2 baselineは TURN_ONLY である。

```text
1 accepted movement command = 1 SRS turn
SRS movement does not directly consume LRS fuel
```

terrain movement costは、1 command / 1 turn 内で移動できる距離、経路選択、到達turn数へ反映する。

### SHARED_FUEL

SHARED_FUEL はbaselineでは採用しない。
比較条件・後続候補として残す。

SHARED_FUELでは、実際に消費したmovement raw costに応じてfuelを消費する。
ただし、Phase 2 baselineの正本挙動は TURN_ONLY である。

## SRS turn

SRS turnは、将来のthreat / encounter / combat接続に使うログ単位として保持する。

```text
MOVE_ROUTE accepted / STOP_BEFORE = +1 SRS turn
MOVE_TO accepted / STOP_BEFORE = +1 SRS turn
INTERACT accepted = +1 SRS turn
WARP_EXIT accepted = +1 SRS turn in SRS engine
WAIT accepted = +1 SRS turn
rejected command = no SRS turn unless explicitly specified
```

integrated CLIで `EXIT <dir>` がLRS移動まで成功した場合は、SRS側のWARP_EXITを通した後、新しいSRS sectorへ入る。
詳細は `integrated_cli.md` を参照する。

## observation

SRSでは、playerの観測範囲に入ったcellを known_state に追加する。

baselineの観測サイズは次とする。

```text
normal sector: 5x5
NEBULA sector: 3x3
```

複数step移動では、entered cellごとにobservationを更新する。
そのため、長いrouteで通過した途中cellも観測centerになり得る。

observation更新では次を記録する。

```text
center
newly_discovered_count
total_discovered_count
```

## known / visited / persistent state

### known_state

`known_state.discovered_cells` は、現在のrunで観測済みのcell集合である。
`known_state.known_cells` は、観測済みcellの公開済みcell情報である。

### visited_cells

`visited_cells` は、playerが訪問済みのcell集合として扱う。
Phase 2 baselineでは、表示・debug・将来評価に使う補助状態である。

### persistent_state

`persistent_state` は、SRS sector再訪時に維持される状態である。

維持対象:

```text
- generated_map_id
- generation_schema_version
- generation_seed
- sector_type
- blocked_edges
- warp_flags
- celestial_body_positions
- consumed_object_ids
- activated_object_ids
- discovered_cells
```

RESOURCE_CACHE / SALVAGE の consumed state、STATION の activated stateはpersistentに記録する。

## object interaction

### common

`INTERACT` は対象objectとrange条件を満たした場合に実行できる。

```text
SAME_CELL:
  player position == object position

ADJACENT:
  Manhattan distance == 1
```

accepted interaction は 1 SRS turn を消費する。
rejected interaction は SRS turn を消費しない。

### RESOURCE_CACHE

RESOURCE_CACHE は SAME_CELL interactionである。

処理順:

```text
1. object存在確認
2. interaction range確認
3. consumedでないことを確認
4. fuel restore量を計算
5. max_fuelでclamp
6. fuel_deltaが0なら REJECTED_NO_EFFECT
7. fuelが増えた場合、objectをconsumedにする
8. consumed_object_idsへ追加
9. INTERACT_ACCEPTED / OBJECT_CONSUMED eventを記録
```

Phase 2 baselineでは、満タンで回復量0の場合は消費済みにしない。

### STATION

STATION は ADJACENT interactionである。

処理順:

```text
1. object存在確認
2. adjacent range確認
3. fuelをmax_fuelへ回復
4. durability / energy / photon torpedo ammoをcapacityまで回復
5. base upgrade choiceがある場合、salvage costを支払ってupgradeを適用
6. activated_object_idsへ追加
7. INTERACT_ACCEPTED / STATION_ACTIVATED eventを記録
```

STATION は reusable objectである。
activated stateは記録するが、再利用不可にはしない。

### SALVAGE

SALVAGE は SAME_CELL interactionである。

Phase 2移動・探索baselineでは、SALVAGEは取得可能objectである。
戦闘・装備・修理効果の最終バランスは別spec / follow-upで扱う。

処理順:

```text
1. object存在確認
2. same-cell range確認
3. consumedでないことを確認
4. salvage rewardをplayerへ適用
5. objectをconsumedにする
6. consumed_object_idsへ追加
7. INTERACT_ACCEPTED / OBJECT_CONSUMED eventを記録
```

SALVAGE効果の詳細は combat / balance spec へ移す。
#1266 の調査では、現行実装・fixture regression上、map pickup / enemy drop / base upgradeの主要経路が確認済みである。

## WARP_EXIT

WARP / RIFT / blocked edge のSRS側仕様は `srs_warp.md` を正本とする。

SRS engine上の `WARP_EXIT` は、少なくとも次を確認する。

```text
- exit_directionが指定されている
- combat_state.enemy_presenceがある場合はreject
- player positionがmap内である
- exit_directionがdescriptor.blocked_edgesに含まれない
- current cellのwarp_flagsにexit_directionが含まれる
```

accepted時は 1 SRS turn を消費し、`WARP_EXIT_ACCEPTED` eventを出す。

integrated CLIでのLRS移動まで含む制約は `integrated_cli.md` を参照する。

## event outcomes

代表outcome:

```text
MOVE_ACCEPTED
MOVE_REJECTED
STOPPED_BEFORE_IMPASSABLE
OBSERVATION_UPDATED
INTERACT_ACCEPTED
INTERACT_REJECTED
OBJECT_CONSUMED
STATION_ACTIVATED
WARP_EXIT_ACCEPTED
WARP_EXIT_REJECTED
WAIT_ACCEPTED
```

movement eventには、少なくとも次を含める。

```text
command_type
cost_mode
start_position
end_position
entered_cells
blocked_position
movement_raw_cost
fuel_before
fuel_after
fuel_delta
observation_updates
outcome
```

## Python reference implementation

Python実装は、この仕様を検証するための実行可能な参照実装である。

主な対応箇所:

```text
experiments/galactic_exodus/srs/model.py
experiments/galactic_exodus/srs/engine.py
experiments/galactic_exodus/srs/generate.py
experiments/galactic_exodus/srs/run_fixture.py
experiments/galactic_exodus/srs/test_fixture_regression.py
```

仕様書・decision log・reference fixtureとPython実装が矛盾した場合は、仕様書・decision log・reference fixtureを正とし、Python実装側を修正する。

## 後続に残すもの

次は、この移動・探索仕様の対象外である。

```text
- enemy / threat / encounter の最終モデル
- combat ruleの完全仕様
- 追跡AI
- SALVAGEの戦闘・装備・修理効果の最終バランス
- VECTOR_COMMAND / DIRECTIONAL_THRUST の正式採用
```

これらは combat / encounter / balance 系の後続specまたはissueで扱う。
