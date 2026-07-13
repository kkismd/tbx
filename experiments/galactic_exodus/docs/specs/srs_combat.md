# Galactic Exodus SRS 戦闘仕様

Source issue: #1319
Parent issue: #1314
Decision inputs: #1178, #1194
Related: #1195, #1259, #1275, #1276, #1296, #1303, #1304, #1318
Base branch: `integration/882-galactic-exodus`

この文書は、Galactic Exodus の SRS 戦闘に関する `CURRENT_SOURCE` である。

- issue comment、legacy spec、fixture、実装は参照根拠または regression 面として扱う
- それらと差分がある場合は、競合する正本として扱うのではなく、この文書に合わせて更新する
- 高度な命中率、回避率、最終的なバランス調整は deferred のままとする
- この文書は、現行実装と既存 regression がすでに固定している範囲を超えて、新しい戦闘ルールを追加しない

## 1. 文書の位置付けと正本性

このファイルは、Phase 2 の戦闘状態、command の流れ、enemy 行動、reaction、combat event payload の current source of truth である。

authority優先順位:

1. `experiments/galactic_exodus/docs/specs/` 配下の、マージ済み判断 issue と current docs
2. 現行実装と regression test
3. legacy `experiments/galactic_exodus/docs/archive/phase2_srs_spec.md`

参照根拠:

- legacy baseline: `experiments/galactic_exodus/docs/archive/phase2_srs_spec.md`
- implementation: `experiments/galactic_exodus/srs/model.py`, `experiments/galactic_exodus/srs/engine.py`
- regression: `experiments/galactic_exodus/srs/test_engine_combat.py`, `experiments/galactic_exodus/srs/test_fixture_regression.py`, `experiments/galactic_exodus/srs/fixtures/combat_*.json`

## 2. 対象範囲と境界

対象:

- 戦闘状態と phase progression
- `COMBAT_STEP` の command 契約
- player attack の解決
- enemy の行動順、攻撃、移動
- `COUNTERATTACK` / `DEFEND` reaction 処理
- 戦闘リソースの消費と回復タイミング
- enemy 破壊と行動 skip
- combat event payload
- fixture regression で使う combat 比較項目

他の current spec へ委譲する範囲:

- encounter roll、danger score、spawn composition: `docs/specs/srs_encounter.md`
- drop された `SALVAGE` object の lifecycle、pickup、reward effect: `docs/specs/srs_objects.md`
- 非戦闘 SRS movement: `docs/specs/srs_movement.md`
- 統合 CLI の parsing と rendering: `docs/specs/integrated_cli.md`
- map 生成と配置: `docs/specs/srs_map_generation.md`

対象外:

- encounter chance の再バランス
- enemy `SALVAGE` drop probability の再バランス
- pickup effect の再設計
- base upgrade cost の調整
- 最終的な weapon / enemy balance
- 新しい hit probability または evasion probability system
- 新しい weapon type、enemy tier、reaction type

## 3. 状態モデル

現行の戦闘状態は `SrsCombatState` であり、次の field を持つ。

- `player`
- `enemies`
- `weapon_profiles`
- `phase`
- `combat_turn`
- `player_attack_target_id`

導出値:

- `enemy_presence = bool(enemies)`
- `target_available = player_attack_target_id references an existing enemy`

現行 phase の enum 名:

```text
PLAYER_MOVEMENT
PLAYER_ATTACK
ENEMY_ACTION
```

基準となる遷移 cycle:

```text
PLAYER_MOVEMENT -> PLAYER_ATTACK -> ENEMY_ACTION -> PLAYER_MOVEMENT
```

遷移ルール:

- `PLAYER_MOVEMENT` は phase advance 専用の command として `COMBAT_STEP` を受理する
- `enemy_presence` が `true` で、選択中 target を少なくとも 1 つの player weapon が attack 可能な場合、次の phase は `PLAYER_ATTACK` になる
- それ以外では、`PLAYER_MOVEMENT` は `ENEMY_ACTION` へ直接 skip する
- `PLAYER_ATTACK` は `ENEMY_ACTION` へ進む
- `ENEMY_ACTION` は `PLAYER_MOVEMENT` へ進む

状態の不変条件:

