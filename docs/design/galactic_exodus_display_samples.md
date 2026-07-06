# Galactic Exodus Phase 2 display samples

Related: #1076, #1214

## 1. Purpose

この文書は、Galactic Exodus Phase 2 の表示設計を確定する前段階として、LRS macro map、SRS local map、HUD、log/debug 表示を組み合わせた比較サンプルをまとめる。

ここではゲームルールを再決定しない。#1218 系列で確定した座標契約と、Phase 2 SRS 正本仕様を前提に、ユーザーが読む表示だけを比較する。

```text
LRS display coordinate:
  origin = lower-left
  coordinates are 1-based
  sample range = (1,1) ... (8,8)

SRS internal coordinate:
  origin = lower-left
  coordinates are 0-based
  used by engine / fixture / validator / tests / raw event payload

SRS display coordinate:
  origin = lower-left
  coordinates are 1-based
  sample range = (1,1) ... (9,9)
  used by render / manual eval / HUD / display samples
```

## 2. Recommended combined baseline

現時点の推奨 baseline は、次の組み合わせとする。

```text
LRS:
  user proposal: border-light macro map
  purpose: known sector / visited route / rift edge を1画面で把握する

SRS:
  user proposal: borderless 9x9 north-to-south local map
  purpose: local movement, combat, reward pickup, warp / blocked edge を素早く読む

HUD:
  compact status block under maps
  purpose: action choice に必要な resource / combat / encounter / reward 状態だけを出す

Log/debug:
  detailed event stream
  purpose: roll value, before/after resource, consumed/activated ids, validation details を追跡する
```

この baseline は、罫線で LRS の edge blocker を表し、SRS では外縁 `#` cell と warp symbol で blocked edge / warp point を表す。LRS と SRS は RIFT の意味が異なるため、同じ描画規則へ無理に統一しない。

## 3. Display sample: normal play screen

### 3.1 LRS macro map

```text
  +---+---+---+---+---+---+---+---+
8 | ?   ?   ?   ?   ?   ?   ?   H |
  +                               +
7 | ?   .   .   N   ?   ?   ?   ? |
  +           +                   +
6 | ?   .   R | .   ?   ?   ?   ? |
  +           +                   +
5 | ?   .   @   B   ?   ?   ?   ? |
  +       +---+                   +
4 | ?   .   .   .   ?   ?   ?   ? |
  +                               +
3 | ?   ?   ?   ?   ?   ?   ?   ? |
  +                               +
2 | ?   ?   ?   ?   ?   ?   ?   ? |
  +                               +
1 | S   .   .   ?   ?   ?   ?   ? |
  +---+---+---+---+---+---+---+---+
    1   2   3   4   5   6   7   8
```

### 3.2 SRS local map

```text
 9  ? ? ? ? ? ? ? ? ?
 8  ? ? ? ? ? ? ? ? ?
 7  ? ? ? . . . ? ? ?
 6  ? ? ? . . . . . #
 5  ? ? ? . e . . . #
 4  ? ? ? . . . @ . #
 3  ? ? ? . $ . . . #
 2  ? ? . . . . . . #
 1  ? ? v v v v v v #

    1 2 3 4 5 6 7 8 9
```

### 3.3 HUD

```text
SECTOR  LRS=(3,5)  TYPE=RIFT  SRS=(7,4)  SENSOR=5x5
TURN    LRS=18     SRS=4      COST=TURN_ONLY
FUEL    6/9        STATUS=EXPLORING

PLAYER  DUR=100/100  EN=6/6  TORP=6/6  SALVAGE=1
COMBAT  PHASE=PLAYER_MOVEMENT  ENEMY=enemy-1 T2 hp=5 at SRS=(5,5)
WARP    S available at display y=1; E blocked by RIFT_BARRIER
REWARD  SALVAGE detected at SRS=(5,3)
LAST    MOVE_ACCEPTED route=E,E center=(7,4); OBSERVATION_UPDATED +6 cells
```

### 3.4 Log / debug tail

```text
LOG
[018.003] MOVE_ACCEPTED route=E,E internal_from=[4,3] internal_to=[6,3] display_from=(5,4) display_to=(7,4)
[018.003] OBSERVATION_UPDATED center_internal=[6,3] center_display=(7,4) size=5x5 new=6 total=34
[018.003] ENCOUNTER_ROLLED terrain=FLOOR base=0.18 modifier=1.0 actual=0.18 roll=0.42 result=no_encounter
[018.004] PLAYER_MOVEMENT phase_ready enemy_presence=true nearest=enemy-1 display=(5,5)
```

HUD は判断に必要な短い情報だけを出し、log/debug は raw internal coordinate と display coordinate の両方を確認できる形にする。通常プレイでは internal coordinate は表示しないが、manual eval / debug では併記してよい。

## 4. Legend

### 4.1 LRS symbols

| Symbol | Meaning |
| --- | --- |
| `?` | 未観測 sector |
| `.` | 既知通常 sector |
| `@` | 現在位置 |
| `S` | start / Sol |
| `H` | home / goal |
| `B` | base sector |
| `R` | resource sector, 未使用 |
| `r` | resource sector, 使用済み |
| `N` | nebula sector |
| `A` | asteroid sector |
| `G` | gravity sector |
| `+---+` / `|` | sector 間の既知 blocked edge / RIFT |
| blank edge | 通行可能または未確定 edge |

LRS の RIFT は「cell と cell の間の edge blocker」であり、sector cell 自体を `#` にしない。

### 4.2 SRS symbols

