# Galactic Exodus Phase 2 SRS Specification

## 1. Status and authority

この文書は、Phase 2 の SRS 移動・探索ルールの正本仕様書である。人間が読む断定形の仕様として扱い、`experiments/galactic_exodus/srs/phase2_decisions.csv` と矛盾しない内容で固定する。

Python 実装は実行可能な参照実装であり、仕様の正本ではない。仕様書と Python 実装が矛盾する場合は Python 実装を修正する。TBX 実装、reference fixture、validator はこの文書と `phase2_decisions.csv` を起点に整合させる。

本書が対象にするのは、LRS から対象 sector へ入った後に始まる SRS の移動・探索である。戦闘、threat、encounter、salvage の追加効果、最終表示レイアウトは対象外とし、必要な引き継ぎ先は後続 issue を明記する。

## 2. Scope and baseline

Phase 2 の正本対象盤面は 9x9 だけである。11x11 比較は採用判断に必要なデータが不足しているため、正本へ入れない。

baseline は次の契約で固定する。

```text
cost_mode = TURN_ONLY
movement_rule = MOVEMENT_POINTS
movement_points_per_turn = 4
path_input_mode = ROUTE_PREVIEW
interaction_mode = EXPLICIT_INTERACT
collision_behavior = STOP_BEFORE
observation_mode = LOCAL_MOVEMENT
max_srs_turns = 40
```

`SHARED_FUEL`、`VECTOR_COMMAND`、`DIRECTIONAL_THRUST` は比較条件としてのみ残し、Phase 2 の正本条件には採用しない。

## 3. Data model and coordinate system

SRS はローカル整数座標の 9x9 盤面で扱う。座標は `Position(x, y)` とし、`x` は東に進むほど増え、`y` は北に進むほど増える。方角列は `Direction = N | E | S | W` で表す。

SRS の地形・オブジェクト・actor の語彙はコード上の enum 名に合わせて固定する。

```text
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
```

各 cell は `terrain`、任意の `object_id`、任意の `actor_id`、`warp_flags: set[Direction]` を持つ。`warp_flags` はその cell から出られる辺を直接表す。

`SectorDescriptor` は少なくとも次を持つ。

```text
sector_id
sector_type
sector_seed
entry_edge
blocked_edges
```

## 4. Sector type and SRS map relationship

LRS でモデル化対象の sector に入った時点で、必ず SRS を開始する。省略条件や自動解決条件は導入しない。

`SectorType` と SRS 上の主な意味づけは次のとおりとする。

| SectorType | SRS 上の主役 |
| --- | --- |
| `NORMAL` | 通常探索区画。`SALVAGE` placeholder を置ける。 |
| `BASE` | `STATION` を持つ補給区画。 |
| `RESOURCE` | `RESOURCE_CACHE` を持つ補給区画。 |
| `NEBULA` | 観測半径だけが 3x3 に縮む区画。 |
| `ASTEROID` | `ASTEROID` / `ASTEROID_FIELD` を含む危険区画。 |
| `GRAVITY` | gravity field を持つ比較用区画。 |
| `RIFT` | `blocked_edges` と `RIFT_BARRIER` を持つ区画。 |

生成 profile、配置制約、要素属性の正本は `phase2_srs_elements.json` と `phase2_srs_generation.json` に置き、本書では移動・探索に必要な外部契約だけを固定する。

## 5. Entry and exit mapping

LRS から SRS へ入る時、sector 側には `entry_edge` が与えられる。entry / exit mapping は `warp_flags` と反対辺 return 候補で表現する契約を維持する。

各 cell の `warp_flags` は、その cell から外へ出られる方角を表す。SRS から LRS へ戻る操作は `WARP_EXIT` だけであり、現在位置の cell が指定方角の `warp_flags` を持つ時にだけ受理する。

対象方角の return 候補が存在しない map は生成エラーであり、実行時 fallback は持たない。corner cell は複数の `warp_flags` を保持してよい。