- reject された戦闘 action は、phase、`combat_turn`、player resource、enemy state を変更しない
- 現在の target を破壊した場合は `player_attack_target_id` を clear する
- 最後の enemy が `enemies` から除去されると `enemy_presence` は `false` になる
- 現行実装は、`enemy_presence = false` の空 `combat_state` を維持し、`COMBAT_STEP` 中に自動で `None` へ置き換えない

## 4. command契約

戦闘では `SrsCommand(command_type="COMBAT_STEP", ...)` を用いる。

現行で受理する戦闘関連 field:

- `player_attack_action`: `ATTACK` または `SKIP`
- `player_attack_weapon`: `PHOTON_TORPEDO` または `PHASER`
- `enemy_reactions`: enemy id から `COUNTERATTACK` または `DEFEND` への mapping
- `salvage_choice`: command model では受理するが、現行の戦闘解決では消費しない。drop された `SALVAGE` reward 解決は `INTERACT` へ委譲しているためである

phase ごとの command 意味:

- `PLAYER_MOVEMENT`: 戦闘 phase を進める。この段階では movement payload を消費しない
- `PLAYER_ATTACK`: player attack または明示的な skip を解決する
- `ENEMY_ACTION`: 残っている enemy を tier 順にすべて解決し、その後 reaction を解決する

現行実装で固定されている reject 条件:

- 戦闘状態がない: `REJECTED_NO_COMBAT_STATE`
- target が存在しない: `REJECTED_TARGET_UNAVAILABLE`
- attack を選んだが weapon がない: `REJECTED_ATTACK_WEAPON_REQUIRED`
- 無効な attack weapon: `REJECTED_INVALID_ATTACK_WEAPON`
- target が射程外、または line of sight が blocked: `REJECTED_TARGET_NOT_ATTACKABLE`
- torpedo ammo が不足している: `REJECTED_INSUFFICIENT_TORPEDO_AMMO`
- phaser energy が不足している: `REJECTED_INSUFFICIENT_PHASER_ENERGY`

`COMBAT_REJECTED` は `srs_turn` を変更しない。

## 5. Player の戦闘リソースと capacity

現行の player 戦闘状態のデフォルト値:

| Field | Default |
|---|---:|
| `durability` | 100 |
| `durability_capacity` | 100 |
| `defense` | 0 |
| `evasion` | 0 |
| `movement_power` | 4 |
| `photon_torpedo_ammo` | 6 |
| `photon_torpedo_ammo_capacity` | 6 |
| `photon_torpedo_power` | 0 |
| `energy` | 6 |
| `energy_capacity` | 6 |
| `phaser_power` | 0 |
| `energy_recovery` | 1 |
| `salvage` | 0 |

現行の戦闘タイミング:

- torpedo ammo は、torpedo attack の実行が成功した場合にのみ消費する
- phaser energy は、phaser attack の実行が成功した場合、または `COUNTERATTACK` の解決が成功した場合にのみ消費する
- player energy は、`ENEMY_ACTION` の解決後かつ `combat_turn` increment 後に、`energy_recovery` だけ回復する
- `COMBAT_STEP` 自体は SRS fuel を消費せず、`srs_turn` も進めない

現行実装は、戦闘 upgrade を player state に直接保持する。

- `defense`
- `evasion`
- `phaser_power`
- `photon_torpedo_power`
- energy と torpedo ammo の拡張 capacity

現行の戦闘解決では、`defense`、`evasion`、`phaser_power`、`photon_torpedo_power` modifier をまだ適用しない。これらは永続的な player 戦闘状態および比較状態の一部として保持する。

## 6. Weapon profile

現行の固定 weapon profile:

| Weapon | Base damage | Range | Resource type | Resource cost | Power modifier field |
|---|---:|---:|---|---:|---|
| `PHASER` | 1 | 2 | `ENERGY` | 1 | `phaser_power` |
| `PHOTON_TORPEDO` | 3 | 3 | `PHOTON_TORPEDO_AMMO` | 1 | `photon_torpedo_power` |
| `ENEMY_WEAPON` | tier-based | 2 | none | 0 | none |

enemy の base damage は、enemy tier のデフォルト値から解決する。

