# Galactic Exodus Phase 2 SRS初期モデル

## 1. 目的

本書は、星系内SRS移動・探索をPythonで検証するための初期仮説を固定する。最終仕様ではなく、#1080で追加判断なしにプロトタイプ実装できる入力契約である。

## 2. 位置づけ

- Phase 1の`GalacticMap.rift_edges`はマクロ評価用の縮約表現である。
- Phase 2では`RIFT`を独立したLRS区画種別として扱う。
- 最終ASCII/Braille/HUDは#1076で扱う。
- 敵、戦闘、追跡は初期検証の非対象とする。
- `NEBULA`、`ASTEROID`、`GRAVITY`は型と初期効果候補をPhase 2Aで定義し、Python実装は段階分けできる。

## 3. データモデル

```text
Direction = N | E | S | W

SectorType =
  NORMAL
  BASE
  RESOURCE
  NEBULA
  ASTEROID
  GRAVITY
  RIFT

SrsTerrainType =
  FLOOR
  WALL
  STATION_STRUCTURE
  DEBRIS
  NEBULA
  ASTEROID_FIELD
  ASTEROID
  GRAVITY_FIELD
  RIFT_DISTORTION
  RIFT_BARRIER

SrsFeatureType =
  WARP_POINT

SrsObjectType =
  STAR
  PLANET
  BASE_NODE
  RESOURCE_CACHE
  SALVAGE

SrsActorType =
  PLAYER

SrsCell:
  terrain: SrsTerrainType
  feature: optional[SrsFeature]
  object_id: optional[str]
  actor_id: optional[str]

SectorDescriptor:
  sector_id: str
  sector_type: SectorType
  sector_seed: int
  blocked_edges: set[Direction]
  terrain_profile: TerrainProfile
  object_profile: ObjectProfile

SrsPersistentState:
  generated_map_id
  discovered_cells
  consumed_object_ids
  activated_object_ids
```

### 3.1 要素カテゴリの責務

```text
terrain:
  基礎通行性、移動コスト、観測効果を持つ

feature:
  terrain上に重なる非占有の機能地点

object:
  天体または取得・利用対象

actor:
  プレイヤーなどの移動主体
```

### 3.2 共通属性

```text
id
category
passable
base_move_cost
observation_effect
can_host_feature
can_host_object
interaction_required
consumable
persistent_after_revisit
state_after_use
allowed_sector_types
blocks_line_travel
collision_behavior
```

必要に応じて次も持てる。

```text
forced_movement
fuel_effect
turn_effect
visibility_rule
directional_cost
placement_constraints
```

### 3.3 不変条件

```text
RIFT:
  blocked_edgesは1..3辺
  少なくとも1辺は非blocked

RIFT以外:
  blocked_edgesは空集合

全区画:
  有効なWARP_POINT方向は非blocked
  selected exit方向は非blocked
  sector typeと内部object typeは独立
  STARは必ず1個
  PLANETは1個以上
```

4辺すべてblockedのRIFTは初期モデルでは生成しない。

## 4. LRS/SRS接続とWARP_POINT

各辺中央の星系間接続セルは、機能上の出入口だが、世界観上は周辺より重力が安定したワープ可能地点である。

```text
SrsFeature:
  feature_type = WARP_POINT
  direction = N | E | S | W
  destination_sector = optional
```

各WARP_POINTは到着地点と出発地点を兼ねる。`ENTRY`と`EXIT`は別型として定義しない。

```text
LRSで西から東へ区画へ進入
  → 移動先SRSのW側WARP_POINTに到着

SRSのN側WARP_POINTからワープ
  → LRSの北隣区画へ移動
```

WARP_POINTからの星系間移動は自動ではなく明示的な`WARP`行動とする。

RIFTのblocked edge方向にはWARP_POINTを生成せず、外周を`RIFT_BARRIER`で閉鎖する。LRS境界は双方向に閉鎖される。

```text
sector AのEがblocked
  ⇔
sector BとのE/W境界が双方向にblocked
```