## 6. Warp points, blocked edges, and RIFT representation

warp point は専用オブジェクトではなく、`warp_flags` を持つ passable cell で表す。`render.py` 上の可視表現は `^ > v < +` を使うが、正本契約は表示記号ではなく `warp_flags` の集合である。

`blocked_edges` は sector 単位の永続契約であり、`RIFT` sector の移動制約と銀河外縁の退出拒否に使う。blocked edge の方向には warp flag を付与しない。

`RIFT` の内部表現は次の 2 層で固定する。

```text
1. sector descriptor の blocked_edges
2. map 上の impassable terrain としての RIFT_BARRIER
```

`blocked_edges` は「その方角へ sector 外へ出られない」ことを表し、`RIFT_BARRIER` は「その cell に進入できない」ことを表す。両者は別契約であり、相互に代替しない。

blocked edge の表示改善は移動ルールの変更ではなく UI の仕事として #1076 で扱う。

## 7. Object placement and lifecycle

SRS 上の主要 object は `STAR`、`PLANET`、`STATION`、`RESOURCE_CACHE`、`SALVAGE` である。

`STAR`、`PLANET`、`STATION` は impassable object として扱う。`RESOURCE_CACHE` と `SALVAGE` は passable object として同じ cell に進入できる。

### RESOURCE_CACHE

`RESOURCE_CACHE` は `EXPLICIT_INTERACT` 前提の same-cell object とする。sector 全体で最大 `+5` 相当の補給価値を持ち、cache 数ごとの分割は次で固定する。

```text
1 cache: +5
2 caches: +3 / +2
3 caches: +2 / +2 / +1
```

回復量は `max_fuel` で clamp する。実回復量が 0 の場合、その cache は消費済みにしない。消費済み状態は `persistent_state.consumed_object_ids` に保持し、再訪時も維持する。

### STATION

`STATION` は `EXPLICIT_INTERACT` 前提の adjacent object とする。受理された interaction は fuel を `max_fuel` まで回復させる。`STATION` は消滅せず、再利用可能である。利用済み状態は `persistent_state.activated_object_ids` に保持する。

### SALVAGE

`SALVAGE` は `EXPLICIT_INTERACT` 前提の same-cell placeholder object とする。受理された interaction は `consumed_object_ids` に記録し、再訪時も取得済みを維持する。戦闘・装備・修理・threat への接続効果は #1167 で設計する。

## 8. Movement commands

Phase 2 の正本 command surface は `MOVE_ROUTE`、`MOVE_TO`、`INTERACT`、`WARP_EXIT` である。

`MOVE_ROUTE` は `N/E/S/W` の方向列を受け取る。`MOVE_TO` は known state 上の target cell を受け取り、解決された route を内部で生成する。`MOVE_TO` の pathfinding は known state だけを使い、unknown cell を経路に含めない。

`MOVE_TO` の tie-break は次の順で固定する。

```text
1. total raw cost が最小
2. step 数が最小
3. N/E/S/W の方向列として辞書順最小
```

無効 target、未知 target、経路なし、1 歩も進めない route は rejected command とし、SRS turn を消費しない。

## 9. Movement cost, collision, and abort behavior

Phase 2 baseline は `MOVEMENT_POINTS` 方式を使う。raw cost は次の単位で固定する。

```text
orthogonal_raw_cost = 10
diagonal_raw_cost = 14
movement_cost_budget_raw = 40
movement_points_per_turn = 4
```

地形 multiplier は baseline で次を使う。

```text
FLOOR = 1
DEBRIS = 2
NEBULA = 2
ASTEROID_FIELD = 3
GRAVITY_FIELD_VERTICAL = 1
GRAVITY_FIELD_HORIZONTAL = 1
RIFT_DISTORTION = 1
```

`TURN_ONLY` では accepted command が SRS turn だけを進め、LRS fuel を直接減らさない。`SHARED_FUEL` は比較条件に留め、実際に通過した raw movement cost の合計を 10 で割り上げて fuel 消費とする。

