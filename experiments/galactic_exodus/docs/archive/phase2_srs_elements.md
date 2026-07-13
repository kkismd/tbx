# Galactic Exodus Phase 2 SRS地形・要素仕様

> **文書区分:** 履歴資料 — 旧仕様
>
> この文書は過去の仕様・設計経緯を保存するための archive です。現行の gameplay specification ではありません。現行仕様は `experiments/galactic_exodus/docs/specs/` を参照してください。

- Former path: `experiments/galactic_exodus/srs/phase2_srs_elements.md`
- Former role: `SRS` terrain / object / actor 要素仕様
- Superseded by: `experiments/galactic_exodus/docs/specs/srs_map_generation.md`, `experiments/galactic_exodus/docs/specs/srs_objects.md`, `experiments/galactic_exodus/docs/specs/srs_combat.md`
- Archived by: #1318

## 1. 目的と対象範囲

本書は、#1086で確定したSRS地形、マップ要素の通行可否、観測、移動コスト計算、`WARP_POINT`配置、Terrain/Object配置互換性を記録する。

地形の配置数・密度は#1088で扱い、移動commandの厳密な解決規則とturn処理は#1089で扱う。

## 2. 盤面サイズ

> **旧仕様に関する注記**
>
> 状態: `SUPERSEDED`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/srs_movement.md`, `experiments/galactic_exodus/docs/specs/srs_map_generation.md`
> 置換内容: `11x11` candidate を含む盤面候補は current Phase 2 baseline では採用されず、現行仕様は `9x9` baseline を正本として固定している。
> 履歴として残す内容: `11x11` candidate の検討履歴。
> 関連issue: #1321

```text
対応サイズ:
  9x9
  11x11

基準サイズ:
  9x9
```

Phase 2 SRSモデルでは`7x7`を使用しない。

## 3. Terrain型

```text
FLOOR
DEBRIS
NEBULA
ASTEROID_FIELD
ASTEROID
GRAVITY_FIELD_VERTICAL
GRAVITY_FIELD_HORIZONTAL
RIFT_DISTORTION
RIFT_BARRIER
```

`WALL`は使用しない。通行不能セルには、世界観上の意味を持つ個別の型を使用する。

## 4. 移動判定に関係するObject型

```text
STAR
PLANET
STATION
RESOURCE_CACHE
SALVAGE
```

`STATION_STRUCTURE`と`BASE_NODE`は廃止し、通行不能かつ隣接操作可能な1セルObjectである`STATION`へ統合する。

## 5. Terrain属性

> **旧仕様に関する注記**
>
> 状態: `CONFLICTING`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/srs_movement.md`, `experiments/galactic_exodus/docs/specs/srs_warp.md`
> 旧記述: `RIFT_DISTORTION` の移動倍率を `2` とし、`GRAVITY_FIELD_VERTICAL` / `GRAVITY_FIELD_HORIZONTAL` を `1または2` としている。
> 現行仕様: `srs_movement.md` は `RIFT_DISTORTION` と両 `GRAVITY_FIELD_*` の移動倍率を baseline で `1` に固定している。
> 競合内容: legacy の地形倍率表を current baseline として読むと、移動コストと gravity field の効果を誤認する。
> 関連issue: #1321

| ID | 和名 | 通行可能 | 移動倍率 | 観測範囲 | 移動・直線航行を遮断 | `WARP_POINT`配置可 |
|---|---|---:|---:|---|---:|---:|
| `FLOOR` | 通常空間 | true | 1 | 5x5 | false | true |
| `DEBRIS` | デブリ帯 | true | 2 | 5x5 | false | false |
| `NEBULA` | 星雲 | true | 2 | 3x3 | false | false |
| `ASTEROID_FIELD` | 小惑星密集域 | true | 3 | 5x5 | false | false |
| `ASTEROID` | 大型小惑星 | false | - | - | true | false |
| `GRAVITY_FIELD_VERTICAL` | 南北重力異常領域 | true | 1または2 | 5x5 | false | false |
| `GRAVITY_FIELD_HORIZONTAL` | 東西重力異常領域 | true | 1または2 | 5x5 | false | false |
| `RIFT_DISTORTION` | 断層歪曲領域 | true | 2 | 5x5 | false | false |
| `RIFT_BARRIER` | 断層障壁 | false | - | - | true | false |

