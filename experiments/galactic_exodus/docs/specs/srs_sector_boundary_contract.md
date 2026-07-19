# Galactic Exodus SRS sector boundary contract

Source issue: #1350
Parent: #1178
Related: #1264, #1344, #1349, #1351
Base branch: `integration/882-galactic-exodus`

この文書は、SRS sector生成、通常sector遷移、WARP/EXIT判定、fixture、restoreが共有するsector境界データ契約の正本である。

## 1. 正本データ契約

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

次の旧fieldは正本契約から廃止する。

```text
entry_edge
spawn_edge
blocked_edges
```

`spawn_position` は必須fieldであり、辺から暗黙導出しない。

## 2. 座標規則

```text
SRS内部座標:
  0-origin
  左下が Position(0, 0)

CLI表示座標:
  1-origin
  左下が (1, 1)
```

新規ゲーム開始位置は次で固定する。

```python
spawn_position = Position(0, 0)
```

CLIでは `(1, 1)` と表示する。

## 3. 退出方向の完全分類

N/E/S/Wの各方向は、必ず次のいずれか1つに属する。

```text
allowed_exit_edges
board_edge_directions
rift_blocked_edges
```

3集合は相互排他的であり、和集合は全方向と一致しなければならない。

```python
ALL_DIRECTIONS = frozenset(Direction)

allowed_exit_edges.isdisjoint(board_edge_directions)
allowed_exit_edges.isdisjoint(rift_blocked_edges)
board_edge_directions.isdisjoint(rift_blocked_edges)

(
    allowed_exit_edges
    | board_edge_directions
    | rift_blocked_edges
) == ALL_DIRECTIONS
```

未分類方向、または複数集合へ重複所属する方向は不正とする。

## 4. 各方向分類の意味

### 4.1 `allowed_exit_edges`

```text
- 対応方向の外周cellは通常terrainのまま
- 対応方向の外周cellへwarp flagを生成する
- 現在sectorからその方向へのEXITを許可する
- 隣接sector側でも対応する進入方向がallowedである場合のみENTRYを許可する
```

### 4.2 `board_edge_directions`

```text
- LRS board外であり隣接sectorは存在しない
- 対応方向の外周cellは通行可能
- RIFT_BARRIERを生成しない
- warp flagを生成しない
- map外への移動操作でWARP_EXITしない
```

board edgeは壁ではなく、外周cellの先に退出先が存在しない状態である。

### 4.3 `rift_blocked_edges`

```text
- actual RIFTによる遮断方向
- 現在sectorからその方向へのEXIT不可
- 隣接sectorからその方向を進入側とするENTRY不可
- 対応方向の外周一列すべてをRIFT_BARRIERにする
- warp flagを生成しない
- 外周cell自体へ進入できない
```

RIFT blocked edgeは双方向通過不能である。

## 5. `SectorType.RIFT` の整合性

```text
- SectorType.RIFT のsectorは、必ず1方向以上のrift_blocked_edgesを持つ
- SectorType.RIFT 以外のsectorは、rift_blocked_edgesを持たない
```

具体的な遮断方向を `sector_type` から推測してはならない。遮断方向の正本は `rift_blocked_edges` とする。

## 6. spawn位置のvalidation責務

共通descriptor / generation validationでは、少なくとも次を保証する。

```text
- spawn_positionが生成map内にある
- spawn_positionが生成後に通行可能である
- spawn_positionがRIFT_BARRIER上ではない
- spawn_positionがSTAR / PLANET / STATION等のobjectと重ならない
- ランダムobject配置候補からspawn_positionを除外する
```

通常sector遷移として正しい外縁位置かどうかは、共通descriptor validationでは検証しない。これはintegrated adapterの責務とする。

新規ゲーム開始は通常sector遷移ではないため、外縁中央への強制を受けない。

## 7. 通常sector遷移

source sectorの退出方向とdestination sectorの進入側は反対方向になる。

```text
source EXIT N -> destination ingress S
source EXIT E -> destination ingress W
source EXIT S -> destination ingress N
source EXIT W -> destination ingress E
```

9x9 SRS mapにおけるdestination側の進入位置は次で固定する。

```python
ENTRY_POSITIONS = {
    Direction.N: Position(4, 8),
    Direction.E: Position(8, 4),
    Direction.S: Position(4, 0),
    Direction.W: Position(0, 4),
}
```

mappingのkeyはdestination側の進入方向である。

integrated adapterは遷移確定前に次を検証する。

