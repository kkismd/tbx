# Galactic Exodus SRS map generation specification

Source issues: #1085, #1086, #1088
Traceability audit: #1260
Follow-up: #1263, #1264

この文書は、Galactic Exodus Phase 2 SRS map generation の正本仕様と、現行prototypeで未実装として残す範囲を記録する。

## 目的

SRS map generation は、LRS sector type、blocked edge情報、LRS board境界情報から、SRS local map、terrain、objects、warp flags、persistent state を作る。

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

ただし、この手順だけでは LRS board外縁方向を判定できない。
LRS board外縁方向の扱いは、この文書の「LRS board境界情報」節を正本とする。

## RIFT / WARP

WARP の詳細仕様は `srs_warp.md` を正本とする。

この文書では、生成器側の責務だけを扱う。

```text
- blocked edge方向にはwarp flagを付与しない
- blocked edge方向の外周にはRIFT_BARRIERを配置する
- non-blocked edgeでは2x2 FLOOR cluster条件を満たす外周cellへwarp flagを付与する
- warp flag付きcellにはobjectを配置しない
```

## LRS board境界情報

### 決定

#1264時点では、次を正式方針とする。

```text
Decision:
  SRS生成は、LRS board外縁方向を blocked edge と同等に扱う。

Preferred implementation:
  SRS生成の呼び出し側が、現在LRS座標とboard boundsから allowed_exit_edges または blocked_exit_edges を解決し、SectorDescriptor相当の入力へ渡す。

Current prototype status:
  `create_sector()` は LRS board境界情報を直接持たないため、descriptor.blocked_edges に含まれない方向をopen edgeとして扱う。
```

銀河外縁方向にはLRS destinationが存在しないため、SRS cellへその方向の `warp_flags` を付与してはならない。

### 用語

| 用語 | 意味 |
|---|---|
| `board edge` | LRS board外縁。隣接sectorが存在しない方向。 |
| `blocked edge` | RIFT等によりsector間移動が禁止されている方向。 |
| `allowed_exit_edges` | LRS destinationが存在し、かつRIFT等でblockedされていない方向集合。 |
| `blocked_exit_edges` | RIFT blocked edge と board edge を合わせた、SRSからexitできない方向集合。 |

### 生成時の解決順

SRS生成時のexit方向は、次の順に解決する。

```text
1. 現在LRS座標から、N/E/S/Wそれぞれのdestinationを計算する
2. destinationがLRS board外なら、その方向を board edge とする
3. known / generated RIFT edge がある方向を blocked edge とする
4. board edge と blocked edge を合わせて blocked_exit_edges とする
5. blocked_exit_edges 方向には warp flag を付与しない
6. blocked_exit_edges 方向のSRS外周には RIFT_BARRIER 相当の通行不能外周を置く
```

`RIFT_BARRIER` というterrain名はRIFT由来だが、Phase 2 prototypeでは「exit不可外周」を表す通行不能terrainとして board edge にも使ってよい。
必要なら将来、表示上だけ `BOARD_EDGE_BARRIER` などへ分ける。

### 推奨データ契約

将来実装では、`SectorDescriptor` またはその呼び出し元に次のどちらかを追加する。

```text
Option A:
  allowed_exit_edges: frozenset[Direction]

Option B:
  blocked_exit_edges: frozenset[Direction]
```

推奨は Option A である。

理由:

```text
- board外・RIFT・将来の一時封鎖を「許可されたexit集合」へ集約できる
- 生成器は allowed_exit_edges に含まれない方向へwarp flagを付けなければよい
- EXIT commandの成功条件とも対応しやすい
```

ただし、現行modelに即して最小変更で実装する場合は、`blocked_edges` に board edge を含めて渡す互換案も許容する。
その場合、`blocked_edges` が「RIFTのみ」ではなく「exit不可方向」を意味するようになるため、命名またはcommentで明確化する。

### integrated CLIとの関係

現行 `integrated_play.py` の minimal SRS は、全edgeへwarp candidateを付与し、`EXIT <dir>` 実行時にLRS board外をrejectする。

これは安全側のruntime validationではあるが、仕様上は生成時点でboard外方向のwarp flagを出さない方が正しい。

したがって、将来の full SRS generation 統合では次を満たす。

```text
- LRS board外方向はSRS生成時点でwarp flagを持たない
- integrated CLIのboard外rejectは、防御的validationとして残してよい
- 表示上、board外方向はwarp可能に見えてはならない
```

### #1264時点の実装方針

#1264では、gameplay実装・fixture・snapshotは変更しない。

このissueでは設計を固定し、実装は後続へ送る。

後続実装issueでは、少なくとも次を扱う。

```text
- SectorDescriptorまたはSRS generation呼び出し元のexit方向contract
- LRS board boundsからallowed_exit_edgesを計算するhelper
- srs/generate.py の warp_flags生成
- integrated_play.py minimal SRS generation
- test_generate.py / test_integrated_play.py で外縁方向にwarp flagが出ないことの確認
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

## #1264 の結論

現行 `create_sector()` は LRS board境界情報を直接持たない。
そのため、`descriptor.blocked_edges` に含まれない方向はopen edgeとして扱う。

#1264では、board外縁方向を生成時にexit不可として扱う方針を固定する。
ただし、現行prototypeの実装変更は行わない。
実装は、allowed_exit_edges / blocked_exit_edges contractを決める後続issueで扱う。
