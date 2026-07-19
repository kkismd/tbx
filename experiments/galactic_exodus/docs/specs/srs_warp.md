# Galactic Exodus SRS WARP specification

Source issue: #1088
Related implementation: #1254, #1255
Traceability audit: #1260

この文書は、Galactic Exodus Phase 2 SRS における WARP / RIFT edge / SRS外周 exit 表現の仕様正本である。

## 目的

SRS local map から隣接 LRS sector へ移動できる地点を、旧来の固定 `WARP_POINT` object ではなく、cell が持つ方向別 `warp_flags` として表現する。

これにより、次を避ける。

```text
- 辺中央1点だけがwarp可能という固定配置
- WARP_POINT object / Feature と terrain / object placement の混同
- WarpZone descriptor など、LRS edgeとSRS cellの対応を別概念で二重管理すること
```

## 用語

| 用語 | 意味 |
|---|---|
| `warp_flags` | SRS cell が持つ方向別 exit flag。値は `N`, `E`, `S`, `W` の集合。 |
| warp cell | `warp_flags` が空でない passable cell。 |
| exit direction | `warp_flags` に含まれる LRS移動方向。 |
| blocked edge | LRS上でRIFT等により通行不能なsector間edge。 |
| `RIFT_BARRIER` | SRS外周に配置される通行不能terrain。対応するLRS edgeがblockedであることを表す。 |

## 廃止した表現

次は現行仕様では使わない。

```text
- `WARP_POINT` object
- Featureとしてのwarp point
- 辺中央固定のwarp point
- WarpZone descriptor
```

WARPは object / feature ではなく、SRS cell の `warp_flags` で表現する。

## 基本ルール

### 1. warp可能cell

warp可能cellは、次の条件を満たす。

```text
- terrain が passable である
- 通常は FLOOR cell として扱う
- SRS map の外周に接している
- 対応方向の `warp_flags` を持つ
```

warp可能cellは、通常の移動対象cellでもある。
playerはそのcell上に立った状態で `EXIT <dir>` を実行する。

### 2. 2x2 FLOOR cluster 条件

外周cellにwarp flagを付与するには、そのcellが対応する辺に接する 2x2 FLOOR cluster に含まれている必要がある。

この条件により、warp可能位置が1点固定ではなく、外周の通行可能な開口部として表現される。

例:

```text
S edgeの場合:
  display y=1 / internal y=0 の外周FLOOR cellが候補になる。
  そのcellを含む、S edgeに接した2x2 FLOOR clusterが存在する場合、`S` flagを付与できる。
```

### 3. 四隅のmulti-flag

四隅cellは、条件を満たせば2方向のwarp flagを持てる。

例:

```text
south-west corner:
  `S` と `W` の両方の2x2 FLOOR cluster条件を満たす場合、warp_flags = {S, W} になり得る。
```

表示上は、単方向なら `^`, `>`, `v`, `<` などで表し、複数方向を持つcellは `+` で表してよい。

### 4. object配置不可

`warp_flags` が空でないcellには object を配置しない。

理由:

```text
- WARPはobjectではなくcell propertyである
- playerがexit可能なcell上で、RESOURCE_CACHE / SALVAGE / STATION等のinteraction対象と意味が衝突するのを避ける
```

## RIFT / blocked edge との関係

### 1. blocked edge 方向はwarp禁止

LRS上で対応方向のedgeがblockedの場合、その方向のwarp flagは付与しない。

例:

```text
blocked_edges = {E}

SRS cell上の `E` warp flag:
  付与しない
```

### 2. RIFT_BARRIER配置

blocked edgeに対応するSRS外周には `RIFT_BARRIER` を配置する。

```text
E edge blocked:
  SRS map の east外周に `RIFT_BARRIER` を配置する
```

`RIFT_BARRIER` は通行不能terrainである。
そのため、playerはblocked edge側の外周cellへ移動できず、その方向の `EXIT` も成立しない。

### 3. non-blocked edge

blockedではない方向では、通常の2x2 FLOOR cluster条件に従ってwarp flagを付与する。

ただし、銀河外縁方向はLRS destinationが存在しないため、warp flagを付与してはならない。
SRS generator単体でLRS board境界情報を持たない場合、その制限は呼び出し側または後続統合で保証する。
この制限は #1264 で扱う。

## EXIT command との関係

SRSからLRSへ移動するのは `EXIT <dir>` のみである。

`EXIT <dir>` は、少なくとも次を満たす場合に成功する。

```text
- 現在playerがいるcellがSRS map内にある
- 現在cellの `warp_flags` に `<dir>` が含まれる
- `<dir>` が known blocked edge ではない
- LRS destinationがboard内にある
- combat等、上位game loop上の移動禁止条件を満たしている
```

この文書では、SRS cell側の `warp_flags` と blocked edge 表現を定義する。
統合CLI上の実装済み / deferred制約の一覧化は #1268 で扱う。

## 実装反映先

現行prototypeでは、主に次へ反映されている。

```text
experiments/galactic_exodus/srs/generate.py
experiments/galactic_exodus/srs/test_generate.py
experiments/galactic_exodus/integrated_play.py
experiments/galactic_exodus/test_integrated_play.py
```

#1254 では `srs/generate.py` の `warp_flags` 生成を #1088 仕様へ同期した。
#1255 では `integrated_play.py` の minimal SRS `warp_flags` を #1088 仕様へ同期した。

## 表示との関係

表示設計では、単方向のwarp flagを次のように表してよい。

| Direction | Symbol |
|---|---|
| `N` | `^` |
| `E` | `>` |
| `S` | `v` |
| `W` | `<` |
| multiple | `+` |

この表示は `experiments/galactic_exodus/docs/design/galactic_exodus_display_samples.md` の比較サンプルに従う。
ただし、表示記号は仕様本体ではなくUI表現である。
仕様上の正本は `warp_flags` である。

## 後続課題

この文書では、terrain-count generation profile の完全実装は扱わない。
#1088 の terrain-count generation profile を今すぐ実装するか deferred にするかは #1263 で扱う。

SRS generatorにLRS board外縁情報を渡す設計は #1264 で扱う。
