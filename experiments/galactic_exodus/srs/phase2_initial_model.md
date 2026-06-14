# Galactic Exodus Phase 2 SRS初期モデル

## 1. 目的

本書は、星系内SRS移動・探索をPythonで検証するための初期仮説を固定する。最終仕様ではなく、#1080で追加判断なしにプロトタイプ実装できる入力契約である。

## 2. 位置づけ

- Phase 1の`GalacticMap.rift_edges`はマクロ評価用の縮約表現である。
- Phase 2では`RIFT`を独立したLRS区画種別として扱う。
- 最終ASCII/Braille/HUDは#1076で扱う。
- N/A/@、敵、戦闘、追跡は初期検証の非対象とする。

## 3. データモデル

```text
Direction = N | E | S | W

SectorType =
  NORMAL
  BASE
  RESOURCE
  RIFT

SectorDescriptor:
  sector_id: str
  sector_type: SectorType
  sector_seed: int
  blocked_edges: set[Direction]
  object_profile: ObjectProfile

SrsObjectType =
  BASE_NODE
  RESOURCE_CACHE
  SALVAGE

SrsPersistentState:
  generated_map_id
  discovered_cells
  consumed_object_ids
  activated_object_ids
```

### 不変条件

```text
RIFT:
  blocked_edgesは1..3辺
  少なくとも1辺は非blocked

RIFT以外:
  blocked_edgesは空集合

全区画:
  entry edgeは非blocked
  selected exitは非blocked
  sector typeと内部object typeは独立
```

4辺すべてblockedのRIFTは初期モデルでは生成しない。

## 4. LRS/SRS接続

LRSである方向から区画へ進入した場合、SRSでは同じ辺の入口から開始する。

```text
LRSで西から東へ区画へ進入 → SRSのW入口から開始
SRSのN出口から離脱       → LRSの北隣区画へ移動
```

RIFTのblocked edge方向には入口・出口を生成せず、外周を通行不能cell/objectで閉鎖する。

LRS境界は双方向に閉鎖される。

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

### 入口・出口

各辺中央1セルを入口・出口とする。

```text
N = (center_x, height-1)
E = (width-1, center_y)
S = (center_x, 0)
W = (0, center_y)
```

プレイヤー開始位置は入口に隣接する盤面内セルとする。

```text
Nから進入 → (center_x, height-2)
Eから進入 → (width-2, center_y)
Sから進入 → (center_x, 1)
Wから進入 → (1, center_y)
```

入口幅2以上は初期比較から外す。

## 6. 内部生成

生成順:

```text
1. 基本床
2. 外周
3. 非blocked入口・出口
4. RIFT閉鎖外周
5. 内部障害物
6. objects
7. 到達可能性検証
```

障害物密度候補:

```text
LOW    = 0.10
MEDIUM = 0.20
```

必須保証:

- entryから全非blocked出口へ到達可能
- 必須objectへ到達可能
- 閉鎖外周は内側からも通過不能
- 同一seedとdescriptorで同一map

## 7. Object profile

```text
PROFILE_MINIMAL:
  NORMAL: なし
  BASE: BASE_NODE 1個
  RESOURCE: RESOURCE_CACHE 1個
  RIFT: なし

PROFILE_EXPLORATION:
  NORMAL: SALVAGE 0..1個
  BASE: BASE_NODE 1個
  RESOURCE: RESOURCE_CACHE 1個
  RIFT: SALVAGEまたはRESOURCE_CACHE 0..1個
```

BASE/RESOURCEの効果量はPhase 1値を変更せず、SRS内部で到達が必要かだけを評価する。

## 8. 行動契約

```text
N / E / S / W
INTERACT
EXIT N / E / S / W
```

- MOVEは隣接盤面内cellへ移動する。
- INTERACTは現在cellのobjectを利用・取得する。
- EXITは対応する出口cellにいる場合だけ成功する。

interaction mode:

```text
AUTO_INTERACT
EXPLICIT_INTERACT
```

## 9. 観測方式

```text
FULL:
  開始時からSRS全体を既知

LOCAL_3X3:
  開始時と成功移動後に現在地周囲3x3を累積開示
```

閉鎖辺の知識:

```text
KNOWN_DESCRIPTOR:
  sector typeとblocked edgesは進入時に既知

LOCAL_DISCOVERY:
  sector typeは既知、閉鎖外周は観測範囲に入った時点で既知
```

視線・SCAN方式は初期検証から外す。

## 10. turn/fuel方式

```text
TURN_ONLY:
  MOVE / INTERACT / EXITはSRS turnだけを消費
  LRS fuelは区画間移動時だけ消費

SHARED_FUEL:
  SRS MOVEごとにLRS fuelを1消費
  INTERACT / EXITはfuelを消費しない
```

「進入・離脱だけfuel消費」はPhase 1の区画間移動と重複するため初期比較から除外する。

## 11. 再訪時の永続状態

保持する:

```text
generated SRS map
sector type
blocked edges
consumed/activated objects
discovered cells
```

セッションごとに更新する:

```text
player position
selected exit
current events
```

visited cellsは独立永続化せず、discovered cellsへ統合する。

## 12. 基準条件

```text
map_size          = 9x9
obstacle_density  = 0.20
observation       = LOCAL_3X3
cost_mode         = TURN_ONLY
interaction_mode  = EXPLICIT_INTERACT
object_profile    = PROFILE_EXPLORATION
rift_knowledge    = KNOWN_DESCRIPTOR
max_srs_turns     = 40
```

## 13. 比較条件

```text
C1  7x7 vs 9x9
C2  FULL vs LOCAL_3X3
C3  TURN_ONLY vs SHARED_FUEL
C4  AUTO_INTERACT vs EXPLICIT_INTERACT
C5  PROFILE_MINIMAL vs PROFILE_EXPLORATION
C6  KNOWN_DESCRIPTOR vs LOCAL_DISCOVERY
C7  obstacle 0.10 vs 0.20
```

各比較では対象以外の条件を基準値へ固定する。全組合せ直積は行わない。

## 14. 初期判断閾値

これらは検証用の仮説であり、#1083で最終決定する。

```text
p90 SRS turn <= 25
手動の区画長さ評価 >= 3.5 / 5
方向感覚評価 >= 4.0 / 5
探索価値評価 >= 3.5 / 5
行動不能率 <= 0.01
blocked edge無駄試行率 <= 0.15
```

## 15. #1080への引き渡し

#1080は以下を追加判断なしで実装する。

- enumとdescriptor schema
- RIFT不変条件
- 7x7 / 9x9
- 入口・出口座標
- RIFT閉鎖外周生成
- object profiles
- observation/cost/interaction modes
- persistent fields
- C1..C7比較条件
- Q1..Q10の必要指標とfixture
