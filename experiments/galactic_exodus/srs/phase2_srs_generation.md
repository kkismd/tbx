# Galactic Exodus Phase 2 SRS生成仕様

## 1. 目的と対象範囲

本書は、#1088で確定したSRS星系生成規則を、日本語仕様書と機械可読JSON契約へ固定する。

対象成果物:

```text
experiments/galactic_exodus/srs/phase2_srs_generation.md
experiments/galactic_exodus/srs/phase2_srs_generation.json
```

本書は生成契約のみを扱う。validator実装は#1092、既存初期モデル・評価条件への統合は#1093で扱う。

## 2. 基本契約

```text
generation_schema_version = 1
対応サイズ = 9x9 / 11x11
obstacle_density = 廃止
FLOOR = 他Terrain配置後の残余
必須Terrain = 最低1以上
任意Terrain = 0を許可
```

生成で使用するTerrain型:

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

生成で使用するObject型:

```text
STAR
PLANET
STATION
RESOURCE_CACHE
SALVAGE
```

以下の旧表現は本契約へ持ち込まない。

```text
汎用obstacle密度
固定1マス入口feature
入口descriptor専用表現
汎用通行不能terrain名
旧基地object名
旧基地構造terrain名
```

## 3. SectorType別 generation profile

### 3.1 Terrain構成

| SectorType | required_terrain | optional_terrain | forbidden_terrain |
|---|---|---|---|
| `NORMAL` | `FLOOR` | `DEBRIS` | `NEBULA`, `ASTEROID_FIELD`, `ASTEROID`, `GRAVITY_FIELD_VERTICAL`, `GRAVITY_FIELD_HORIZONTAL`, `RIFT_DISTORTION`, `RIFT_BARRIER` |
| `BASE` | `FLOOR` | `DEBRIS` | `NEBULA`, `ASTEROID_FIELD`, `ASTEROID`, `GRAVITY_FIELD_VERTICAL`, `GRAVITY_FIELD_HORIZONTAL`, `RIFT_DISTORTION`, `RIFT_BARRIER` |
| `RESOURCE` | `FLOOR`, `DEBRIS` | `ASTEROID_FIELD`, `ASTEROID` | `NEBULA`, `GRAVITY_FIELD_VERTICAL`, `GRAVITY_FIELD_HORIZONTAL`, `RIFT_DISTORTION`, `RIFT_BARRIER` |
| `NEBULA` | `FLOOR`, `NEBULA` | `DEBRIS` | `ASTEROID_FIELD`, `ASTEROID`, `GRAVITY_FIELD_VERTICAL`, `GRAVITY_FIELD_HORIZONTAL`, `RIFT_DISTORTION`, `RIFT_BARRIER` |
| `ASTEROID` | `FLOOR`, `ASTEROID_FIELD`, `ASTEROID` | `DEBRIS` | `NEBULA`, `GRAVITY_FIELD_VERTICAL`, `GRAVITY_FIELD_HORIZONTAL`, `RIFT_DISTORTION`, `RIFT_BARRIER` |
| `GRAVITY` | `FLOOR` | `GRAVITY_FIELD_VERTICAL`, `GRAVITY_FIELD_HORIZONTAL` | `DEBRIS`, `NEBULA`, `ASTEROID_FIELD`, `ASTEROID`, `RIFT_DISTORTION`, `RIFT_BARRIER` |
| `RIFT` | `FLOOR`, `RIFT_DISTORTION`, `RIFT_BARRIER` | `DEBRIS`, `ASTEROID_FIELD`, `ASTEROID`, `GRAVITY_FIELD_VERTICAL`, `GRAVITY_FIELD_HORIZONTAL` | `NEBULA` |

`GRAVITY`では個別の向きは任意Terrainとして扱うが、縦横合計セル数は必ず1以上とする。

### 3.2 Terrain個数range

#### 9x9