collision behavior は `STOP_BEFORE` で固定する。impassable terrain は `ASTEROID` と `RIFT_BARRIER`、impassable object は `STAR`、`PLANET`、`STATION` である。

最初の対象 cell が impassable だった場合は、その cell へ進入せず、位置は変わらず、movement cost は 0、観測更新も行わない。これは accepted movement command として 1 SRS turn を消費する。

途中まで進んだ後に impassable cell に当たった場合は、最後に進入できた passable cell で停止する。衝突 cell 自体の movement cost は消費しない。移動結果は partial move として記録する。

`MOVE_ROUTE` の prefix が 1 歩も実行できない budget 超過は rejected command とし、turn を消費しない。accepted command の解決後に `max_srs_turns = 40` の上限判定を行う。

## 10. Observation and known state

Phase 2 baseline の観測方式は `LOCAL_MOVEMENT` である。`FULL` は比較用定義として残すが、正本条件では使わない。

観測契約は次で固定する。

```text
default observation = 5x5
NEBULA observation = 3x3
observation center = each successful destination cell
known map = cumulative
failed or rejected command = no observation update
first blocked cell collision = no observation update
```

`NEBULA` 上でだけ 3x3 観測を適用する。追加の報酬や threat 軽減は持たせない。NEBULA 固有の価値づけは #1167 で扱う。

known state は `discovered_cells`、`known_cells`、`visited_cells` で表す。比較対象として永続化するのは `discovered_cells` と、そこから復元できる known map である。

## 11. INTERACT command

baseline の interaction mode は `EXPLICIT_INTERACT` である。自動取得や自動補給は行わない。

`INTERACT` は `target_object_id` を明示指定する。unknown object、unsupported object、range 不一致、既取得 object、効果 0 は rejected command とし、SRS turn を消費しない。

受理された `INTERACT` は常に 1 SRS turn を消費する。event payload には少なくとも `object_id`、`object_type`、`interaction_range`、`effect`、`fuel_before`、`fuel_after`、`fuel_delta`、`outcome` を含める。

## 12. WARP_EXIT command

`WARP_EXIT` は `exit_direction` を取る。現在位置の cell がその方向の `warp_flags` を持ち、かつその方向が `blocked_edges` でも銀河外縁でもない時にだけ accepted とする。

次の場合は rejected とし、SRS turn を消費しない。

```text
- 現在位置が map 外
- exit_direction が blocked_edges に含まれる
- 現在位置 cell に対応 warp flag がない
```

accepted `WARP_EXIT` は 1 SRS turn を消費し、LRS へ戻る唯一の出口になる。LRS/SRS 遷移は sector entry と accepted `WARP_EXIT` だけで発生する。

## 13. Persistent state and revisit contract

同一 sector を再訪する時は、次の persistent state を保持する。

```text
generated_map_id
generation_schema_version
generation_seed
sector_type
blocked_edges
warp_flags
celestial_body_positions
consumed_object_ids
activated_object_ids
discovered_cells
```

同じ `sector_id` と `sector_seed` の再訪では、warp flag、既知状態、利用済み object 状態を維持する。`RESOURCE_CACHE` や `SALVAGE` は取得済みなら再取得できない。`STATION` は再利用できる。

再訪時の state 復元は Python 実装の内部詳細ではなく、この永続フィールド集合を比較可能に保つ契約として扱う。

## 14. Turn, fuel, outcomes, and command rejection

SRS turn を消費するのは accepted command だけである。baseline では次の 3 種が各 1 turn を消費する。

```text
movement command
INTERACT
WARP_EXIT
```

無効入力、拒否された command、実回復量 0 の interaction、blocked edge への `WARP_EXIT` は turn を消費しない。

SRS 実行の代表 outcome は少なくとも次を持つ。