```text
- sourceの退出方向がsource.allowed_exit_edgesに含まれる
- destinationの進入方向がdestination.allowed_exit_edgesに含まれる
- destination spawn_positionが進入方向のENTRY_POSITIONSと一致する
```

sourceまたはdestinationの対象方向が `board_edge_directions` または `rift_blocked_edges` に属する場合、遷移を開始してはならない。

## 8. `known_routes` との関係

```text
known_routes:
  発見済み情報・表示情報

allowed_exit_edges / board_edge_directions / rift_blocked_edges:
  actualなゲーム状態・SRS生成入力
```

`known_routes` をSRS actual generationの入力には使用しない。

actual RIFTは未発見でも `rift_blocked_edges` に反映する。

## 9. persistent metadata

persistent metadataにはdescriptorと同じ境界契約を保存する。

```python
@dataclass(frozen=True, slots=True)
class SrsPersistentState:
    generated_map_id: str
    generation_schema_version: int
    generation_seed: int
    sector_type: SectorType
    spawn_position: Position
    allowed_exit_edges: frozenset[Direction]
    board_edge_directions: frozenset[Direction]
    rift_blocked_edges: frozenset[Direction]
```

descriptorとpersistent metadataの次の値は一致必須とする。

```text
sector_type
sector_seed / generation_seed
spawn_position
allowed_exit_edges
board_edge_directions
rift_blocked_edges
```

3集合の排他性・全方向網羅性はrestore時にもvalidationする。

## 10. schema・fixture・restore移行

```text
- generation schema versionを更新する
- 旧generation schemaのrestoreは明示的にrejectする
- 旧形式を新形式へ推測変換する互換readerは追加しない
- 既存fixtureは新schemaへ一括migrationする
```

旧形式からはboard edgeとRIFTの原因区別、および正確なspawn位置を情報損失なしで復元できないため、後方互換変換は行わない。

## 11. helperと依存方向

`derive_lrs_blocked_routes(...)` は削除する。

情報の流れは次の一方向とする。

```text
LRS actual state
  -> integrated adapter
  -> SectorDescriptor
  -> SRS generation
```

SRS descriptorからLRS route表現を逆算するhelperは設けない。

## 12. 既存文書の更新指示

### 12.1 `srs_map_generation.md`

削除または置換する記述:

```text
- descriptor.blocked_edges に対応する外周へRIFT_BARRIERを配置する
- non-blocked edgeへwarp_flagsを付与する
- entry_edgeに対応する固定初期位置へplayerを置く
- board edgeをblocked edgeと同等に扱う
- board edgeへRIFT_BARRIER相当の通行不能外周を置く
- allowed_exit_edges / blocked_exit_edges の選択肢
- blocked_edgesへboard edgeを含める互換案
```

新たに記載する内容:

```text
- spawn_positionと3方向集合を正本入力とする
- board edgeは通行可能外周、warpなし、RIFT_BARRIERなし
- RIFT blocked edgeだけがRIFT_BARRIERを生成する
- N/E/S/Wの完全分類validation
- SectorType.RIFTの整合性
- persistent metadataとschema移行方針
```

### 12.2 `srs_warp.md`

削除または置換する記述:

```text
- blocked edgeという単一分類
- non-blocked edgeならwarp可能という表現
- known blocked edgeをEXIT成功条件に含める表現
```

新たに記載する内容:

```text
- warp flag生成対象はallowed_exit_edgesだけ
- board_edge_directionsはwarpなしだが通行可能
- rift_blocked_edgesはwarpなし、RIFT_BARRIER、ENTRY/EXITとも不可
- actual状態とknown_routesの責務分離
```

### 12.3 `integrated_cli.md`

削除または置換する記述:

```text
- descriptor.blocked_edgesを直接参照する説明
- known RIFT edgeを正本の移動可否判定とする説明
- full generation統合後も全edgeへwarp candidateを付与する前提
```

新たに記載する内容:

```text
- source.allowed_exit_edgesとdestination.allowed_exit_edgesを両方検証する
- destination ingress方向とspawn_positionの対応
- board外rejectとRIFT rejectは防御的validationとして残せる
- 表示上は生成時点で不可能方向にwarp flagを出さない
```

## 13. 対象外

本仕様更新では次を変更しない。

```text
- Python model / validation / generation実装
- integrated_play.py
- fixture
- runtime test
- snapshot
- gameplay balance
```

実装は #1351、integrated接続は #1344 で扱う。