| Enemy tier | Durability | Attack damage | Movement power |
|---|---:|---:|---:|
| `TIER1` | 3 | 6 | 3 |
| `TIER2` | 5 | 7 | 3 |
| `TIER3` | 8 | 8 | 3 |
| `TIER4` | 12 | 10 | 3 |

## 7. Targeting、射程、line of sight

現行の targeting 契約:

- player の target は `player_attack_target_id` で識別する
- target は `combat_state.enemies` 内に存在しなければならない
- player 側の attackability は player position から enemy position への関係で判定する
- enemy 側の attackability は enemy position から player position への関係で判定する

現行の距離 metric:

```text
combat_range_distance = max(abs(dx), abs(dy))
```

現行の line-of-sight 契約:

- line of sight は attacker と target の間にある Bresenham cell を使う
- attacker 自身と target 自身の cell は line of sight を block しない
- 中間の impassable cell は line of sight を block する
- impassable terrain / object のルールは `srs_movement.md` に従う

attack の reject 契約:

- 射程外、または line of sight blocked は `REJECTED_TARGET_NOT_ATTACKABLE` として解決する
- reject された attack は ammo も energy も消費しない

## 8. Player attack の解決

`PLAYER_ATTACK` における現行の解決順:

1. `player_attack_action` を読む。デフォルトは `SKIP`
2. action が `SKIP` なら、resource 変更なしの accepted `player_action` payload を emit する
3. `target_available` が `true` であることを検証する
4. player weapon が指定されており、かつ current player attack weapon のいずれかであることを検証する
5. 現在の target enemy に対して、射程と line of sight を検証する
6. 選択 weapon に必要な resource を検証する
7. ammo または energy を消費する
8. 固定 weapon damage を enemy durability に適用する
9. enemy が破壊された場合は `enemies` から除去する
10. 破壊された enemy が `drop_salvage = true` なら、drop された `SALVAGE` object helper を呼び、`salvage_drop` payload を付与する
11. enemy が生存した場合は、durability を減らした状態で保持する
12. 破壊された enemy が `player_attack_target_id` だった場合は、次の戦闘状態を保存する前に target id を clear する

現行の player attack payload field:

- `selected_action`
- `selected_weapon`
- `target_enemy_id`
- `attack_executed`
- `damage_applied`
- `resource_cost`
- `resource_type`
- `target_destroyed`
- enemy が生存した場合の `target_remaining_durability`
- drop された object が生成または skip された場合の `salvage_drop`

## 9. Enemy の行動順と解決

現行の enemy 行動順:

- `SrsCombatState` は enemy storage を tier 昇順で正規化する
- `ENEMY_ACTION` 中の iteration 順は、その正規化済み mapping に従う
- 同 tier 内の順序は、正規化前の insertion order に依存する。現行 test は tier 順のみを固定し、二次的な enemy-id sort は固定していない

残っている各 enemy に対する現行の行動フロー:

1. その enemy が、より前の reaction によってすでに除去されていれば slot を skip する
2. enemy が `ENEMY_WEAPON` で player をすでに攻撃できるか確認する
3. 攻撃できる場合は、攻撃と選択された reaction をただちに解決する
4. 攻撃できない場合は、player 周囲の attackable cell を計算し、最小 cost で到達可能な cell を選び、path に沿って最大 `movement_power` cell まで移動する
5. 移動後は、同じ `COMBAT_STEP` 内では攻撃しない。payload には最終位置から attackable になったかどうかだけを記録する

現行の enemy action payload 順序は、action 解決順と一致する。

## 10. 戦闘中の enemy movement

現行の movement 契約:

- `movement_power` は現行の全 enemy tier で 3 とする
- path search は、直交移動のみを対象とした Dijkstra を使う
- player の cell には入らない
- impassable terrain と impassable object cell は除外する
- 現行の path search では、enemy が占有している cell を特別扱いで block しない。enemy body collision の独立ルールは現行 test で未定義である
- 選ぶ target は、到達可能で attackable な位置のうち、総移動 cost が最小のものとする
- 同 cost の target cell は位置順 `(y, x)` で tie-break する
- 同 cost の path expansion は方向順 `N`, `W`, `E`, `S` で tie-break する
- 到達可能な attackable cell がない場合、enemy はその場に留まり、空 path を返す