| SectorType | Terrain | range |
|---|---|---:|
| `NORMAL` | `DEBRIS` | 0〜5 |
| `BASE` | `DEBRIS` | 0〜4 |
| `RESOURCE` | `DEBRIS` | 6〜12 |
| `RESOURCE` | `ASTEROID_FIELD` | 0〜5 |
| `RESOURCE` | `ASTEROID` | 0〜2 |
| `NEBULA` | `NEBULA` | 12〜22 |
| `NEBULA` | `DEBRIS` | 0〜4 |
| `ASTEROID` | `ASTEROID_FIELD` | 10〜18 |
| `ASTEROID` | `ASTEROID` | 3〜7 |
| `ASTEROID` | `DEBRIS` | 0〜4 |
| `GRAVITY` | `GRAVITY_FIELD_VERTICAL + GRAVITY_FIELD_HORIZONTAL` | 10〜20 |
| `RIFT` | `RIFT_DISTORTION` | Barrier隣接候補の30〜60% |
| `RIFT` | `DEBRIS` | 0〜4 |
| `RIFT` | `ASTEROID_FIELD` | 0〜4 |
| `RIFT` | `ASTEROID` | 0〜2 |
| `RIFT` | `GRAVITY_FIELD_VERTICAL + GRAVITY_FIELD_HORIZONTAL` | 0〜5 |

#### 11x11

| SectorType | Terrain | range |
|---|---|---:|
| `NORMAL` | `DEBRIS` | 0〜8 |
| `BASE` | `DEBRIS` | 0〜6 |
| `RESOURCE` | `DEBRIS` | 9〜18 |
| `RESOURCE` | `ASTEROID_FIELD` | 0〜8 |
| `RESOURCE` | `ASTEROID` | 0〜3 |
| `NEBULA` | `NEBULA` | 18〜32 |
| `NEBULA` | `DEBRIS` | 0〜6 |
| `ASTEROID` | `ASTEROID_FIELD` | 15〜27 |
| `ASTEROID` | `ASTEROID` | 5〜10 |
| `ASTEROID` | `DEBRIS` | 0〜6 |
| `GRAVITY` | `GRAVITY_FIELD_VERTICAL + GRAVITY_FIELD_HORIZONTAL` | 15〜30 |
| `RIFT` | `RIFT_DISTORTION` | Barrier隣接候補の30〜60% |
| `RIFT` | `DEBRIS` | 0〜6 |
| `RIFT` | `ASTEROID_FIELD` | 0〜6 |
| `RIFT` | `ASTEROID` | 0〜3 |
| `RIFT` | `GRAVITY_FIELD_VERTICAL + GRAVITY_FIELD_HORIZONTAL` | 0〜8 |

### 3.3 特殊Terrain合計上限

| SectorType | 9x9 | 11x11 |
|---|---:|---:|
| `NORMAL` | 5 | 8 |
| `BASE` | 4 | 6 |
| `RESOURCE` | 16 | 24 |
| `NEBULA` | 24 | 35 |
| `ASTEROID` | 25 | 38 |
| `GRAVITY` | 20 | 30 |
| `RIFT` | `RIFT_BARRIER`既定数 + 14 | `RIFT_BARRIER`既定数 + 22 |

### 3.4 内部通行不能セル上限

`RIFT_BARRIER`を除く内部通行不能セル上限:

```text
9x9  = 10
11x11 = 15
```

対象:

```text
ASTEROID
STAR
PLANET
STATION
```

### 3.5 Object構成と個数range

天体Objectは全SectorType共通で次を適用する。

```text
STAR:
  exactly 1

PLANET:
  9x9  = 2〜4
  11x11 = 3〜6
```

SectorType別 profile:

