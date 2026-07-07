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
- enemy dropped SALVAGE の現行挙動
- base station interaction
- base upgrade と salvage消費
- persistent_state / SRS turn / fuel / player_state への副作用
- #1275〜#1278 の deferred / follow-up

Excluded:

- gameplay実装変更
- fixture / snapshot 更新
- SALVAGE効果量の再バランス
- enemy drop randomization 実装
- enemy drop map object化 実装
- counterattack kill reward 実装
- integrated CLI入力UI実装

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

この文書は #1277 の仕様判断を反映した source-of-truth である。

注意: #1277 Step 1 は documentation-only であり、実装・fixture・snapshot はまだ B 方針へ同期しない。実装同期は #1277 Step 2 で扱う。

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

reject reason 名は実装PRで確定する。
候補:

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

## Enemy dropped SALVAGE current behavior

現行仕様:

- player attack phase で target enemy を破壊したとき、`target_enemy.drop_salvage` が `true` なら SALVAGE reward を即時適用する
- `reward_source` は `ENEMY_DROP`
- map object は生成しない
- `INTERACT` は不要
- `consumed_object_ids` / `activated_object_ids` は更新しない
- reward payload は `player_action.salvage_reward` に入る
- drop量は enemy tier に依存する

Current drop value:

```text
TIER1: salvage +1
TIER2: salvage +1
TIER3: salvage +2
TIER4: salvage +3
```

Current drop recovery amount for supported choices:

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

Unsupported enemy drop recovery choice:

```text
RECOVER_DURABILITY
```

注意: 現行実装・fixture はまだ `RECOVER_DURABILITY` を扱っている可能性がある。#1277 Step 2 で実装・fixture・test をこの仕様へ同期する。

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

#1266 / #1288 時点で確認済みの fixture regression:

```text
test_salvage_placeholder
test_salvage_recover_durability
test_base_upgrade_defense
test_combat_salvage_drop_tier3_energy
test_combat_salvage_no_drop_tier1
```

#1277 により、`test_salvage_recover_durability` は後続の実装同期で更新または置換される。

この spec 更新では fixture を変更しない。fixture は実装済み挙動の regression 固定であり、spec変更に合わせた更新は別PRで行う。

## Deferred / follow-up issues

### #1275

`#1275 counterattack撃破時にも enemy dropped SALVAGE を適用する`

Current:

```text
player attack kill only applies immediate ENEMY_DROP reward.
```

Deferred:

```text
counterattack kill reward is handled by #1275, but may be superseded or absorbed by #1276.
```

### #1276

`#1276 enemy dropped SALVAGE を即時取得ではなく map object として生成する`

Current:

```text
enemy drop is immediate reward; no map object is generated.
```

Deferred:

```text
#1276 may replace immediate reward with dropped SALVAGE object lifecycle.
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
#1277 Step 2 updates engine.py / model.py / fixtures / tests to match this spec.
```

### #1278

`#1278 enemy SALVAGE drop発生判定をランダム化する`

Current:

```text
enemy.drop_salvage bool determines drop/no-drop.
```

Deferred:

```text
#1278 may introduce tier-based random drop chance during encounter/spawn.
```

## Follow-up ordering memo

```text
1. #1277 Step 1 records the B decision in the source-of-truth spec.
2. #1277 Step 2 synchronizes implementation / fixtures / tests with the B decision.
3. #1276 may replace immediate enemy drop reward with map object lifecycle.
4. #1278 randomizes drop/no-drop decision without conflicting with #1276.
5. #1275 should be integrated into or re-scoped after #1276 if #1276 is adopted.
```