`passable = false`の要素だけが移動と直線航行を遮断する。通行可能Terrainは、移動コスト増加や観測範囲縮小の効果を持っていても経路を遮断しない。

## 6. 観測

観測は成功した1セル移動ごとに更新する。

```text
1. 次のセルへ移動する
2. 移動先セルのterrainを確認する
3. NEBULAなら3x3、それ以外なら5x5を観測する
4. 観測結果を永続的な既知マップへ統合する
5. 移動が残っていれば次の1セルへ進む
```

既知セルは累積保持し、`NEBULA`へ進入しても過去に発見した情報を忘れない。

新しいセルへの移動が成立しなかった場合、移動先セル基準の観測更新は発生しない。

## 7. 幾何学的な移動コスト

ユークリッド距離を整数で近似する。

```text
ORTHOGONAL_COST = 10
DIAGONAL_COST = 14
```

経路コストは、実際に通過した1セル単位のstep costを合計する。始点と終点の座標差だけでは計算しない。

```text
step_cost = geometric_step_cost
          * destination_terrain_multiplier
          * gravity_multiplier
```

例:

| 移動先Terrain | 縦横移動 | 斜め移動 |
|---|---:|---:|
| `FLOOR` | 10 | 14 |
| `DEBRIS` | 20 | 28 |
| `NEBULA` | 20 | 28 |
| `ASTEROID_FIELD` | 30 | 42 |
| `RIFT_DISTORTION` | 20 | 28 |

## 8. 重力異常領域

> **旧仕様に関する注記**
>
> 状態: `CONFLICTING`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/srs_movement.md`
> 旧記述: `GRAVITY_FIELD_VERTICAL` / `GRAVITY_FIELD_HORIZONTAL` は移動方向に応じて `gravity_multiplier = 1 or 2` を取る。
> 現行仕様: `srs_movement.md` は current baseline の gravity field multiplier を両方とも `1` に固定している。
> 競合内容: legacy は方向依存の追加コストを前提にしているが、current contract は比較候補ではなく baseline の固定値を採用している。
> 関連issue: #1321

### `GRAVITY_FIELD_VERTICAL`

南北方向に沿った重力異常領域であり、X座標の変化を伴う移動のコストを2倍にする。

```text
if dx != 0:
  gravity_multiplier = 2
else:
  gravity_multiplier = 1
```

### `GRAVITY_FIELD_HORIZONTAL`

東西方向に沿った重力異常領域であり、Y座標の変化を伴う移動のコストを2倍にする。

```text
if dy != 0:
  gravity_multiplier = 2
else:
  gravity_multiplier = 1
```

斜め移動ではX座標とY座標の両方が変化するため、どちらの重力異常領域でもコストが2倍になる。

`GRAVITY`星系では、南北・東西の重力異常セルをランダムに選択する。片方のみ、または両方の混在を許可するが、合計配置数は必ず1セル以上とする。総配置量は#1088で決定する。

## 9. 通行不能要素と`STOP_BEFORE`

共通の通行不能要素は次のとおり。

```text
ASTEROID
RIFT_BARRIER
STAR
PLANET
STATION
```

すべて共通して次の挙動を使用する。

```text
collision_behavior = STOP_BEFORE
movement_cost_consumed = false
```

turn消費、途中まで進んだ場合の最終位置、command全体の扱いは#1089で決定する。

## 10. `WARP_POINT`の配置

> **旧仕様に関する注記**
>
> 状態: `SUPERSEDED`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/srs_warp.md`, `experiments/galactic_exodus/docs/specs/srs_map_generation.md`
> 置換内容: object/feature としての `WARP_POINT` 配置規則は廃止され、現行仕様は各 cell の `warp_flags` と edge / return-cell selection 契約で warp を表現する。
> 履歴として残す内容: `WARP_POINT` を独立 feature として扱っていた旧配置前提。
> 関連issue: #1321

`WARP_POINT`は、隣接星系へワープ可能な重力の安定した地点を表す。

```text
WARP_POINTはFLOOR上にのみ配置可能
```

生成時の不変条件:

```text
有効な辺中央:
  terrain = FLOOR
  feature = WARP_POINT

内側隣接セル:
  terrain = FLOOR

WARP_POINTセル:
  object配置不可

RIFTのblocked edge:
  WARP_POINTを生成しない
```

## 11. SectorType × Terrainマトリクス

