# Galactic Exodus Phase 2 SRS Specification

## 1. Status and authority

この文書は、Phase 2 SRS の正本仕様書である。`experiments/galactic_exodus/srs/phase2_decisions.csv` と矛盾しない断定形の仕様として扱い、Python prototype・reference fixture・TBX 実装は本書を起点に整合させる。

Python 実装は参照実装であり、正本ではない。仕様と prototype が矛盾した場合は prototype を修正する。`phase2_reference.json` と `validate_phase2_results.py` は、本書で固定した外部契約を replay 可能な形で検証する。

## 2. Scope and deferred items

Phase 2 SRS reference が正本として含む対象は次のとおりである。

```text
- movement / observation / interaction / warp exit
- combat initial model
- encounter initial model
- reward initial model
```

Phase 2 では次を deferred として扱う。

```text
- advanced combat probability / tuning (#1195)
- final display layout and glyph design (#1076)
- TBX runtime implementation
- new balance adjustments or new random rules
```

## 3. Baseline

正本 baseline は 9x9 SRS とし、比較条件ではなく正式採用する契約は次で固定する。

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

`SHARED_FUEL` は比較条件として残すが、正本 baseline ではない。

## 4. Data model and coordinate system

SRS はローカル整数座標の 9x9 盤面で扱う。座標は `Position(x, y)` とし、`x` は東に進むほど増え、`y` は北に進むほど増える。方角列は `Direction = N | E | S | W` で表す。

座標契約は internal coordinate と display coordinate の 2 層に分ける。

```text
internal coordinate:
  used by Python prototype, fixtures, validators, tests, and engine event payloads
  origin = lower-left
  x increases eastward
  y increases northward
  coordinates are 0-based
  9x9 valid range = (0,0) ... (8,8)

display coordinate:
  used by render output, HUD, manual evaluation text, docs, and display samples
  origin = lower-left
  x increases eastward
  y increases northward
  coordinates are 1-based
  9x9 display range = (1,1) ... (9,9)
```

`Position(x, y)` は internal coordinate を表す。display coordinate が必要な箇所では、internal `Position` から明示的に変換する。

```text
display_x = internal_x + 1
display_y = internal_y + 1

internal_x = display_x - 1
internal_y = display_y - 1
```

Python prototype の `SrsActualMap.cells` は internal y 軸と同じ順で保持する。

```text
cells[0] = y=0 の south row
cells[height - 1] = y=height - 1 の north row
```

したがって、internal `Position` から cell へアクセスする時は `cells[position.y][position.x]` を使う。表示時だけ north-to-south に描画するため、`y=height-1` から `y=0` へ降順に走査する。

9x9 baseline の edge / warp point は internal coordinate では次になる。

```text
N edge: y = 8
S edge: y = 0
E edge: x = 8
W edge: x = 0

N warp point: Position(4,8)
E warp point: Position(8,4)
S warp point: Position(4,0)
W warp point: Position(0,4)
```

同じ点を display coordinate で表示する場合は次になる。

```text
N warp point: (5,9)
E warp point: (9,5)
S warp point: (5,1)
W warp point: (1,5)
```

`RIFT_BARRIER` は blocked edge に対応する外縁 cell に置く。

```text
N blocked: internal y=height-1 row / display y=height row
S blocked: internal y=0 row / display y=1 row
E blocked: internal x=width-1 column / display x=width column
W blocked: internal x=0 column / display x=1 column
```

固定語彙は次の enum 名に合わせる。

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

各 cell は `terrain`、任意の `object_id`、任意の `actor_id`、`warp_flags: set[Direction]` を持つ。`SectorDescriptor` は少なくとも `sector_id`、`sector_type`、`sector_seed`、`entry_edge`、`blocked_edges` を持つ。

## 5. Sector meaning

| SectorType | SRS 上の主役 |
| --- | --- |
| `NORMAL` | 通常探索区画。`SALVAGE` を置ける。 |
| `BASE` | `STATION` を持つ補給区画。 |
| `RESOURCE` | `RESOURCE_CACHE` を持つ補給区画。 |
| `NEBULA` | 観測半径が 3x3 に縮み、encounter chance に 0.7 modifier をかける区画。 |
| `ASTEROID` | `ASTEROID` / `ASTEROID_FIELD` を含む危険区画。 |
| `GRAVITY` | gravity field を持つ比較用区画。 |
| `RIFT` | `blocked_edges` と `RIFT_BARRIER` を持つ区画。 |

生成 profile と配置制約の正本は `phase2_srs_generation.json` に置き、本書では移動・戦闘・報酬に必要な外部契約だけを固定する。

## 6. Entry and exit mapping

LRS で対象 sector へ入った時点で、必ず SRS を開始する。省略条件や自動解決条件は導入しない。

各 cell の `warp_flags` は、その cell から外へ出られる方角を表す。SRS から LRS へ戻る操作は `WARP_EXIT` だけであり、現在位置の cell が指定方角の `warp_flags` を持つ時にだけ受理する。