```text
ACCEPTED
STOPPED_BEFORE_IMPASSABLE
REJECTED_ZERO_STEP
REJECTED_UNKNOWN_TARGET
REJECTED_NO_PATH
REJECTED_UNKNOWN_OBJECT
REJECTED_WRONG_RANGE
REJECTED_ALREADY_CONSUMED
REJECTED_NO_EFFECT
REJECTED_BLOCKED_EDGE
REJECTED_NO_WARP_FLAG
```

これらは UI 文言ではなく、GameLog と TBX state が比較すべき外部契約名として扱う。

## 15. GameLog and TBX state contract

GameLog は Python の補助ログではなく、reference fixture と TBX 移植が突き合わせるための比較面である。最低限、各 event は `srs_turn` と `event_type` を持つ。

required event types は次で固定する。

```text
MOVE_ACCEPTED
MOVE_REJECTED
STOPPED_BEFORE_IMPASSABLE
INTERACT_ACCEPTED
INTERACT_REJECTED
OBJECT_CONSUMED
STATION_ACTIVATED
WARP_EXIT_ACCEPTED
WARP_EXIT_REJECTED
OBSERVATION_UPDATED
```

movement event は少なくとも `command_type`、`movement_rule`、`cost_mode`、`start_position`、`end_position`、`entered_cells`、`movement_raw_cost`、`fuel_delta`、`observation_updates`、`outcome` を含む。`MOVE_TO` は追加で `target_position` と `resolved_route` を含む。

`WARP_EXIT` event は少なくとも `exit_direction`、`start_position`、`warp_position`、`sector_id`、`generated_map_id`、`outcome` を含む。`INTERACT` event は object 識別子と燃料変化を含む。

TBX state は GameLog と同じ観測・永続・退出契約を比較できる形で保持する。少なくとも次を比較対象に含める。

```text
srs_turn
fuel
max_fuel
player_position
consumed_object_ids
activated_object_ids
discovered_cells または discovered_count と復元可能な known map
entry / exit 情報
```

reference fixture と validator の詳細作成は #1165 で扱うが、比較対象の外部契約は本節で固定する。

## 16. Python / TBX parity target

Phase 2 で Python と TBX が一致すべき対象は、実装内部ではなく外部観測可能な契約である。少なくとも次の一致を要求する。

```text
- command acceptance / rejection と outcome 名
- SRS turn の進み方
- movement raw cost と fuel delta
- 観測更新の有無と discovered state
- object lifecycle
- persistent state
- WARP_EXIT の受理条件と sector 離脱タイミング
```

Python 実装は parity を説明する資料ではなく、上記契約を満たす reference implementation として扱う。

## 17. Display-input contract for #1076

#1076 へ渡す入力契約の要点は次のとおりである。

```text
1. 表示対象は actual map ではなく known map である
2. 未発見 cell は unknown と区別できる必要がある
3. 現在位置、warp_flags、passable value object、impassable celestial bodies を区別できる必要がある
4. consumed / activated object state を表示側で識別できる必要がある
5. blocked edge の存在を player が誤解しにくい形で示す必要がある
6. fuel_before / fuel_after / fuel_delta と outcome を command 単位で表示できる必要がある
7. entry / exit 方向と sector 離脱イベントを表示できる必要がある
```

現在の Python 表示では `@ * o S R $ r s ^ > v < + ?` を使っているが、表示記号そのものは #1076 で再設計してよい。固定するのは「どの状態差分を UI へ渡すか」という入力契約だけである。

## 18. Deferred topics

本仕様で確定しないが follow-up issue が明示されている項目は次のとおりとする。

```text
- blocked edge の表示改善: #1076
- reference fixture / validator の整備: #1165
- 表示入力契約の具体 UI 化: #1076 と #1166
- NEBULA 追加価値、SALVAGE 効果、threat / encounter / combat 接続: #1167
```

これらの後続作業は Phase 2 の移動・探索正本契約を変更しない前提で行う。
