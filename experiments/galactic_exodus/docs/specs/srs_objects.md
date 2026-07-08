# Galactic Exodus SRS objects and rewards

Source issue: #1288
Decision issue: #1277
Parent issue: #1266
Related: #1178, #1185, #1194, #1259, #1275, #1276, #1277, #1278
Base branch: `integration/882-galactic-exodus`

This document records the Phase 2 source-of-truth specification for SRS object interaction, SALVAGE rewards, and RESOURCE / STATION reward timing.

## Scope

Included:

- map上の SALVAGE pickup
- SALVAGE reward common flow
- salvage_choice / recovery choice
- enemy dropped SALVAGE object lifecycle
- dropped SALVAGE spawn
- dropped SALVAGE pickup through `INTERACT`
- drop payload / skip payload
- base station interaction
- base upgrade と salvage消費
- persistent_state / SRS turn / fuel / player_state への副作用
- #1275 / #1278 との follow-up関係

Excluded:

- gameplay実装変更
- fixture / snapshot 更新
- SALVAGE効果量の再バランス
- enemy drop randomization 実装 (#1278)
- counterattack実装変更 (#1275)
- integrated CLI入力UI実装
- 近傍空きcell探索ルール

## Current Phase 2 initial behavior

#1266 の棚卸し結果として、SALVAGE は prototype-only ではなく Phase 2 初期仕様として固定する。

#1288 / PR #1289 では、現行実装と fixture regression が固定していた挙動を正本化した。

#1277 では、SALVAGE取得時の即時回復対象について B 方針を採用した。

```text
SALVAGE取得時の即時回復は:
  - RECOVER_ENERGY
  - RECOVER_PHOTON_TORPEDO_AMMO
  - STORE_ONLY

に限定する。

RECOVER_DURABILITY は SALVAGE pickup では非対応にし、durability recovery は BASE / STATION に寄せる。
```

#1277 / #1292 により、実装・fixture・test は B 方針へ同期済み。

## Map SALVAGE pickup

仕様:

- map上の SALVAGE object は `INTERACT` で取得する
- `reward_source` は `MAP_PICKUP`
- base salvage value は `1`
- `salvage_choice` がない場合は `STORE_ONLY`
- successful interaction で SRS turn `+1`
- fuel は変化しない
- object は consumed になる
- `persistent_state.consumed_object_ids` に `object_id` を追加する
- `player_state` / `combat_state.player` に reward を反映する

Supported immediate effects:

```text
RECOVER_ENERGY
RECOVER_PHOTON_TORPEDO_AMMO
STORE_ONLY
```

Unsupported immediate effects:

```text
RECOVER_DURABILITY
```

`RECOVER_DURABILITY` が SALVAGE reward choice として指定された場合は、暗黙に `STORE_ONLY` へ丸めず、明示的に reject する。

reject reason:

```text
REJECTED_UNSUPPORTED_SALVAGE_CHOICE
```

## SALVAGE reward common flow

仕様:

- choiceなしの場合は `STORE_ONLY`
- `RECOVER_ENERGY` は `energy_capacity` まで回復
- `RECOVER_PHOTON_TORPEDO_AMMO` は `photon_torpedo_ammo_capacity` まで回復
- `STORE_ONLY` は即時回復せず、salvage inventory のみ増やす
- どのsupported choiceでも salvage inventory は `salvage_value` 分増える
- reward payload には `salvage_before` / `salvage_after` / `selected_salvage_choice` / 各delta が含まれる

`RECOVER_DURABILITY` は SALVAGE pickup immediate effect としては非対応とする。

durability recovery は BASE / STATION recovery 側で扱う。

## Enemy dropped SALVAGE object lifecycle

Before #1276:

```text
player attack kill with target_enemy.drop_salvage=true applied ENEMY_DROP reward immediately.
no map object was generated.
INTERACT was not required.
```

After #1276:

```text
enemy kill with drop_salvage=true spawns a SALVAGE object at the destroyed enemy position.
player inventory is not updated at kill time.
reward is applied when the dropped object is picked up with INTERACT.
```

仕様:

- 対象 enemy の `drop_salvage` が `true` の場合のみ drop候補になる
- `drop_salvage` が `false` の場合、object は生成しない
- object_type は `SALVAGE`
- position は destroyed enemy の position
- `reward_source` は `ENEMY_DROP`
- `salvage_value` は enemy tier から解決する
- generated dropped SALVAGE は map上の initial SALVAGE と同じ `INTERACT` lifecycle で取得する
- player inventory は kill 時点では更新しない
- successful pickup で SRS turn `+1`
- fuel は変化しない
- object は consumed になる
- `persistent_state.consumed_object_ids` に `object_id` を追加する
- `player_state` / `combat_state.player` に reward を反映する
- reward payload には `salvage_before` / `salvage_after` / `selected_salvage_choice` / 各delta が含まれる

Dropped SALVAGE metadata / payload meaning:

```text
reward_source: ENEMY_DROP
dropped_by_enemy_id
dropped_by_enemy_tier
salvage_value
recovery_profile or enemy_tier
```

補足:

- 実装上の保持形式は Step 2 (#1294) で決めてよい
- ただし `INTERACT` 時に value / recovery amount を再現できることが必要

Drop value by enemy tier:

```text
TIER1: salvage +1
TIER2: salvage +1
TIER3: salvage +2
TIER4: salvage +3
```

Supported recovery choices for dropped SALVAGE:

```text
RECOVER_ENERGY
RECOVER_PHOTON_TORPEDO_AMMO
STORE_ONLY
```

Unsupported recovery choices for dropped SALVAGE:

```text
RECOVER_DURABILITY
```

`RECOVER_DURABILITY` が dropped SALVAGE pickup の `salvage_choice` として指定された場合も、暗黙に `STORE_ONLY` へ丸めず、明示的に reject する。

reject reason:

```text
REJECTED_UNSUPPORTED_SALVAGE_CHOICE
```

Recovery amount by enemy tier:

```text
TIER1:
  RECOVER_ENERGY: 2
  RECOVER_PHOTON_TORPEDO_AMMO: 1
  STORE_ONLY: 0

TIER2:
  RECOVER_ENERGY: 2
  RECOVER_PHOTON_TORPEDO_AMMO: 1
  STORE_ONLY: 0

TIER3:
  RECOVER_ENERGY: 3
  RECOVER_PHOTON_TORPEDO_AMMO: 1
  STORE_ONLY: 0

TIER4:
  RECOVER_ENERGY: 4
  RECOVER_PHOTON_TORPEDO_AMMO: 2
  STORE_ONLY: 0
```

Occupied-cell skip policy:

```text
destroyed enemy position の cell.object_id が None の場合のみ dropped SALVAGE を生成する
cell.object_id が既にある場合は生成しない
近傍空きcell探索はしない
payload に spawned=false と skip_reason を記録する
```

推奨 skip reason:

```text
OCCUPIED_CELL
```

実装PRで別名にする場合は、PR本文に理由を書く。

Event payload meaning:

```text
player attack phase:
  COMBAT_TRANSITIONED.payload.player_action.salvage_drop

counterattack reaction:
  COMBAT_TRANSITIONED.payload.enemy_actions[].reaction.salvage_drop
```

payload に含める情報:

```text
reward_source: ENEMY_DROP
object_id
position
enemy_id
enemy_tier
salvage_value
spawned
skip_reason, if spawned=false
```

注意:

- #1293 は payload shape の意味を正本化する
- 実際の exact key name は #1294 で実装に合わせて最終化してよい
- ただし PR本文で specとの差分があれば明記する

## Base station / upgrade

仕様:

- `STATION` interaction は fuel / durability / energy / photon torpedo ammo を回復する
- successful interaction で SRS turn `+1`
- station は activated になる
- `persistent_state.activated_object_ids` に `object_id` を追加する
- `base_upgrade_choice` が指定され、costを満たす場合、salvageを消費して player能力を更新する

Current upgrade cost:

```text
PHASER_POWER: 4
PHOTON_TORPEDO_POWER: 5
ENERGY_CAPACITY: 3
PHOTON_TORPEDO_AMMO_CAPACITY: 3
DEFENSE: 4
EVASION: 4
```

Durability recovery remains supported on BASE / STATION interaction.
SALVAGE pickup does not provide durability recovery.

## Fixture regression references

#1292 時点で確認済みの fixture regression:

```text
test_salvage_placeholder
test_salvage_reject_recover_durability
test_salvage_recover_energy
test_salvage_recover_photon_torpedo_ammo
test_base_upgrade_defense
test_combat_salvage_drop_tier3_energy
test_combat_salvage_no_drop_tier1
```

#1293 は documentation-only であり、fixture は変更しない。dropped SALVAGE object lifecycle の fixture 更新は #1294 で扱う。

## Deferred / follow-up issues

### #1275

`#1275 counterattack撃破時にも enemy dropped SALVAGE を適用する`

Follow-up:

```text
counterattack kill should use the same dropped SALVAGE object spawn helper.
immediate ENEMY_DROP reward should not be reintroduced.
```

### #1277

`#1277 SALVAGE取得時の即時回復対象を ammo / energy 中心に再整理する`

Decision:

```text
B を採用する。

Supported:
  RECOVER_ENERGY
  RECOVER_PHOTON_TORPEDO_AMMO
  STORE_ONLY

Unsupported:
  RECOVER_DURABILITY on SALVAGE pickup

Durability recovery:
  BASE / STATION recovery側で扱う
```

Implementation sync:

```text
#1277 / #1292 updated engine.py / fixtures / tests to match this spec.
```

### #1278

`#1278 enemy SALVAGE drop発生判定をランダム化する`

Current:

```text
enemy.drop_salvage bool determines drop/no-drop.
```

Deferred:

```text
randomization should decide drop_salvage or drop/no-drop before combat reward resolution.
once drop_salvage is true, #1276 object lifecycle applies.
```

## Follow-up ordering memo

```text
1. #1277 Step 1 records the B decision in the source-of-truth spec.
2. #1277 / #1292 synchronized implementation / fixtures / tests with the B decision.
3. #1276 replaces immediate enemy drop reward with dropped SALVAGE object lifecycle.
4. #1278 randomizes drop/no-drop decision without conflicting with #1276.
5. #1275 should be integrated into the same spawn-based lifecycle after #1276.
```