`blocked_edges` は sector 単位の永続契約であり、blocked 方向への warp flag は付与しない。return 候補が存在しない map は生成エラーとし、実行時 fallback は持たない。

## 7. Object placement and lifecycle

SRS 上の主要 object は `STAR`、`PLANET`、`STATION`、`RESOURCE_CACHE`、`SALVAGE` である。

`STAR`、`PLANET`、`STATION` は impassable object、`RESOURCE_CACHE` と `SALVAGE` は passable object とする。

### RESOURCE_CACHE

`RESOURCE_CACHE` は same-cell の explicit interact object であり、固定で fuel を `+3` 回復する。回復量は `fuel_capacity` で clamp する。

```text
range = SAME_CELL
effect = fixed fuel +3
persistent field = consumed_object_ids
0 actual recovery = rejected and not consumed
```

`fuel_after == fuel_before` の場合は `REJECTED_NO_EFFECT` とし、消費済みにしない。

### STATION / BASE

`STATION` は adjacent の explicit interact object であり、再利用可能である。

```text
range = ADJACENT
effect = full recovery of fuel / durability / energy / photon_torpedo_ammo
persistent field = activated_object_ids
reusable = true
```

`BASE` では station interaction 時に salvage を通貨として upgrade を購入できる。Phase 2 で利用可能な upgrade は `PHASER_POWER`、`PHOTON_TORPEDO_POWER`、`ENERGY_CAPACITY`、`PHOTON_TORPEDO_AMMO_CAPACITY`、`DEFENSE`、`EVASION` である。

### SALVAGE

`SALVAGE` は same-cell の explicit interact object であり、取得時に salvage inventory を増やし、同時に即時回復 choice を 1 つ適用できる。

```text
range = SAME_CELL
reward source = MAP_PICKUP
salvage value = +1 inventory
choices =
  RECOVER_DURABILITY
  RECOVER_ENERGY
  RECOVER_PHOTON_TORPEDO_AMMO
  STORE_ONLY
persistent field = consumed_object_ids
```

即時回復量は durability / energy / photon torpedo ammo を capacity で clamp する。取得済み `SALVAGE` は再訪時も再取得できない。

## 8. Movement commands

Phase 2 の正本 command surface は次で固定する。

```text
MOVE_ROUTE
MOVE_TO
INTERACT
WARP_EXIT
WAIT
COMBAT_STEP
```

`MOVE_TO` は known state だけを使って path を作る。tie-break は次の順で固定する。

```text
1. total raw cost が最小
2. step 数が最小
3. N/E/S/W の方向列として辞書順最小
```

## 9. Movement cost, collision, and observation

raw cost は次で固定する。

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

collision behavior は `STOP_BEFORE` で固定する。impassable terrain は `ASTEROID` と `RIFT_BARRIER`、impassable object は `STAR`、`PLANET`、`STATION` である。

観測契約は次で固定する。

```text
default observation = 5x5
NEBULA observation = 3x3
observation center = each successful destination cell
known map = cumulative
failed or rejected command = no observation update
first blocked-cell collision = no observation update
```

## 10. Turn, fuel, and outcomes

accepted command だけが SRS turn を消費する。baseline では次が各 1 turn を消費する。

```text
movement command
INTERACT
WARP_EXIT
WAIT
```

`COMBAT_STEP` は combat phase を進めるが SRS turn は進めない。combat turn は `ENEMY_ACTION` 解決後にだけ `+1` する。

baseline の `TURN_ONLY` では LRS fuel を直接減らさない。`SHARED_FUEL` は実際に通過した raw movement cost の合計を 10 で割り上げて fuel 消費とする。

代表 outcome は少なくとも次を持つ。

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
REJECTED_ENEMY_PRESENCE
ENCOUNTER_STARTED
```

## 11. Combat initial model

combat state は `PLAYER_MOVEMENT -> PLAYER_ATTACK -> ENEMY_ACTION -> PLAYER_MOVEMENT` の phase 遷移を持つ。`PLAYER_MOVEMENT` で target が attackable なら `PLAYER_ATTACK`、attackable でなければ `ENEMY_ACTION` へ進む。

player の初期 combat resource は少なくとも次を持つ。

```text
durability = 100
energy = 6
photon_torpedo_ammo = 6
energy_recovery = 1
```

Phase 2 の weapon profile は次で固定する。

```text
PHOTON_TORPEDO:
  damage = 3
  range = 3
  ammo_cost = 1

PHASER:
  damage = 1
  range = 2
  energy_cost = 1

ENEMY_WEAPON:
  tierごとの固定 damage / range を使用