RIFT区画が閉鎖辺を所有する内部表現と、LRS境界が双方向に閉鎖される外部契約は分離する。

## 5. SRS盤面

比較対象は次の2種類とする。

```text
SMALL  = 7x7
MEDIUM = 9x9
```

初期推奨値は`9x9`、比較対象は`7x7`とする。11x11以上は初期検証から外す。

座標は0始まり、南西を`(0,0)`とし、北は`y+1`、東は`x+1`とする。幅・高さは奇数のみ許可する。

### 5.1 WARP_POINT座標

各辺中央1セルをWARP_POINT候補とする。

```text
N = (center_x, height-1)
E = (width-1, center_y)
S = (center_x, 0)
W = (0, center_y)
```

到着時のプレイヤーはWARP_POINT上に配置する。WARP_POINTから盤面内部へ少なくとも1マス進めることを保証する。

```text
warp_clearance_depth = 1
```

WARP_POINTとその内側1セルには、通行不能terrainまたはobjectを配置しない。入口幅2以上は初期比較から外す。

## 6. 恒星と惑星

すべてのSectorTypeのSRSマップへ恒星と惑星を配置する。

```text
STAR:
  count = exactly 1
  category = CELESTIAL_BODY
  passable = false
  blocks_line_travel = true
  interaction_required = false
  consumable = false
  persistent_after_revisit = true
  collision_behavior = STOP_BEFORE

PLANET:
  count >= 1
  category = CELESTIAL_BODY
  passable = false
  blocks_line_travel = true
  interaction_required = false
  consumable = false
  persistent_after_revisit = true
  collision_behavior = STOP_BEFORE
```

初期配置数は盤面サイズ別に次を用いる。

```text
7x7:
  STAR = 1
  PLANET = 1..3

9x9:
  STAR = 1
  PLANET = 2..5
```

STARとPLANETは次と重ならない。

```text
WARP_POINT
他のobject
PLAYER初期位置
```

天体配置後も、全WARP_POINT間および必須objectまでの到達可能性を維持する。

## 7. Terrain profile

```text
NORMAL:
  FLOOR
  WALL

BASE:
  FLOOR
  STATION_STRUCTURE

RESOURCE:
  FLOOR
  DEBRIS

NEBULA:
  FLOOR
  NEBULA
  初期効果候補 = 観測範囲縮小

ASTEROID:
  FLOOR
  ASTEROID_FIELD
  ASTEROID
  ASTEROID_FIELD = 通行可能・高移動コスト
  ASTEROID = 通行不能

GRAVITY:
  FLOOR
  GRAVITY_FIELD
  初期効果候補 = 方向依存移動コスト
  強制移動は比較候補とし基準条件には含めない

RIFT:
  FLOOR
  RIFT_DISTORTION
  RIFT_BARRIER
```

地形効果は移動方式から独立したデータとして定義する。

```text
passable
base_move_cost
blocks_line_travel
stops_on_entry
observation_effect
directional_cost
forced_movement
```

## 8. Object profile

天体objectは全profile・全SectorTypeへ必須配置する。以下は天体以外の価値objectだけを示す。

```text
PROFILE_MINIMAL:
  NORMAL: なし
  BASE: BASE_NODE 1個
  RESOURCE: RESOURCE_CACHE 1個
  NEBULA: なし
  ASTEROID: なし
  GRAVITY: なし
  RIFT: なし

PROFILE_EXPLORATION:
  NORMAL: SALVAGE 0..1個
  BASE: BASE_NODE 1個
  RESOURCE: RESOURCE_CACHE 1個
  NEBULA: SALVAGE 0..1個
  ASTEROID: SALVAGEまたはRESOURCE_CACHE 0..1個
  GRAVITY: SALVAGE 0..1個
  RIFT: SALVAGEまたはRESOURCE_CACHE 0..1個
```

BASE/RESOURCEの効果量はPhase 1値を変更せず、SRS内部で到達が必要かだけを評価する。

