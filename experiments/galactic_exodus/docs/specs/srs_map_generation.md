# Galactic Exodus SRS map generation specification

Source issues: #1085, #1086, #1088
Traceability audit: #1260
Follow-up: #1263, #1264

この文書は、Galactic Exodus Phase 2 SRS map generation の正本仕様と、現行prototypeで未実装として残す範囲を記録する。

## 目的

SRS map generation は、LRS sector type と blocked edge 情報から、SRS local map、terrain、objects、warp flags、persistent state を作る。

現時点では、WARP / RIFT edge 表現とobject配置は実装済みである。
一方、#1088 で検討された SectorType別 terrain-count generation profile は、現行prototypeではまだ full implementation にしない。

## 現行prototypeで固定する範囲

現行 `experiments/galactic_exodus/srs/generate.py` は、9x9 mapのみを生成する。

```text
MAP_WIDTH  = 9
MAP_HEIGHT = 9
MAP_CENTER = internal Position(4, 4)
```

SRS internal coordinate は 0-origin lower-left である。

```text
x increases eastward
y increases northward
```

## 生成手順

現行prototypeの生成手順は次の通りである。

```text
1. 全cellを FLOOR として初期化する
2. descriptor.blocked_edges に対応する外周へ RIFT_BARRIER を配置する
3. non-blocked edge へ warp_flags を付与する
4. entry_edge に対応する固定初期位置へplayerを置く
5. STAR / PLANET / sector type別extra objectを配置する
6. persistent_stateへgeneration metadataを保存する
```

## RIFT / WARP

WARP の詳細仕様は `srs_warp.md` を正本とする。

この文書では、生成器側の責務だけを扱う。

```text
- blocked edge方向にはwarp flagを付与しない
- blocked edge方向の外周にはRIFT_BARRIERを配置する
- non-blocked edgeでは2x2 FLOOR cluster条件を満たす外周cellへwarp flagを付与する
- warp flag付きcellにはobjectを配置しない
```

## object placement

現行prototypeでは、各sectorに次のobjectを配置する。

```text
common:
  - STAR x1
  - PLANET x2

sector type別extra:
  NORMAL   -> SALVAGE x1
  BASE     -> STATION x1
  RESOURCE -> RESOURCE_CACHE x1
  RIFT     -> SALVAGE x1
```

object配置候補は次を満たすcellである。

```text
- player初期位置ではない
- terrain が FLOOR
- warp_flags が空
- object_id が未設定
```

## resource cache metadata

RESOURCE_CACHE は、現行prototypeではcache数ぶん一律で fuel restore 3 を持つ。

```text
resource_cache_restore_values(cache_count):
  return tuple(3 for _ in range(cache_count))
```

## #1088 terrain-count generation profile の扱い

#1088 では、旧 `obstacle_density` 方式をやめ、SectorType別 terrain count range / limit で生成する方針が検討された。

ただし、現行prototypeでは次の理由により、このprofileをまだ full implementation にしない。

```text
- WARP / RIFT edge / EXIT command の整合を先に固定している
- terrain countを入れると object placement、warp candidate、fixture、snapshotが同時に変化する
- combat / encounter / movement評価の前提が変わる可能性がある
- #1264 の LRS外縁情報設計と合わせて扱う方が安全である
```

したがって、#1263時点では次を正式方針とする。

```text
Decision:
  #1088 terrain-count generation profile は deferred とする。

Current implementation:
  minimal all-FLOOR base + RIFT_BARRIER + warp_flags + objects を維持する。

Future implementation:
  SectorType別terrain count range / limitを実装する場合は、専用issueでgenerator・fixtures・tests・balanceへの影響をまとめて扱う。
```

## deferred項目

次は未実装として明記する。

```text
- SectorType別 terrain count range / limit
- FLOORを残余として扱うterrain composition generation
- NEBULA / ASTEROID_FIELD / DEBRIS / GRAVITY_FIELD / RIFT_DISTORTION の配置profile
- terrain countに応じたobject placement候補の再計算
- terrain countに応じたwarp candidate不足時のfallback / retry policy
- terrain generation seedの安定化方針
```

## 実装変更方針

この文書追加では、gameplay実装、fixture、snapshot、balance値を変更しない。

full terrain-count profileを実装する場合は、別issueで次を同時に扱う。

```text
- experiments/galactic_exodus/srs/generate.py
- experiments/galactic_exodus/srs/test_generate.py
- fixtures / fixture regression
- display snapshotがある場合はsnapshot更新
- encounter / movement balanceへの影響確認
```

## #1264 との関係

現行 `create_sector()` は LRS board境界情報を直接持たない。
そのため、`descriptor.blocked_edges` に含まれない方向はopen edgeとして扱う。

銀河外縁方向にwarp flagを出さない保証をどの層で行うかは #1264 で扱う。
