# Galactic Exodus Phase 2 SRS初期モデル

## 1. 目的

本書は、Phase 2 SRSの初期モデルを正本参照型へ更新し、実装・評価・validatorが共有する最小契約を固定する。

- 要素契約の正本: `phase2_srs_elements.md` / `phase2_srs_elements.json`
- 生成契約の正本: `phase2_srs_generation.md` / `phase2_srs_generation.json`
- 評価 baseline・比較条件・閾値・永続項目: `phase2_initial_values.json`

本書は初期モデルの責務だけを持つ。型属性、個数 range、seed/retry、生成手順、通行性、観測効果、移動コスト計算は正本を再定義しない。

## 2. データモデル

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
  DEBRIS
  NEBULA
  ASTEROID_FIELD
  ASTEROID
  GRAVITY_FIELD_VERTICAL
  GRAVITY_FIELD_HORIZONTAL
  RIFT_DISTORTION
  RIFT_BARRIER

SrsObjectType =
  STAR
  PLANET
  STATION
  RESOURCE_CACHE
  SALVAGE

SrsActorType =
  PLAYER

SrsCell:
  terrain: SrsTerrainType
  object_id: optional[str]
  actor_id: optional[str]
  warp_flags: set[Direction]

SectorDescriptor:
  sector_id: str
  sector_type: SectorType
  sector_x: int
  sector_y: int
  blocked_edges: set[Direction]
  galaxy_seed: int
  generation_schema_version: int
  generation_profile_ref: str
```

削除済みの旧表現:

```text
WALL
STATION_STRUCTURE
BASE_NODE
WARP_POINT
WarpZone
GRAVITY_FIELD
SrsFeatureType
SrsFeature
```

`SrsCell` は feature を持たない。warp はセルごとの方向別 flag で表現する。

## 3. 正本の責務分離

初期モデルは次の責務分離を前提にする。

```text
要素型・通行性・観測効果・移動倍率・Terrain/Object配置互換:
  phase2_srs_elements.md/json

SectorType別 generation profile、Terrain/Object range、warp予約、
配置制約、到着候補選択、seed/retry、検証順:
  phase2_srs_generation.md/json

評価 baseline、比較条件 C1〜C8、閾値、永続項目:
  phase2_initial_values.json
```

`SectorDescriptor.generation_profile_ref` は `phase2_srs_generation.json` の SectorType別 profile を指す。初期モデル内に Terrain profile、Object profile、range、retry 条件を複製しない。

## 4. warp統合表現

warp は固定 1 セル feature ではなく、`SrsCell.warp_flags` で表現する。

```text
- warpはSrsCellのN/E/S/W方向flagで表現
- 接続可能な各辺に2x2以上のFLOOR予約領域を最低1つ生成
- qualifying clusterの外周セルへ方向flagを導出
- corner cellは複数方向flagを保持可能
- RIFT blocked edgeと銀河外縁ではflag禁止
- 到着候補は反対辺のreturn flag付きセル
- 軸距離最小、契約済みtie-breakで選択
- 候補なしは生成不正としてretry、実行時fallbackなし
```

LRS境界の開閉とSRS内の warp flag 生成規則は `phase2_srs_generation.md/json` を正本とする。

## 5. 盤面と天体の前提

```text
盤面候補:
  9x9
  11x11

baseline:
  9x9

STAR:
  exactly 1

PLANET:
  9x9 = 2..4
  11x11 = 3..6
```

Terrain/Object の個数 range、SectorType別 profile、special terrain 上限は `phase2_srs_generation.json` を参照する。

初期モデルから削除した旧前提:

```text
7x7
PLANET 9x9 = 2..5
obstacle_density
固定入口幅
clearance depthの独立契約
PROFILE_MINIMAL
PROFILE_EXPLORATION
```

## 6. 実装段階

```text
第一段階:
  NORMAL
  BASE
  RESOURCE
  RIFT

第二段階:
  NEBULA
  ASTEROID
  GRAVITY
```

ただし第一段階開始時点で、全7 SectorType の型、generation profile、不変条件は `phase2_srs_generation.md/json` から参照可能であることを前提にする。

## 7. 引き渡し

- `phase2_questions.csv` の比較観点調整は #1097 で扱う
- 初期モデル validator / 実ファイル統合の拡張は #1098 で扱う
- 本Issueでは、旧契約の重複を除去したうえで、後続実装が `phase2_srs_elements.*` と `phase2_srs_generation.*` を正本として参照できる状態を作る