## 9. 重なり規則

```text
terrain + feature:
  terrain.can_host_feature = trueの場合のみ可

terrain + object:
  terrain.can_host_object = trueの場合のみ可

feature + object:
  不可

object + object:
  不可

actor + object:
  object.passable = trueの場合のみ可

actor + WARP_POINT:
  可
```

セル通行可否は、terrain、feature、object、actor占有規則を合成して決定する。

## 10. 移動方式

LRSの`N/E/S/W`一歩移動をSRSの基準方式として固定しない。次の3方式を比較する。

```text
VECTOR_COMMAND:
  角度 + 距離を指定
  経路は直線をグリッドへラスタライズ

MOVEMENT_POINTS:
  1ターンの移動力内で経路または目的地を指定
  terrain costを消費

DIRECTIONAL_THRUST:
  8方向 + 推進距離を指定
  直線移動
```

移動ルールと入力方式を分離する。

```text
movement_rule:
  VECTOR_COMMAND
  MOVEMENT_POINTS
  DIRECTIONAL_THRUST

path_input_mode:
  STEPWISE_ROUTE
  DESTINATION_AUTO_PATH
  ROUTE_PREVIEW
```

### 10.1 基準値

```text
movement_rule = MOVEMENT_POINTS
movement_points_per_turn = 4
path_input_mode = ROUTE_PREVIEW
```

`MOVEMENT_POINTS`を基準方式、`DIRECTIONAL_THRUST`を比較方式、`VECTOR_COMMAND`を実験方式とする。

### 10.2 直線移動の衝突規則

VECTOR_COMMANDとDIRECTIONAL_THRUSTでは、移動経路上の全セルを順番に判定する。

```text
STOP_BEFORE:
  最初の通行不能セルの直前で停止
  実際に通過した距離またはcostだけを消費
  通行不能セルへは進入しない
```

未観測セルへの移動は許可するが、衝突・早期停止をGameLogへ記録する。

### 10.3 行動契約

```text
MOVE_ROUTE <coordinates...>
MOVE_TO <coordinate>
VECTOR <angle> <distance>
THRUST <direction8> <distance>
INTERACT
WARP <N|E|S|W>
```

有効なコマンドは選択中のmovement_ruleとpath_input_modeにより制限される。

- `INTERACT`は現在cellのobjectを利用・取得する。
- `WARP`は対応方向のWARP_POINT上にいる場合だけ成功する。
- 1ターン中の複数セル移動後に観測を更新するタイミングは、通過セルごととする。

interaction mode:

```text
AUTO_INTERACT
EXPLICIT_INTERACT
```

## 11. 観測方式

```text
FULL:
  開始時からSRS全体を既知

LOCAL_3X3:
  開始時と成功移動中の各通過cellで現在地周囲3x3を累積開示
```

閉鎖辺の知識:

```text
KNOWN_DESCRIPTOR:
  sector typeとblocked edgesは進入時に既知

LOCAL_DISCOVERY:
  sector typeは既知、閉鎖外周は観測範囲に入った時点で既知
```

視線・SCAN方式は初期検証から外す。

## 12. turn/fuel方式

```text
TURN_ONLY:
  移動command / INTERACT / WARPはSRS turnだけを消費
  LRS fuelは区画間移動時だけ消費

SHARED_FUEL:
  SRS移動では実際に消費したmovement costに応じてLRS fuelを消費
  INTERACT / WARPはfuelを消費しない
```

「進入・離脱だけfuel消費」はPhase 1の区画間移動と重複するため初期比較から除外する。

## 13. 内部生成

生成順:

```text
1. 基本terrain
2. 外周
3. 非blocked辺中央へWARP_POINT
4. RIFT閉鎖外周
5. STAR 1個
6. PLANET複数
7. 内部terrain障害物
8. 価値objects
9. 到達可能性検証
```

障害物密度候補:

```text
LOW    = 0.10
MEDIUM = 0.20
```

必須保証:

- 全有効WARP_POINTが同一の通行可能領域に属する
- 各WARP_POINTから盤面内部へ進める
- 必須objectへ到達可能
- 閉鎖外周は内側からも通過不能
- STARはexactly 1
- PLANET数は盤面サイズ別範囲内
- 同一seedとdescriptorで同一map

## 14. 再訪時の永続状態

保持する:

```text
generated SRS map
sector type
blocked edges
STAR / PLANET配置
consumed/activated objects
discovered cells
```

セッションごとに更新する:

```text
player position
selected warp direction
current events
```

visited cellsは独立永続化せず、discovered cellsへ統合する。

## 15. 基準条件

```text
map_size                 = 9x9
obstacle_density         = 0.20
observation              = LOCAL_3X3
cost_mode                = TURN_ONLY
interaction_mode         = EXPLICIT_INTERACT
object_profile           = PROFILE_EXPLORATION
rift_knowledge           = KNOWN_DESCRIPTOR
movement_rule            = MOVEMENT_POINTS
movement_points_per_turn = 4
path_input_mode           = ROUTE_PREVIEW
warp_clearance_depth     = 1
star_count               = 1
planet_count             = 2..5
max_srs_turns            = 40
```

## 16. 比較条件

```text
C1  7x7 vs 9x9
C2  FULL vs LOCAL_3X3
C3  TURN_ONLY vs SHARED_FUEL
C4  AUTO_INTERACT vs EXPLICIT_INTERACT
C5  PROFILE_MINIMAL vs PROFILE_EXPLORATION
C6  KNOWN_DESCRIPTOR vs LOCAL_DISCOVERY
C7  obstacle 0.10 vs 0.20
C8  VECTOR_COMMAND vs MOVEMENT_POINTS vs DIRECTIONAL_THRUST
```

C8は3条件比較とし、負荷が高い場合は次の二段階へ分割できる。

```text
C8a VECTOR_COMMAND vs MOVEMENT_POINTS
C8b C8aの勝者 vs DIRECTIONAL_THRUST
```

各比較では対象以外の条件を基準値へ固定する。全組合せ直積は行わない。

## 17. 評価指標

共通:

```text
goal_reach_rate
median_srs_turn
p90_srs_turn
mean_commands_to_exit
movement_distance
wasted_movement
collision_count
blocked_command_count
route_replanning_count
explored_cell_ratio
object_acquisition_rate
fuel_or_turn_cost
unreachable_state_rate
```

VECTOR_COMMAND:

```text
endpoint_rounding_error
premature_collision_rate
unknown_space_collision_rate
requested_vs_actual_distance
```

MOVEMENT_POINTS:

```text
unused_movement_points
path_cost_efficiency
auto_path_override_count
```

## 18. 初期判断閾値

これらは検証用の仮説であり、#1083で最終決定する。

```text
p90 SRS turn <= 25
手動の区画長さ評価 >= 3.5 / 5
方向感覚評価 >= 4.0 / 5
探索価値評価 >= 3.5 / 5
移動理解評価 >= 4.0 / 5
操縦感評価 >= 3.5 / 5
行動不能率 <= 0.01
blocked edge無駄試行率 <= 0.15
意図しない衝突率 <= 0.10
```

## 19. #1080への引き渡し

#1080は次を追加判断なしで実装する。

第一段階:

- enumとdescriptor schema
- `NORMAL / BASE / RESOURCE / RIFT`
- terrain / feature / object / actor分離
- WARP_POINTとRIFT閉鎖外周
- STAR 1個とPLANET複数の配置
- 7x7 / 9x9
- object profiles
- observation/cost/interaction modes
- 3移動方式を切り替え可能な共通movement interface
- persistent fields
- C1..C8比較条件
- Q1..Q16の必要指標とfixture

第二段階:

- `NEBULA / ASTEROID / GRAVITY`
- 観測縮小、高移動コスト、方向依存コスト
- 必要に応じて重力場の強制移動比較