> **旧仕様に関する注記**
>
> 状態: `SUPERSEDED`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/srs_map_generation.md`, `experiments/galactic_exodus/docs/specs/srs_objects.md`, `experiments/galactic_exodus/docs/specs/srs_warp.md`
> 置換内容: `## 11-12` の terrain / object matrix は current docs で generator の必須・任意・禁止要素、object 配置制約、`warp_flags` 契約へ分割移行された。
> 履歴として残す内容: full terrain-count profile、`11x11` range、現行 docs で deferred のまま残している matrix 比較根拠。
> 関連issue: #1321

凡例:

- `必須`: 1セル以上、または構造上必ず必要
- `任意`: Sector profileに応じて配置可能
- `禁止`: 配置不可
- `blocked辺に必須`: 各blocked edgeに対応して必須

| SectorType | FLOOR | DEBRIS | NEBULA | ASTEROID_FIELD | ASTEROID | GRAVITY_VERTICAL | GRAVITY_HORIZONTAL | RIFT_DISTORTION | RIFT_BARRIER |
|---|---|---|---|---|---|---|---|---|---|
| `NORMAL` | 必須 | 任意 | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 |
| `BASE` | 必須 | 任意 | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 |
| `RESOURCE` | 必須 | 必須 | 禁止 | 任意 | 任意 | 禁止 | 禁止 | 禁止 | 禁止 |
| `NEBULA` | 必須 | 任意 | 必須 | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 |
| `ASTEROID` | 必須 | 任意 | 禁止 | 必須 | 必須 | 禁止 | 禁止 | 禁止 | 禁止 |
| `GRAVITY` | 必須 | 禁止 | 禁止 | 禁止 | 禁止 | 必須または任意 | 必須または任意 | 禁止 | 禁止 |
| `RIFT` | 必須 | 任意 | 禁止 | 任意 | 任意 | 任意 | 任意 | 必須 | blocked辺に必須 |

`GRAVITY`星系には、次の追加不変条件を適用する。

```text
count(GRAVITY_FIELD_VERTICAL)
+ count(GRAVITY_FIELD_HORIZONTAL)
>= 1
```

`RIFT_DISTORTION`は、`RIFT_BARRIER`の内側に隣接する通行可能候補セルへランダムに配置する。配置量は#1088で決定する。

## 12. Terrain × Objectマトリクス

| Terrain | STAR | PLANET | STATION | RESOURCE_CACHE | SALVAGE |
|---|---:|---:|---:|---:|---:|
| `FLOOR` | 可 | 可 | 可 | 可 | 可 |
| `DEBRIS` | 不可 | 不可 | 不可 | 可 | 可 |
| `NEBULA` | 可 | 可 | 不可 | 可 | 可 |
| `ASTEROID_FIELD` | 不可 | 不可 | 不可 | 可 | 可 |
| `ASTEROID` | 不可 | 不可 | 不可 | 不可 | 不可 |
| `GRAVITY_FIELD_VERTICAL` | 可 | 可 | 不可 | 可 | 可 |
| `GRAVITY_FIELD_HORIZONTAL` | 可 | 可 | 不可 | 可 | 可 |
| `RIFT_DISTORTION` | 不可 | 不可 | 不可 | 可 | 可 |
| `RIFT_BARRIER` | 不可 | 不可 | 不可 | 不可 | 不可 |

Object配置の共通不変条件:

```text
WARP_POINTセル:
  object配置不可

通行不能Terrain:
  object配置不可

1セル:
  objectは最大1個

STAR:
  SRSマップごとにexactly 1

PLANET:
  SRSマップごとに複数

STATION:
  BASE星系にexactly 1
  FLOOR上のみ
  通行不能
  隣接セルからINTERACT
```

## 13. 後続Issueへ委譲する事項

以下は意図的に後続Issueへ委譲する。

```text
#1088:
  terrainの配置数・密度
  重力異常領域の総配置量
  RIFT_DISTORTIONの配置量・確率
  STAR / PLANETの個数・間隔
  決定的生成の再試行規則

#1089:
  command schema
  STOP_BEFORE時のturn消費
  途中停止時の最終位置
  斜め移動のcorner cutting
  movement budget不足時の処理
  VECTOR_COMMANDの経路ラスタライズ
  DIRECTIONAL_THRUSTの距離上限
```