| Symbol | Meaning |
| --- | --- |
| `?` | 未観測 cell |
| `.` | 既知 floor / passable cell |
| `@` | player |
| `e` | enemy |
| `$` | unconsumed salvage |
| `s` | consumed salvage |
| `R` | unconsumed resource cache |
| `r` | consumed resource cache |
| `S` | station / base object |
| `*` | star / impassable object |
| `o` | planet / impassable object |
| `#` | impassable terrain, asteroid or RIFT_BARRIER |
| `^` | N warp point |
| `>` | E warp point |
| `v` | S warp point |
| `<` | W warp point |
| `+` | multiple warp flags on same cell |

SRS の RIFT は「外縁 cell が `RIFT_BARRIER` になる」表現である。上の sample では、display x=9 の列が `#` で、E edge が blocked であることを示す。

## 5. HUD contents

通常HUDは、プレイヤーが次の行動を選ぶために必要な情報だけを表示する。

```text
required normal HUD:
  - current LRS display coordinate
  - current SRS display coordinate when inside SRS
  - sector type
  - sensor range: 5x5 or 3x3
  - LRS fuel / capacity
  - SRS turn
  - player durability / energy / torpedo ammo / salvage
  - combat phase if enemy exists
  - nearest or currently targeted enemy summary
  - warp availability / blocked edge summary
  - currently relevant reward object summary
  - last event one-line summary
```

通常HUDからは外す。

```text
debug/log only:
  - raw internal coordinate unless manual eval mode
  - full event payload
  - roll seed / RNG stream details
  - all consumed_object_ids / activated_object_ids
  - exact before/after dictionaries for every resource
  - full enemy action list when it is not needed for immediate action
  - validator-only known map secrecy details
```

## 6. Event wording samples

### 6.1 Movement / observation

```text
MOVE  accepted route=E,E to SRS=(7,4)
SCAN  5x5 update: +6 known cells, total=34
SCAN  NEBULA interference: sensor range reduced to 3x3
STOP  blocked by RIFT_BARRIER at SRS=(9,4)
```

### 6.2 Warp / RIFT

```text
WARP  S available at SRS=(5,1)
WARP  rejected: E edge is blocked by RIFT_BARRIER
RIFT  discovered: LRS edge (3,5)-E is blocked
RIFT  known blocked edge: route cancelled before fuel spend
```

### 6.3 Reward

```text
CACHE acquired: fuel +3 -> 6/9
CACHE already consumed at SRS=(5,3)
SALVAGE acquired: +1 inventory, durability +8 -> 100/100
BASE station activated: full recovery complete
UPGRADE defense +1, salvage 4 -> 0
```

### 6.4 Encounter / combat

```text
ENCOUNTER roll=0.110 < 0.126: enemy T2 spawned at SRS=(5,5)
COMBAT player phase: enemy-1 T2 hp=5 at SRS=(5,5)
ATTACK photon torpedo hit enemy-1: hp 5 -> 2, ammo 6 -> 5
ENEMY enemy-1 attacks: damage 7, player durability 100 -> 93
REACT defend: damage reduced 7 -> 4
```

## 7. Comparison with previous candidates

| Candidate | Role | Strength | Weakness | Suggested status |
| --- | --- | --- | --- | --- |
| A: space-separated map + status | ASCII fallback / debug baseline | Simple, compact, easy snapshot | RIFT direction is weak | Keep as fallback |
| C: Rogue-like bordered local map | SRS precision reference | Cell boundaries are clear | Consumes width/height | Keep for debug / detailed SRS |
| D: STTR-style frame + local SRS | Normal play layout | Good flavor and integrated HUD | Unicode frame width risk | Candidate for final UI |
| User LRS + user SRS baseline | Current recommended baseline | LRS edge blockers and SRS local readability are balanced | Needs HUD/log supplementation | Use as #1214 primary sample |
| E: Braille macro map | compact macro experiment | Can encode edges densely | Font width / accessibility risk | Optional experiment, not baseline |

## 8. Width and accessibility notes

```text
80-column target:
  - LRS sample width is approximately 38 columns plus y labels.
  - SRS sample width is approximately 23 columns plus y labels.
  - Side-by-side LRS + SRS is possible only with a compact HUD or no HUD.
  - Baseline should allow stacked maps with HUD below.
```

```text
accessibility:
  - Do not depend on color alone.
  - Keep ASCII fallback for every Unicode frame or Braille sample.
  - Keep a text legend close to the sample.
  - Use explicit event wording for RIFT direction, resource consumed state, and enemy position.
```

## 9. Recommended next decision for #1076

#1214 の比較サンプルとしては、この文書の `User LRS + user SRS baseline` を primary candidate にする。

#1076 へ渡す採用候補は次とする。

```text
normal play candidate:
  user LRS border-light macro map
  user SRS borderless north-to-south map
  compact HUD below maps
  one-line last event

debug candidate:
  same maps
  detailed log tail
  internal/display coordinate pair where needed

fallback candidate:
  space-separated ASCII map
  no Unicode frame / no Braille dependency
```

未決定として残す項目は次である。

```text
- LRS edge blocker を常時罫線表示するか、known blocked edge list も併記するか
- SRS `#` を RIFT_BARRIER と ASTEROID で同じにするか、別 glyph を使うか
- enemy を tier別に `e1` / `e2` のように表示するか、HUDだけで tier を出すか
- HUD を maps の下に置くか、右ペインに置くか
- Braille macro map を採用候補に残すか、実験案として棄却するか
```