現行の movement payload field:

- `target_attackable_position`
- `planned_path`
- `moved_path`
- `final_position`
- `movement_power`
- `movement_cost`
- `can_attack_before_move`
- `can_attack_after_move`

## 11. reaction契約

現行の baseline reaction:

```text
COUNTERATTACK
DEFEND
```

### `COUNTERATTACK`

現行の `COUNTERATTACK` 要件:

- その enemy に対して選択された reaction が `COUNTERATTACK` である
- player が `PHASER.energy_cost` 分の phaser energy を十分に持つ
- その enemy が、player position から phaser の射程内かつ line of sight 内にいる

現行の `COUNTERATTACK` 挙動:

- 戦闘 player state から phaser energy を 1 消費する
- enemy に固定 phaser damage 1 を与える
- enemy の outgoing attack damage は減らさない
- その enemy は、後続 enemy action の前に counterattack で破壊されてよい
- その enemy が `drop_salvage = true` なら、drop された `SALVAGE` payload を付与し、lifecycle を `srs_objects.md` へ委譲する

### `DEFEND`

現行の `DEFEND` 挙動:

- 受ける enemy damage を半減する
- `ceil(enemy.attack_damage * 0.5)` で切り上げる
- player energy と ammo は消費しない
- 現行実装は `defense` を追加 modifier として適用しない

### Fallback

現行の fallback ルール:

- `COUNTERATTACK` が要求されても、energy、射程、line of sight の要件を満たさない場合は `DEFEND` として解決する

reaction payload field:

- `selected_reaction`
- `resolved_reaction`
- `counterattack_available`
- `fallback_to_defend`
- `damage_to_player`
- `counterattack_damage`
- `enemy_destroyed`
- counterattack 破壊で drop helper が起動した場合の `salvage_drop`

## 12. Enemy 破壊と行動 skip

現行の破壊契約:

- player attack で破壊された enemy は `ENEMY_ACTION` 前に除去される
- counterattack で破壊された enemy は、その enemy の action 中に即時除去される
- 後続 iteration では、すでに current `updated_enemies` から除去された enemy id を skip する
- 破壊された enemy は、同じ `COMBAT_STEP` 中の後続 action を受けない
- `enemy_presence` は残っている enemy 数から導出する
- 最後の enemy を破壊した後も、phase は通常の遷移ルールに従って進む。現行実装は terminal combat phase を特別扱いしない

test と fixture で固定されている現行 regression には次を含む。

- torpedo kill された enemy には、その後の enemy action が発生しない
- counterattack kill は、その enemy の後続 action を skip する
- drop された `SALVAGE` は、即時 reward ではなく combat-to-object 境界として扱う

## 13. combat turn と SRS turn の計上

現行の計上契約:

- `COMBAT_STEP` は `srs_turn` を進めない
- reject された戦闘 action は `combat_turn` を進めない
- `combat_turn` は `ENEMY_ACTION` の解決後にのみ increment する
- player energy recovery は `ENEMY_ACTION` の後、かつ increment 後に適用し、`energy_capacity` で cap する
- `PLAYER_MOVEMENT -> PLAYER_ATTACK` と `PLAYER_ATTACK -> ENEMY_ACTION` の遷移では `combat_turn` を increment しない

現行 fixture 挙動の例:

```text
initial: phase=PLAYER_MOVEMENT, combat_turn=0
COMBAT_STEP -> PLAYER_ATTACK or ENEMY_ACTION, combat_turn=0
COMBAT_STEP at ENEMY_ACTION -> PLAYER_MOVEMENT, combat_turn=1, energy recovers
```

## 14. event と payload 契約

現行の combat event type:

- `COMBAT_TRANSITIONED`
- `COMBAT_REJECTED`

`COMBAT_REJECTED.payload` が含む内容:

- `command_type`
- 戦闘状態が存在した場合の `phase`
- `outcome`

`COMBAT_TRANSITIONED.payload` が含む内容:

| Field | Meaning |
|---|---|
| `command_type` | 現状は常に `COMBAT_STEP` |
| `phase_from` | 遷移前の combat phase |
| `phase_to` | 遷移後の combat phase |
| `combat_turn_before` | 更新前の turn counter |
| `combat_turn_after` | 更新後の turn counter |
| `enemy_presence` | 遷移 payload 構築時点で、遷移前に enemy が存在したか |
| `target_available` | 解決前に `player_attack_target_id` が生きた enemy を参照していたか |
| `target_attackable` | phase advance 時点で target が attackable だったか |
| `player_action` | player attack または skip payload。`PLAYER_ATTACK` 以外では `null` |
| `enemy_actions` | enemy action payload の順序付き list。`ENEMY_ACTION` 以外では空 list |
| `player_durability_before` / `after` | player durability の差分 |
| `player_energy_before` / `after` | player energy の差分 |
| `player_torpedo_ammo_before` / `after` | player torpedo ammo の差分 |
| `outcome` | 現状は `ACCEPTED` |

代表的な payload shape:

```text
COMBAT_TRANSITIONED.payload:
  phase_from
  phase_to
  combat_turn_before
  combat_turn_after
  player_action or enemy_actions
  player_durability_before/after
  player_energy_before/after
  player_torpedo_ammo_before/after
```

## 15. 永続状態と比較状態

fixture regression が使う現行の combat 比較項目:

- `combat_phase`
- `combat_turn`
- `enemy_presence`
- `combat_player_durability`
- `combat_player_energy`
- `combat_player_torpedo_ammo`
- `combat_enemy_positions`
- `combat_enemy_durabilities`

現行の combat state は次も保持しており、比較では event payload 経由で参照されうる。

- enemy id
- enemy tier
- enemy position
- enemy durability
- enemy `drop_salvage`
- player `salvage`
- player の戦闘 upgrade field
- `player_attack_target_id`

永続化の境界:

- `combat_state` は live な `SrsGameState` の一部である
- `SrsPersistentState` は combat phase、combat turn、enemy roster を保存しない
- そのため、現行の sector 永続化 model は、直列化された combat snapshot ではなく object / discovery 中心である
- この文書は、1 つの `SrsGameState` 内で command をまたいで保持される combat persistence を current behavior として扱い、それを超える sector revisit 時の combat 復元は定義しない

## 16. reward 境界

combat 側が責務を持つ境界:

- enemy が破壊されたかどうかを判定する
- `enemy.drop_salvage` を読む
- 必要に応じて drop された `SALVAGE` object helper を呼ぶ
- combat payload に `salvage_drop` 情報を付与する

`srs_objects.md` へ委譲する範囲:

- drop された `SALVAGE` object schema
- `INTERACT` による pickup
- salvage value の inventory 反映と recovery
- occupied-cell skip と object-id collision 処理の詳細

現行の combat spec は、kill 時点の legacy 即時 reward や `salvage_reward` 挙動を再導入しない。

## 17. 実装と regression の参照

実装:

- `experiments/galactic_exodus/srs/engine.py`
- `experiments/galactic_exodus/srs/model.py`

Regression test:

- `experiments/galactic_exodus/srs/test_engine_combat.py`
- `experiments/galactic_exodus/srs/test_fixture_regression.py`

Fixture:

- `experiments/galactic_exodus/srs/fixtures/combat_attack_blocked_los_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_attack_clear_los_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_attack_out_of_range_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_core_state_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_counterattack_salvage_drop_object_tier2_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_enemy_counterattack_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_enemy_counterattack_fallback_energy_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_enemy_defend_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_enemy_movement_tiebreak_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_energy_pressure_danger3_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_energy_pressure_danger4_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_phaser_attack_damage_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_salvage_drop_object_tier3_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_salvage_drop_occupied_cell_skip_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_salvage_drop_then_interact_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_salvage_no_drop_tier1_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_torpedo_destroy_no_counterattack_9x9.json`

traceability 用の issue / PR 履歴:

- #1178
- #1194
- #1195
- #1259
- #1275
- #1276
- #1296
- #1303
- #1304
- #1319

## 18. Deferred 項目

Deferred:

- hit probability
- evasion probability
- defense / evasion final tuning
- advanced combat probability and tuning: #1195
- final enemy AI tuning
- final weapon balance
