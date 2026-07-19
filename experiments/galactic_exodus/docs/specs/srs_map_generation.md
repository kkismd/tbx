# Galactic Exodus SRS map generation specification

Source issues: #1085, #1086, #1088
Traceability audit: #1260
Follow-up: #1263, #1264, #1350, #1351, #1344

この文書は、Galactic Exodus Phase 2 SRS map generation の正本仕様と、現行prototypeで未実装として残す範囲を記録する。

## 目的

SRS map generation は、actual な LRS sector state から構築された `SectorDescriptor` を入力にして、9x9 の SRS local map、terrain、objects、warp flags、persistent generation metadata を作る。

現行prototypeでは、WARP / RIFT edge 表現とobject配置の一部は旧contractで実装済みである。
`spawn_position` と3方向集合contractへの同期は #1351 で実装する。
一方、#1088 で検討された SectorType別 terrain-count generation profile は、現行prototypeではまだ full implementation にしない。

## mapと座標

現行 `experiments/galactic_exodus/srs/generate.py` は、9x9 mapのみを生成する。

```text
MAP_WIDTH  = 9
MAP_HEIGHT = 9
MAP_CENTER = internal Position(4, 4)
```

SRS internal coordinate は 0-origin lower-left である。
CLI表示座標は 1-origin lower-left である。

```text
internal Position(0, 0) == CLI display (1, 1)
x increases eastward
y increases northward
```

新規ゲーム開始時の player 位置は次で固定する。

```python
spawn_position = Position(0, 0)
```

通常sector遷移では、sourceの退出方向とdestinationの進入側は反対方向になる。

```text
source EXIT N -> destination ingress S
source EXIT E -> destination ingress W
source EXIT S -> destination ingress N
source EXIT W -> destination ingress E
```

9x9 mapの進入位置は次で固定する。

```python
ENTRY_POSITIONS = {
    Direction.N: Position(4, 8),
    Direction.E: Position(8, 4),
    Direction.S: Position(4, 0),
    Direction.W: Position(0, 4),
}
```

`spawn_position` は descriptor の必須fieldであり、進入辺から暗黙導出しない。

## SectorDescriptor contract

SRS生成の正本入力は次の contract とする。

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
未分類方向および重複分類は不正とする。

### `allowed_exit_edges`

`allowed_exit_edges` は、actual に隣接sectorが存在し、RIFTで遮断されていない方向を表す。

```text
- 対応方向の外周cellは通常terrain
- 2x2 FLOOR cluster条件を満たす外周cellへwarp flagを生成する
- 現在sectorからその方向へのEXITを許可する
- destination側の対応進入方向もallowedの場合のみENTRYを許可する
```

### `board_edge_directions`

`board_edge_directions` は、LRS board外で隣接sectorが存在しない方向を表す。

```text
- 外周cellは通行可能
- RIFT_BARRIERを生成しない
- warp flagを生成しない
- WARP_EXITしない
```

board edgeは壁ではなく、外周cellの先に退出先が存在しない状態である。
たとえば新規ゲーム開始位置 `Position(0, 0)` はsouth / west側の外周cell上に置けるが、その方向がboard edgeであればEXITは成立しない。

### `rift_blocked_edges`

`rift_blocked_edges` は、actual RIFTによる遮断方向を表す。

```text
- EXIT不可
- 対応方向からのENTRY不可
- 外周一列すべてをRIFT_BARRIERにする
- warp flagを生成しない
- 外周cell自体へ進入できない
```

RIFT blocked edgeは双方向通過不能である。

## SectorType.RIFT consistency

`sector_type` は具体的な遮断方向を決めない。
遮断方向の正本は `rift_blocked_edges` とする。

```text
- SectorType.RIFT のsectorは、必ず1方向以上のrift_blocked_edgesを持つ
- SectorType.RIFT 以外のsectorは、rift_blocked_edgesを持たない
- sector_typeから具体的な遮断方向を推測しない
```

## 生成手順

正本仕様としての生成手順は次の通りである。

```text
1. SectorDescriptorを検証する
2. 全cellを FLOOR として初期化する
3. rift_blocked_edges に対応する外周一列へ RIFT_BARRIER を配置する
4. allowed_exit_edges の外周cellへ、2x2 FLOOR cluster条件に従って warp_flags を付与する
5. spawn_position にplayerを置く
6. STAR / PLANET / sector type別extra objectを配置する
7. persistent generation metadataを保存する
```

object配置候補、warp flag生成、terrain配置は同じ3方向集合 contract を参照する。

## validation責務

共通descriptor / generation validationは次を保証する。

```text
- spawn_positionがmap内
- spawn_positionが生成後に通行可能
- spawn_positionがRIFT_BARRIER上ではない
- spawn_positionがobjectと重ならない
- object配置候補からspawn_positionを除外する
- 3方向集合が相互排他的かつ全方向を網羅する
- SectorType.RIFTとrift_blocked_edgesが整合する
```

通常遷移として正しい外縁位置か、source/destination双方が通過可能かは integrated adapter の責務とする。

## known_routesとの関係

`known_routes` は発見済み情報・表示情報である。
3方向集合は actual なゲーム状態・SRS生成入力である。

```text
LRS actual state
  -> integrated adapter
  -> SectorDescriptor
  -> SRS generation
```

`known_routes` を actual generation 入力に使用しない。
actual RIFTは未発見でも `rift_blocked_edges` に反映する。
SRS descriptorからLRS route表示を逆算するhelperは設けない。

## persistent metadata

persistent metadataへ次を直接保存する。

```text
sector_type
generation_seed
spawn_position
allowed_exit_edges
board_edge_directions
rift_blocked_edges
```

restore時にも、metadataとdescriptorの一致、3集合の排他性・網羅性、`SectorType.RIFT` と `rift_blocked_edges` の整合性を検証する。

generation schema versionは #1351 で更新する。
旧generation schemaのrestoreは明示的にrejectする。
旧形式を推測変換する互換readerは追加しない。
既存fixtureは #1351 で新schemaへ一括migrationする。

## object placement

Phase 2 prototypeでは、各sectorに次のobjectを配置する。

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
- spawn_positionではない
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
- #1350 の sector boundary contract と合わせて扱う方が安全である
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

## 実装反映先と後続issue

この文書更新では、Pythonコード、fixture、runtime test、snapshot、balance値を変更しない。

#1351 では次を同時に扱う。

```text
- Python model / validation / generation の更新
- generation schema version 更新
- restore時の新metadata検証と旧schema reject
- fixture一括migration
- runtime test / snapshot更新
```

#1344 では次を扱う。

```text
- integrated adapterでactual LRS stateからSectorDescriptorを構築する
- 通常sector遷移時のsource/destination通過可否を検証する
- integrated_play.pyへ新descriptor contractを接続する
```
