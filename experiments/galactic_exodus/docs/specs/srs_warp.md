# Galactic Exodus SRS WARP specification

Source issue: #1088
Related implementation: #1254, #1255
Traceability audit: #1260
Boundary contract: #1350

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
| `allowed_exit_edges` | actual に隣接sectorが存在し、RIFTで遮断されていない方向集合。 |
| `board_edge_directions` | LRS board外で隣接sectorが存在しない方向集合。 |
| `rift_blocked_edges` | actual RIFT による双方向通過不能edgeの方向集合。 |
| `RIFT_BARRIER` | RIFT blocked edge のSRS外周一列に配置される通行不能terrain。 |

## 廃止した表現

次は現行仕様では使わない。

```text
- `WARP_POINT` object
- Featureとしてのwarp point
- 辺中央固定のwarp point
- WarpZone descriptor
```

WARPは object / feature ではなく、SRS cell の `warp_flags` で表現する。

## SectorDescriptorとの関係

SRS generation と WARP_EXIT は、同じ3方向集合 contract を参照する。

```python
@dataclass(frozen=True, slots=True)
class SectorDescriptor:
    sector_id: str
    sector_type: SectorType
    sector_seed: int
    spawn_position: Position
    allowed_exit_edges: frozenset[Direction]
    board_edge_directions: frozenset[Direction]
    rift_blocked_edges: frozenset[Direction]
```

`Direction.N/E/S/W` の各方向は、必ず次のいずれか1つに属する。

```text
allowed_exit_edges
board_edge_directions
rift_blocked_edges
```

3集合は相互排他的であり、和集合は全方向と一致しなければならない。

## 基本ルール

### 1. warp可能cell

warp可能cellは、次の条件を満たす。

```text
- terrain が passable である
- 通常は FLOOR cell として扱う
- SRS map の外周に接している
- 対応方向が allowed_exit_edges に含まれる
- 対応方向の warp_flags を持つ
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
各方向は個別に `allowed_exit_edges` と 2x2 FLOOR cluster 条件を満たす必要がある。

例:

```text
south-west corner:
  S と W の両方が allowed_exit_edges に含まれ、
  両方向の2x2 FLOOR cluster条件を満たす場合、
  warp_flags = {S, W} になり得る。
```

表示上は、単方向なら `^`, `>`, `v`, `<` などで表し、複数方向を持つcellは `+` で表してよい。

### 4. object配置不可

`warp_flags` が空でないcellには object を配置しない。

理由:

```text
- WARPはobjectではなくcell propertyである
- playerがexit可能なcell上で、RESOURCE_CACHE / SALVAGE / STATION等のinteraction対象と意味が衝突するのを避ける
```

## direction分類ごとの生成規則

### `allowed_exit_edges`

`allowed_exit_edges` に含まれる方向だけが warp flag を生成できる。

```text
- 対応方向の外周cellは通常terrain
- 2x2 FLOOR cluster条件を満たす外周cellへwarp flagを生成する
- EXIT <dir> の候補方向になる
```

ENTRYは、destination側の対応進入方向もallowedの場合のみ許可する。

### `board_edge_directions`

`board_edge_directions` に含まれる方向は、LRS destinationが存在しない。

```text
- warp flagを生成しない
- RIFT_BARRIERを生成しない
- 外周cellは通行可能
- EXIT <dir> は成立しない
```

board edgeは壁ではなく、外周cellの先に退出先が存在しない状態である。
そのため、playerはboard edge側の外周cellへ移動できるが、その方向へWARP_EXITしない。

### `rift_blocked_edges`

`rift_blocked_edges` に含まれる方向は、actual RIFT による双方向通過不能edgeである。

```text
- warp flagを生成しない
- 外周一列をRIFT_BARRIERにする
- 外周cell自体へ進入できない
- EXIT <dir> は成立しない
- 対応方向からのENTRYも成立しない
```

## EXIT command との関係

SRSからLRSへ移動するのは `EXIT <dir>` のみである。

`EXIT <dir>` は、少なくとも次を満たす場合に成功する。

```text
- 現在playerがいるcellがSRS map内にある
- 現在cellの warp_flags に <dir> が含まれる
- <dir> が allowed_exit_edges に含まれる
- destination側の対応進入方向もallowedである
- combat等、上位game loop上の移動禁止条件を満たしている
```

board edge方向は `warp_flags` を持たないため失敗する。
RIFT blocked edge方向も `warp_flags` を持たず、外周一列が `RIFT_BARRIER` であるため失敗する。

## known_routesとの関係

`known_routes` は発見済み情報・表示情報である。
WARP generation と WARP_EXIT は actual な3方向集合 contract を参照する。

```text
known_routes:
  発見済み情報・表示情報

allowed_exit_edges / board_edge_directions / rift_blocked_edges:
  actualなゲーム状態・SRS生成入力・WARP_EXIT判定入力
```

actual RIFTは未発見でも `rift_blocked_edges` に反映する。

## 実装反映先

この仕様は、主に次へ反映される。

```text
experiments/galactic_exodus/srs/generate.py
experiments/galactic_exodus/srs/test_generate.py
experiments/galactic_exodus/integrated_play.py
experiments/galactic_exodus/test_integrated_play.py
```

#1351 では Python model / validation / generation / fixture / test を新しい3方向集合 contract へ同期する。
#1344 では integrated adapter と `integrated_play.py` を同じ contract へ接続する。

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
#1088 の terrain-count generation profile は #1263 で deferred としている。
