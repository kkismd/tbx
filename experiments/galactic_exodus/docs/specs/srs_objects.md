# Galactic Exodus SRS object と reward

Source issue: #1288
Decision issue: #1277
Parent issue: #1266
Related: #1178, #1185, #1194, #1259, #1275, #1276, #1277, #1278
Base branch: `integration/882-galactic-exodus`

この文書は、Phase 2 における SRS object interaction、`SALVAGE` reward、`RESOURCE` / `STATION` reward timing の source-of-truth spec を記録する。

## 1. 文書の位置付けと正本性

このファイルは、Phase 2 の object interaction と `SALVAGE` reward に関する current source of truth である。

authority優先順位:

1. マージ済み判断 issue と `experiments/galactic_exodus/docs/specs/` 配下の current docs
2. 現行実装と fixture regression
3. legacy doc と過去の issue comment

参照根拠:

- implementation: `experiments/galactic_exodus/srs/engine.py`, `experiments/galactic_exodus/srs/model.py`
- regression: `experiments/galactic_exodus/srs/test_fixture_regression.py`
- legacy / archive: `experiments/galactic_exodus/docs/archive/`

## 2. 対象範囲と境界

対象:

- map 上の `SALVAGE` pickup
- `SALVAGE` reward の共通フロー
- `salvage_choice` / recovery choice
- enemy が drop する `SALVAGE` object の lifecycle
- enemy `SALVAGE` drop の randomization
- tier-based enemy `SALVAGE` drop chance
- drop された `SALVAGE` の spawn
- `INTERACT` による dropped `SALVAGE` pickup
- drop payload / skip payload
- base station interaction
- base upgrade と salvage 消費
- `persistent_state` / SRS turn / fuel / `player_state` への副作用
- #1275 / #1278 との追跡関係

対象外:

- gameplay 実装の変更
- fixture / snapshot の更新
- `SALVAGE` 効果量の再バランス
- enemy drop randomization 実装そのもの (#1278)
- counterattack 実装の変更 (#1275)
- integrated CLI の入力 UI 実装
- 近傍空き cell 探索ルール

他の current spec へ委譲する範囲:

- 戦闘 phase と `COMBAT_STEP`: `docs/specs/srs_combat.md`
- encounter / spawn 全体の決定: `docs/specs/srs_encounter.md`
- movement と impassable 判定: `docs/specs/srs_movement.md`

## 3. 現行仕様

#1266 の棚卸し結果として、`SALVAGE` は prototype-only ではなく、Phase 2 初期仕様として固定する。

#1288 / PR #1289 では、現行実装と fixture regression が固定していた挙動を正本化した。

#1277 では、`SALVAGE` 取得時の即時回復対象について B 方針を採用した。

```text
SALVAGE取得時の即時回復は:
  - RECOVER_ENERGY
  - RECOVER_PHOTON_TORPEDO_AMMO
  - STORE_ONLY

に限定する。

RECOVER_DURABILITY は SALVAGE pickup では非対応にし、durability recovery は BASE / STATION に寄せる。
```

この文書は、#1277 の仕様判断を反映した source-of-truth である。

#1277 / #1292 により、実装・fixture・test は B 方針へ同期済みである。

## 4. マップ上の`SALVAGE`取得

仕様:

- map 上の `SALVAGE` object は `INTERACT` で取得する
- `reward_source` は `MAP_PICKUP`
- base salvage value は `1`
- `salvage_choice` がない場合は `STORE_ONLY`
- successful interaction で SRS turn は `+1`
- fuel は変化しない
- object は consumed になる
- `persistent_state.consumed_object_ids` に `object_id` を追加する
- `player_state` / `combat_state.player` に reward を反映する

対応する即時効果:

```text
RECOVER_ENERGY
RECOVER_PHOTON_TORPEDO_AMMO
STORE_ONLY
```

対応しない即時効果:

```text
RECOVER_DURABILITY
```

`RECOVER_DURABILITY` が `SALVAGE` reward choice として指定された場合は、暗黙に `STORE_ONLY` へ丸めず、明示的に reject する。

reject reason:

```text
REJECTED_UNSUPPORTED_SALVAGE_CHOICE
```

## 5. `SALVAGE`報酬の共通フロー

仕様:

- choice がない場合は `STORE_ONLY`
- `RECOVER_ENERGY` は `energy_capacity` まで回復する
- `RECOVER_PHOTON_TORPEDO_AMMO` は `photon_torpedo_ammo_capacity` まで回復する
- `STORE_ONLY` は即時回復せず、salvage inventory だけを増やす
- どの対応済み choice でも salvage inventory は `salvage_value` 分だけ増える
- reward payload には `salvage_before` / `salvage_after` / `selected_salvage_choice` / 各 delta を含める

`RECOVER_DURABILITY` は `SALVAGE` pickup の immediate effect としては非対応とする。

durability recovery は `BASE` / `STATION` recovery 側で扱う。

## 6. 敵`SALVAGE` dropの乱数化

#1278 では、enemy dropped `SALVAGE` の drop / no-drop 判定を、固定 bool だけでなく random roll でも決められるようにする。

変更前の#1278:

```text
enemy.drop_salvage bool が drop / no-drop を決める。
fixture では drop_salvage true / false を明示できる。
drop chance table、random roll、seed driven の drop resolution は存在しない。
```

変更後の#1278:

```text
enemy SALVAGE の drop / no-drop は combat reward resolution 前に確定する。
通常の encounter / enemy spawn では enemy tier と RNG roll から drop_salvage を決めてよい。
combat は、すでに確定済みの enemy.drop_salvage bool だけを読み、再 roll しない。
explicit fixture / fixed encounter の drop_salvage 値は deterministic override のまま維持する。
```

仕様:

- enemy `SALVAGE` drop 判定は、combat reward resolution より前に確定する
- combat 中に drop 判定を再 roll しない
- 通常の encounter / enemy spawn 側で random roll により `drop_salvage` bool を確定する
- fixture / fixed encounter では `drop_salvage: true / false` を明示できる
- 明示的な `drop_salvage` 指定は random roll より優先する
- `drop_salvage == true` の enemy が破壊された場合は、#1276 / #1296 の dropped `SALVAGE` object lifecycle を使う
- `drop_salvage == false` の enemy が破壊された場合は dropped `SALVAGE` object を生成しない
- #1278 は drop 発生有無だけを決め、immediate reward は再導入しない

tier-based drop chance:

```text
TIER1: 0.25
TIER2: 0.35
TIER3: 0.50
TIER4: 0.75
```

drop chance 制約:

```text
0.0 <= chance <= 1.0
TIER1 <= TIER2 <= TIER3 <= TIER4
```

roll 解決:

```text
roll < chance:
  drop_salvage = true

roll >= chance:
  drop_salvage = false
```

fixture / fixed encounter override:

```text
explicit drop_salvage: true
  always keep drop_salvage = true
  do not roll

explicit drop_salvage: false
  always keep drop_salvage = false
  do not roll

no explicit drop_salvage:
  resolve drop_salvage from tier chance and roll
```

debug payload / summary の意味:

```text
enemy_id
enemy_tier
salvage_drop_chance
salvage_drop_roll
drop_salvage
```

補足:

- exact payload key name は #1298 の実装に合わせて最終化してよい
- ただし chance / roll / result を debug または fixture summary から追跡できることを必須とする
- drop chance は gameplay balance に影響するため、実装 PR では採用値の理由を短く記録する

## 7. 敵がdropする`SALVAGE` objectのライフサイクル

変更前の#1276:

```text
player attack で target_enemy.drop_salvage=true の kill が起きた場合、ENEMY_DROP reward を即時適用していた。
map object は生成されなかった。
INTERACT は不要だった。
```

変更後の#1276:

```text
drop_salvage=true の enemy を kill した場合、破壊された enemy の位置に SALVAGE object を spawn する。
player inventory は kill 時点では更新しない。
reward は、drop された object を INTERACT で取得した時点で適用する。
```

仕様:

- 対象 enemy の `drop_salvage` が `true` の場合にのみ drop 候補になる
- `drop_salvage` が `false` の場合、object は生成しない
- `object_type` は `SALVAGE`
- `position` は破壊された enemy の position
- `reward_source` は `ENEMY_DROP`
- `salvage_value` は enemy tier から解決する
- 生成された dropped `SALVAGE` は、map 上の初期 `SALVAGE` と同じ `INTERACT` lifecycle で取得する
- player inventory は kill 時点では更新しない
- successful pickup で SRS turn は `+1`
- fuel は変化しない
- object は consumed になる
- `persistent_state.consumed_object_ids` に `object_id` を追加する
- `player_state` / `combat_state.player` に reward を反映する
- reward payload には `salvage_before` / `salvage_after` / `selected_salvage_choice` / 各 delta を含める

dropped `SALVAGE` metadata / payload の意味:

```text
reward_source: ENEMY_DROP
dropped_by_enemy_id
dropped_by_enemy_tier
salvage_value
recovery_profile or enemy_tier
```

補足:

- 実装上の保持形式は Step 2 (#1294) で決めてよい
- ただし `INTERACT` 時に value / recovery amount を再現できることを必須とする

enemy tier ごとの drop value:

```text
TIER1: salvage +1
TIER2: salvage +1
TIER3: salvage +2
TIER4: salvage +3
```

dropped `SALVAGE` に対応する recovery choice:

```text
RECOVER_ENERGY
RECOVER_PHOTON_TORPEDO_AMMO
STORE_ONLY
```

dropped `SALVAGE` で対応しない recovery choice:

```text
RECOVER_DURABILITY
```

`RECOVER_DURABILITY` が dropped `SALVAGE` pickup の `salvage_choice` として指定された場合も、暗黙に `STORE_ONLY` へ丸めず、明示的に reject する。

reject reason:

```text
REJECTED_UNSUPPORTED_SALVAGE_CHOICE
```

enemy tier ごとの recovery amount:

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

occupied-cell skip 方針:

```text
destroyed enemy position の cell.object_id が None の場合のみ dropped SALVAGE を生成する
cell.object_id が既にある場合は生成しない
近傍空きcell探索はしない
payload に spawned=false と skip_reason を記録する
```

推奨する skip reason:

```text
OCCUPIED_CELL
```

実装 PR で別名にする場合は、PR 本文に理由を書く。

event payload における意味:

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
- ただし PR 本文で spec との差分があれば明記する

## 8. 基地ステーションと強化

仕様:

- `STATION` interaction は fuel / durability / energy / photon torpedo ammo を回復する
- successful interaction で SRS turn は `+1`
- station は activated になる
- `persistent_state.activated_object_ids` に `object_id` を追加する
- `base_upgrade_choice` が指定され、cost を満たす場合、salvage を消費して player 能力を更新する

現行の upgrade cost:

```text
PHASER_POWER: 4
PHOTON_TORPEDO_POWER: 5
ENERGY_CAPACITY: 3
PHOTON_TORPEDO_AMMO_CAPACITY: 3
DEFENSE: 4
EVASION: 4
```

durability recovery は `BASE` / `STATION` interaction で引き続き対応する。
`SALVAGE` pickup では durability recovery を提供しない。

## 9. fixture回帰確認の参照

#1296 時点で確認済みの fixture regression:

```text
test_salvage_placeholder
test_salvage_reject_recover_durability
test_salvage_recover_energy
test_salvage_recover_photon_torpedo_ammo
test_base_upgrade_defense
test_combat_salvage_drop_object_tier3
test_combat_salvage_drop_then_interact
test_combat_salvage_drop_occupied_cell_skip
test_combat_salvage_no_drop_tier1
```

#1297 は documentation-only であり、fixture は変更しない。enemy `SALVAGE` drop randomization の fixture 更新は #1298 で扱う。

## 10. 後続のissueと追跡項目

### #1275

`#1275 counterattack撃破時にも enemy dropped SALVAGE を適用する`

追跡事項:

```text
counterattack kill でも、同じ dropped SALVAGE object spawn helper を使うべきである。
immediate ENEMY_DROP reward は再導入しない。
```

#1296 により、counterattack reaction payload 側でも `salvage_drop` を扱える土台は入っている。残件がある場合は #1275 の scope を再確認する。

### #1277

`#1277 SALVAGE取得時の即時回復対象を ammo / energy 中心に再整理する`

判断:

```text
B を採用する。

対応する即時回復:
  RECOVER_ENERGY
  RECOVER_PHOTON_TORPEDO_AMMO
  STORE_ONLY

対応しない即時回復:
  SALVAGE pickup における RECOVER_DURABILITY

durability 回復:
  BASE / STATION の recovery 側で扱う
```

実装同期:

```text
#1277 / #1292 で engine.py / fixture / test をこの spec に同期済みである。
```

### #1278

`#1278 enemy SALVAGE drop発生判定をランダム化する`

判断:

```text
上記の `enemy SALVAGE dropのランダム化` 節に記録したとおりである。
```

実装同期:

```text
#1298 で encounter / spawn 実装、fixture、test をこの spec に同期する。
```

## 11. 追跡対応順メモ

```text
1. #1277 で B 判断を source-of-truth spec に記録する。
2. #1277 / #1292 で、その B 判断へ実装 / fixture / test を同期する。
3. #1276 / #1295 / #1296 で、immediate enemy drop reward を dropped SALVAGE object lifecycle へ置き換える。
4. #1278 / #1297 で、enemy SALVAGE drop のランダム化を source-of-truth spec に記録する。
5. #1278 / #1298 で、ランダム化 spec へ encounter / spawn 実装、fixture、test を同期する。
6. #1296 後に、#1275 を spawn-based lifecycle 前提で再確認する。
```