| SectorType | required_objects | optional_objects | object_count_ranges |
|---|---|---|---|
| `NORMAL` | `STAR`, `PLANET` | `SALVAGE` | `SALVAGE = 0..1` |
| `BASE` | `STAR`, `PLANET`, `STATION` | なし | `STATION = 1` |
| `RESOURCE` | `STAR`, `PLANET`, `RESOURCE_CACHE` | `SALVAGE` | `RESOURCE_CACHE = 9x9:1..2, 11x11:1..3`, `SALVAGE = 0..1` |
| `NEBULA` | `STAR`, `PLANET` | `SALVAGE` | `SALVAGE = 0..1` |
| `ASTEROID` | `STAR`, `PLANET` | `RESOURCE_CACHE`, `SALVAGE` | `RESOURCE_CACHE = 0..1`, `SALVAGE = 0..2` |
| `GRAVITY` | `STAR`, `PLANET` | `SALVAGE` | `SALVAGE = 0..1` |
| `RIFT` | `STAR`, `PLANET`, `RESOURCE_CACHE`, `SALVAGE` | なし | `RESOURCE_CACHE = 1..2`, `SALVAGE = 1..2` |

`RESOURCE_CACHE`と`SALVAGE`は同一星系への同時配置を許可するが、同一セル重複は禁止する。

## 4. 共通配置制約

### 4.1 天体

```text
STAR / PLANET:
  最外周禁止
  星体同士のChebyshev距離2以上
  warp flag付きセルからChebyshev距離1以内へ配置禁止

STATION:
  BASEにexactly 1
  STAR / PLANETからChebyshev距離2以上
  周囲8近傍をFLOOR予約
```

### 4.2 価値Object

```text
RESOURCE_CACHE / SALVAGE:
  warp flag付きセルおよびChebyshev距離1以内へ配置禁止
  STAR / PLANET / STATIONからChebyshev距離1以内へ配置禁止
  各Objectを個別に到達可能へする
```

### 4.3 SectorType追加制約

```text
RESOURCE:
  ASTEROID_FIELD + ASTEROID <= DEBRIS count

ASTEROID:
  ASTEROID count <= ASTEROID_FIELD count / 2

GRAVITY:
  GRAVITY_FIELD_VERTICAL + GRAVITY_FIELD_HORIZONTAL >= 1
  縦横比率はseedで決定

RIFT:
  RIFT_DISTORTIONはBarrier内側隣接候補を座標順列挙し、
  seeded shuffle後に30〜60%を選択する
  最低1セル
```

## 5. warp仕様

### 5.1 生成表現

本契約は旧来の固定入口featureや入口descriptor専用表現を使用しない。

```text
各セルが方向別warp_flags = {N, E, S, W}を持つ
```

### 5.2 生成条件

```text
各有効辺に2x2以上のFLOORクラスタを最低1つ保証
判定に使う2x2 FLOOR全体はTerrain上書き禁止
warp flag付き最外周セルのみObject配置禁止
内側予約セルはObject配置可
```

非RIFT:

```text
隣接Sectorがある各辺でwarp可能セルを1つ以上保証
```

RIFT:

```text
non-blocked edgeのみwarp flagを許可・保証
blocked edgeはwarp flag禁止、RIFT_BARRIERで閉鎖
```

四隅:

```text
条件を満たす各方向を独立判定する
必要なら2方向warpを許可する
```

### 5.3 到着セル選択

出発方向に対応する隣接Sectorの反対辺にあり、戻り方向のwarp flagを持つセルを候補とする。

```text
N / S:
  abs(destination.x - source.x)最小
  tie-break = x昇順, y昇順

E / W:
  abs(destination.y - source.y)最小
  tie-break = y昇順, x昇順
```

到着候補が存在しないマップは生成不正とし、決定的再試行を行う。実行時fallbackは設けない。

## 6. 生成順

```text
1. FLOOR初期化
2. RIFT_BARRIER
3. warp用FLOOR予約
4. 必須Terrain
5. 任意Terrain
6. STAR
7. PLANET
8. STATION
9. 価値Object
10. warp flag導出
11. 検証
```

Terrain個数決定:

```text
1. 必須Terrainを範囲内で抽選
2. 任意Terrainを0を含む範囲内で抽選
3. SectorType別合計上限を検証
4. 超過時は再抽選せず、固定優先順で任意Terrainを減らす
5. 必須Terrainの最低数を再検証
```

任意Terrain削減アルゴリズム:

