# Galactic Exodus Phase 2 SRS初期モデル

> **文書区分:** 履歴資料 — 旧仕様
>
> この文書は過去の仕様・設計経緯を保存するための archive です。現行の gameplay specification ではありません。現行仕様は `experiments/galactic_exodus/docs/specs/` を参照してください。

- Former path: `experiments/galactic_exodus/srs/phase2_initial_model.md`
- Former role: `Phase 2` 初期データモデル・設計前提
- Superseded by: `experiments/galactic_exodus/docs/specs/README.md`, `experiments/galactic_exodus/docs/specs/srs_map_generation.md`, `experiments/galactic_exodus/docs/specs/srs_movement.md`, `experiments/galactic_exodus/docs/specs/srs_warp.md`
- Archived by: #1318

## 1. 目的

本書は、星系内SRS移動・探索の初期検証で使用するモデル境界を固定する。最終仕様ではなく、#1080 のプロトタイプ実装と評価設計が同じ入力契約を参照できる状態を作る。

## 2. 文書の責務

> **旧仕様に関する注記**
>
> 状態: `SUPERSEDED`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/srs_movement.md`, `experiments/galactic_exodus/docs/specs/srs_map_generation.md`, `experiments/galactic_exodus/docs/specs/srs_warp.md`
> 置換内容: `## 2-4` の文書責務、data model、warp contract は current docs で責務別 spec へ分割され、現行 baseline は `9x9` と `warp_flags` 契約を直接参照する。
> 履歴として残す内容: evaluation-stage の文書境界、`11x11` candidate を含む初期モデル、統合前の比較用メモ。
> 関連issue: #1321

- 要素型・通行性・効果の正本は `phase2_srs_elements.md/json`
- `SectorType` 別生成profile、range、warp、配置制約、seed/retry の正本は `phase2_srs_generation.md/json`
- baseline、比較条件、閾値、永続項目は `phase2_initial_values.json`
- 本文では正本の値を再定義しない

初期モデルでは、featureベースのwarp表現、統合前のTerrain/Object型、方向を持たない重力場型、固定入口幅、密度ベース生成は廃止した。正本型と生成規則は elements/generation 契約を参照する。

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

`SrsCell.warp_flags` は各セルの辺接続を直接表し、`SectorDescriptor` は銀河側で決まる blocked edge と seed を保持する。要素属性、配置可否、配置数range、特殊Terrain上限は正本JSONを参照する。

## 4. Warp契約

- warp は `SrsCell.warp_flags` の `N/E/S/W` で表現する
- 接続可能な各辺に 2x2 以上の `FLOOR` 予約領域を最低 1 つ生成する
- qualifying cluster の外周セルへ方向 flag を付与する
- corner cell は複数方向 flag を保持できる
- `RIFT` blocked edge と銀河外縁では flag を禁止する
- 到着候補は反対辺の return flag 付きセルとする
- 軸距離最小と契約済み tie-break で候補を選ぶ
- 候補なしは生成失敗として retry する
- 実行時 fallback map は持たない

warp の具体的な配置制約、retry、seed 再現性、return flag の決定順序は `phase2_srs_generation.md/json` を参照する。

## 5. 盤面と天体

> **旧仕様に関する注記**
>
> 状態: `SUPERSEDED`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/srs_map_generation.md`
> 置換内容: `11x11` candidate の天体数 range は current Phase 2 baseline に含まれず、現行仕様は `9x9` baseline の map size と celestial counts を正本としている。
> 履歴として残す内容: `11x11` candidate board size に対する `PLANET` count の比較用候補値。
> 関連issue: #1321

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

座標系、観測、移動コスト、`STOP_BEFORE`、天体・価値Objectの配置制約、特殊Terrain個数上限は正本契約を参照する。本文では profile 値を複製しない。

## 6. 実装段階

> **旧仕様に関する注記**
>
> 状態: `SUPERSEDED`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/README.md`, `experiments/galactic_exodus/docs/specs/srs_map_generation.md`
> 置換内容: staged rollout の段階分けは gameplay contract の authority ではなく、現行仕様は各 current spec header と `README.md` の source / follow-up 管理へ移行している。
> 履歴として残す内容: 実装段階のメモと rollout 順序。
> 関連issue: #1321

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

ただし第一段階開始時点で、全 7 `SectorType` の型、profile、不変条件が正本に存在することを前提とする。

## 7. 評価前提

> **旧仕様に関する注記**
>
> 状態: `SUPERSEDED`
>
> このsectionは現行仕様の正本ではない。
> 現在の正本: `experiments/galactic_exodus/docs/specs/README.md`, `experiments/galactic_exodus/docs/specs/srs_movement.md`
> 置換内容: current gameplay contract は baseline の移動方式と command 契約を `srs_movement.md` へ移し、spec index は `README.md` で管理している。
> 履歴として残す内容: `C1..C8` 比較 ID、`Q1..Q16` 質問票、`VECTOR_COMMAND` / `MOVEMENT_POINTS` / `DIRECTIONAL_THRUST` の evaluation-specific comparison memo。
> 関連issue: #1321

初期比較条件、baseline、永続状態、比較IDは `phase2_initial_values.json` と `phase2_questions.csv` を参照する。

```text
baseline:
  LOCAL_MOVEMENT
  TURN_ONLY
  EXPLICIT_INTERACT
  VALUE_OBJECT_DETOUR
  KNOWN_DESCRIPTOR
  MOVEMENT_POINTS
  ROUTE_PREVIEW
  STOP_BEFORE

比較:
  C1..C8

質問:
  Q1..Q16
```

移動方式の比較対象は次の 3 つとする。

```text
VECTOR_COMMAND
MOVEMENT_POINTS
DIRECTIONAL_THRUST
```

観測方式、相互作用方式、移動方式、persistent fields の確定値は `phase2_initial_values.json` を正本とする。