```

player attack では line of sight と range を満たした target だけを攻撃できる。`PHOTON_TORPEDO` は ammo を消費し、`PHASER` は energy を消費する。

enemy action では tier 昇順で行動順を固定し、attack 可能なら attack、そうでなければ shortest path で接近する。reaction は `COUNTERATTACK` または `DEFEND` を選べるが、counterattack に必要な energy または attackable position を満たさない時は `DEFEND` に fallback する。

## 12. Encounter initial model

encounter roll は `MOVE_ROUTE`、`MOVE_TO`、`WAIT` に対して、accepted で SRS turn が進んだ時だけ判定する。

次の場合、encounter roll は suppressed または skipped とする。

```text
SKIPPED_COMMAND
SKIPPED_NO_TURN_ADVANCE
SKIPPED_ENEMY_PRESENCE
SUPPRESSED_BASE_DOCKED
```

encounter chance は次で固定する。

```text
base encounter chance per SRS turn = 0.18
NEBULA terrain modifier = 0.7
other terrain modifier = 1.0
actual encounter chance = base chance * terrain modifier
```

danger score と group budget range は次で固定する。

```text
0 -> cost 1..1
1 -> cost 1..2
2 -> cost 2..3
3 -> cost 3..4
4 -> cost 4..5
```

spawn enemy composition は danger score ごとの固定 option table から選ぶ。spawn 点は warp point 候補から player 近傍 3x3 を除外し、候補数を超える場合は strongest tier から spawn cap を適用する。

## 13. Reward initial model

Phase 2 reward は次の 4 系統を含む。

```text
RESOURCE_CACHE:
  fixed +3 fuel recovery

SALVAGE pickup:
  immediate recovery choice + salvage inventory

enemy drop salvage:
  tier fixed salvage reward + immediate recovery choice

BASE upgrade:
  full recovery + salvage-spend upgrade purchase
```

enemy drop salvage は tier ごとに固定 value と recovery 量を持つ。representative fixture では `TIER3` drop が `salvage +2` と `RECOVER_ENERGY` を同時に適用する。

BASE upgrade の cost は prototype 実装の固定 table に従う。representative fixture では `DEFENSE` upgrade が salvage 4 を消費して `defense +1` を適用する。

## 14. WARP_EXIT command

`WARP_EXIT` は `exit_direction` を取り、現在位置の cell がその方向の `warp_flags` を持ち、かつその方向が `blocked_edges` でも銀河外縁でもない時にだけ accepted とする。

enemy presence が true の間は `WARP_EXIT` を `REJECTED_ENEMY_PRESENCE` とする。accepted `WARP_EXIT` は 1 SRS turn を消費し、LRS へ戻る唯一の出口になる。

## 15. Persistent state

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

`RESOURCE_CACHE` と `SALVAGE` は consumed 状態、`STATION` は activated 状態を再訪後も維持する。`discovered_cells` から復元した known map は replay 後も actual map の未観測情報を漏らしてはならない。

## 16. GameLog and comparison contract

GameLog は TBX parity の比較面であり、各 event は少なくとも `srs_turn`、`event_type`、`payload` を持つ。

required event types は次で固定する。

```text
MOVE_ACCEPTED
MOVE_REJECTED
WAIT_ACCEPTED
STOPPED_BEFORE_IMPASSABLE
OBSERVATION_UPDATED
INTERACT_ACCEPTED
INTERACT_REJECTED
OBJECT_CONSUMED
STATION_ACTIVATED
WARP_EXIT_ACCEPTED
WARP_EXIT_REJECTED
COMBAT_TRANSITIONED
COMBAT_REJECTED
ENCOUNTER_ROLLED
```

`INTERACT_ACCEPTED` は object 識別子、range、fuel before/after、combat resource before/after、salvage delta、upgrade または salvage choice の結果を含む。`COMBAT_TRANSITIONED` は phase 遷移、player action、enemy actions、resource before/after を含む。`ENCOUNTER_ROLLED` は terrain、terrain modifier、actual encounter chance、roll result、danger score、composition、spawned enemy ids を含む。

TBX state は少なくとも次を比較対象に含める。

```text
srs_turn
fuel
max_fuel
player_position
player durability / energy / photon_torpedo_ammo / salvage
consumed_object_ids
activated_object_ids
discovered_cells または復元可能な known map
combat phase / combat turn / enemy positions
entry / exit 情報
```

## 17. Display-input contract for #1076

#1076 へ渡す表示入力契約は、最終 UI ではなく表示 layer が受け取る state 差分として次を固定する。

```text
combat:
  enemy_presence
  combat_phase
  player durability / energy / photon_torpedo_ammo / salvage
  enemy tier / durability / position
  player attack result
  enemy action result
  reaction result
  counterattack / defend outcome

encounter:
  encounter roll required / skipped / suppressed
  base encounter chance
  terrain modifier
  actual encounter chance
  roll_result
  spawned enemy tiers / positions

reward:
  resource cache recovery result
  salvage choice
  salvage inventory delta
  enemy drop result
  base full recovery
  base upgrade availability / selected upgrade / salvage spent
```

加えて、既存の known map / visited / warp / consumed / activated object / blocked edge 表示契約も維持する。

## 18. Deferred follow-up map

現時点で open の follow-up は次のとおりとする。

```text
- advanced combat probability / hit-evasion-defense tuning: #1195
- final HUD / SRS / map layout: #1076
- TBX 移植時の parity 実装: follow-up implementation issue
```

これらの後続作業は、本書と `phase2_decisions.csv` で固定した Phase 2 external contract を変更しない前提で進める。