```text
1. SectorTypeごとの reduction priority を先頭から走査する
2. 各Terrainがその最小値を上回っていれば1セルだけ減らす
3. 1セル減らすたびに priority 走査を先頭へ戻す
4. 合計上限を満たすまで繰り返す
5. どの任意Terrainもこれ以上減らせなければ生成失敗
```

SectorType別 reduction priority:

```text
NORMAL:
  DEBRIS

BASE:
  DEBRIS

RESOURCE:
  ASTEROID
  ASTEROID_FIELD

NEBULA:
  DEBRIS

ASTEROID:
  DEBRIS

GRAVITY:
  GRAVITY_FIELD_HORIZONTAL
  GRAVITY_FIELD_VERTICAL

RIFT:
  DEBRIS
  ASTEROID_FIELD
  ASTEROID
  GRAVITY_FIELD_HORIZONTAL
  GRAVITY_FIELD_VERTICAL
```

## 7. クラスタ生成

```text
NEBULA
ASTEROID_FIELD
DEBRIS
GRAVITY_FIELD_VERTICAL / HORIZONTAL
RIFT_DISTORTION
```

上記は連結クラスタ生成を基本とする。共通制約:

```text
単独孤立セルは全体の20%以下
小さな孤立領域を許可しない
```

個別制約:

```text
NEBULA:
  9x9  = 1〜2クラスタ
  11x11 = 1〜3クラスタ
  各クラスタ最低4セル

ASTEROID:
  ASTEROID_FIELDを先に配置
  ASTEROIDの50%以上をASTEROID_FIELD内部または8近傍へ配置

GRAVITY:
  count > 0 の向きごとに1つ以上のクラスタ
  クラスタ内では同じ向きを優先

RIFT_DISTORTION:
  Barrier内側隣接候補を座標順列挙
  seeded shuffle後に30〜60%を選択
  最低1セル
```

## 8. 到達可能性

以下を同一の通行可能連結成分へ含める。

```text
全warp flag付きセル
STATION
未消費RESOURCE_CACHE
未取得SALVAGE
```

到達可能性判定:

```text
隣接規則は#1089の移動規則IDを参照する
passable = true なら高コストTerrain経由でも到達可能
経路コスト上限は生成妥当性条件へ含めない
その他の通行可能セルも原則すべて同一成分とする
```

## 9. seed・再試行・generation report

### 9.1 seedと派生seed

```text
seed input:
  generation_schema_version
  galaxy_seed
  sector_x
  sector_y
  sector_descriptor

seed construction:
  canonical JSON UTF-8 bytesをSHA-256でhash
  digest全32 bytesをbig-endian unsigned integerへ変換

field order:
  generation_schema_version
  galaxy_seed
  sector_x
  sector_y
  sector_descriptor

normalization:
  文字列はNFC
  object keyは辞書順
  set相当fieldは辞書順arrayへ正規化

sector_descriptor canonical form:
  canonical JSON object
  object keyは辞書順
  blocked_edgesはN/E/S/W辞書順array
  将来field追加時も同じcanonical JSON規則を使う

Python reference:
  random.Random(seed)

derived seeds:
  terrain_seed
  celestial_seed
  object_seed
```

派生seedは、`retry_index`を含む再試行単位seedを先に作成し、その値とlabelを同じcanonical JSON規則でhashして求める。

```text
attempt_seed input:
  base_seed
  retry_index

derived seed input:
  attempt_seed
  phase_label
```

TBX移植時は同一乱数列を要求せず、fixture一致または仕様不変条件一致を要求する。

### 9.2 再試行

```text
失敗時:
  base_seedとretry_indexからattempt_seedを生成
  マップ全体を再生成

attempt数:
  最大64

retry_index:
  0..63

初回attempt:
  retry_index = 0

fallback map:
  なし
```

既存seedの結果が変わる生成規則変更では`generation_schema_version`を上げる。

### 9.3 generation report schema

通常GameLogとは分離し、少なくとも次を記録する。

```text
seed
derived_seeds
retry_index
各Terrain/Objectの要求数
各Terrain/Objectの実配置数
検証結果
```
