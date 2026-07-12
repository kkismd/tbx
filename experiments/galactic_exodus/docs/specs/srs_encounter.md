# Galactic Exodus SRS encounter仕様

Source issue: #1320
Parent issue: #1314
Decision inputs: #1178, #1194, #1278, #1297, #1298
Related: #1257, #1259, #1299, #1300, #1301, #1318
Base branch: `integration/882-galactic-exodus`

この文書は、Galactic Exodus Phase 2 SRS encounter の CURRENT_SOURCE である。

- issue comment、legacy spec、fixtures、実装は根拠または regression surface として扱う
- それらが矛盾した場合は、この文書を起点に同期する
- 新しい encounter rule や balance 値は追加しない
- `encounter.py` の current implementation は、一般ゲームプレイ中の自動 RNG encounter engine ではなく、固定入力を使う `FixedEncounterRoll` と fixture handoff を含む契約として読む

## 1. 文書の位置付けと正本性

この文書は、Phase 2 SRS encounter の現行仕様の正本である。

authority の優先順位:

1. merged 済みの specification decision issue と `experiments/galactic_exodus/docs/specs/`
2. current implementation と regression tests
3. legacy `experiments/galactic_exodus/srs/phase2_srs_spec.md`

根拠として参照する主な source:

- legacy baseline: `experiments/galactic_exodus/srs/phase2_srs_spec.md`
- 実装: `experiments/galactic_exodus/srs/encounter.py`, `experiments/galactic_exodus/srs/engine.py`, `experiments/galactic_exodus/srs/run_fixture.py`, `experiments/galactic_exodus/srs/model.py`
- 回帰: `experiments/galactic_exodus/srs/test_encounter.py`, `experiments/galactic_exodus/srs/test_fixtures.py`, `experiments/galactic_exodus/srs/test_fixture_regression.py`, `experiments/galactic_exodus/srs/fixtures/combat_encounter_*.json`

## 2. 対象範囲と他仕様との境界

この文書に含めるもの:

- `FixedEncounterRoll` を前提にした encounter roll の対象 command
- roll 実行条件、skip、suppression
- `base_encounter_chance_per_srs_turn` と terrain modifier
- `danger_score`、group budget、tier composition
- spawn candidate point、filter、player 近傍除外
- spawn position assignment、spawn cap、enemy id / order
- enemy `drop_salvage` の確定タイミング
- deterministic contract と current RNG 使用箇所
- `ENCOUNTER_ROLLED` event / payload
- combat への handoff 境界

他仕様に委譲するもの:

- combat phase / action / damage / reaction: `docs/specs/srs_combat.md`
- dropped `SALVAGE` object lifecycle / pickup / reward: `docs/specs/srs_objects.md`
- SRS movement / accepted turn / `WAIT` / `MOVE_ROUTE` / `MOVE_TO`: `docs/specs/srs_movement.md`
- map generation / terrain / warp flags: `docs/specs/srs_map_generation.md`, `docs/specs/srs_warp.md`
- integrated CLI の入力・描画: `docs/specs/integrated_cli.md`

この issue の対象外:

- encounter chance 再調整
- `danger_score` 再設計
- composition table 再調整
- new enemy tier 追加
- combat 仕様変更
- dropped `SALVAGE` object 生成・pickup 仕様変更
- spawn seed token 変更
- Python / fixture / test / JSON / CSV 変更

## 3. 遭遇判定タイミング

current implementation 上の encounter 判定フローは、`apply_srs_command(...)` の結果と `encounter_roll_disposition(...)` / `resolve_fixed_encounter_roll(...)` を組み合わせて扱う。

固定 encounter roll の current flow:

1. command を `apply_srs_command(...)` で解決する
2. command が accepted かどうかを event と state から判断する
3. `next_state.srs_turn > previous_state.srs_turn` かを確認する
4. `encounter_roll_disposition(...)` で skip / suppression / required を決める
5. disposition が `REQUIRED` なら `actual_encounter_chance(...)` を計算する
6. fixture / fixed input から `FixedEncounterRoll.roll_result` を受け取る
7. `roll_result == "success"` のときだけ `danger_score` と `composition` を使って spawn を解決する
8. spawned enemy ごとに `drop_salvage` を確定する
9. `ENCOUNTER_ROLLED` payload を構築する
10. `combat_state_from_fixed_encounter(...)` で combat state へ handoff する

補足:

- current `engine.py` は通常の `apply_srs_command(...)` 内で encounter roll を自動実行しない
- current prototype の encounter event 生成は、主に `run_fixture.py` から `resolve_fixed_encounter_roll(...)` を呼ぶ経路で固定されている
- したがって、この文書で「roll 実行条件」と書く部分は、live RNG encounter の一般実装ではなく、fixed encounter handoff の current contract を指す

## 4. 判定対象コマンドとturn条件

`encounter_roll_disposition(...)` が fixed encounter 判定対象として扱う command は次だけである。

```text
MOVE_ROUTE
MOVE_TO
WAIT
```

`REQUIRED` になる条件:

- `command_type` が `MOVE_ROUTE`、`MOVE_TO`、`WAIT` のいずれかである
- `next_state.srs_turn > previous_state.srs_turn`
- `previous_state` と `next_state` のどちらにも `enemy_presence` がない
- `is_base_docked(next_state)` ではない

その他 command の扱い:

- `INTERACT` は encounter roll 対象外
- `WARP_EXIT` は encounter roll 対象外
- `COMBAT_STEP` は encounter roll 対象外
- rejected command は `srs_turn` が進まないため encounter roll を行わない

## 5. skip・suppression条件

current reason token:

```text
SKIPPED_COMMAND
SKIPPED_NO_TURN_ADVANCE
SKIPPED_ENEMY_PRESENCE
SUPPRESSED_BASE_DOCKED
```

各 disposition の意味:

- `SKIPPED_COMMAND`
  `MOVE_ROUTE` / `MOVE_TO` / `WAIT` 以外の command である。encounter RNG は消費しない。`ENCOUNTER_ROLLED` event も生成しない。
- `SKIPPED_NO_TURN_ADVANCE`
  command 解決後も `srs_turn` が増えていない。encounter RNG は消費しない。rejected command や no-turn path がここに入る。
- `SKIPPED_ENEMY_PRESENCE`
  `previous_state` または `next_state` のどちらかに `enemy_presence` がある。combat 中の re-roll suppression を表す。encounter RNG は消費しない。
- `SUPPRESSED_BASE_DOCKED`
  `SectorType.BASE` かつ player が `STATION` 隣接で docked と判定される。`WAIT` などで `srs_turn` は進んでも encounter は抑止される。encounter RNG は消費しない。

current fixture / test 契約では、skip / suppression 時に `ENCOUNTER_ROLLED` event は追加されない。

## 6. encounter chance計算

current baseline:

```text
EXPECTED_SRS_TURNS = 4
ENCOUNTERS_PER_LRS_STEP = 0.75
BASE_ENCOUNTER_CHANCE_PER_SRS_TURN = 0.18
```

terrain modifier:

```text
NEBULA -> 0.7
other terrain -> 1.0
```

actual chance:

```text
actual_encounter_chance = BASE_ENCOUNTER_CHANCE_PER_SRS_TURN * terrain_encounter_modifier(player_cell_terrain)
```

modifier の参照元は `state.actual_map.cell_at(state.player_position).terrain` である。

current implementation 上の roll 判定について:

- `resolve_fixed_encounter_roll(...)` は chance 値を payload に記録する
- ただし success / failure 自体は current code では `FixedEncounterRoll.roll_result` で与えられ、`actual_encounter_chance` と数値比較して自動判定する live RNG path は未実装である

## 7. danger score入力

current `danger_score` 契約:

- 値域は `0..4`
- `encounter_group_budget_range(...)` と `encounter_composition_options(...)` の両方が `_validated_danger_score(...)` を通す
- 範囲外は `SrsEncounterError("danger_score must be in range 0..4")`

current implementation 上の入力元:

- success の `FixedEncounterRoll` は `danger_score` を必須とする
- failure の `FixedEncounterRoll` は `danger_score` を持ってはならない
- fixture の初期 `combat.encounter` でも fixed composition 入力として `danger_score` を使える

current implementation には、map state や LRS state から `danger_score` を自動導出するロジックは入っていない。

## 8. group budget範囲

current table:

| `danger_score` | budget range |
|---|---|
| 0 | `1..1` |
| 1 | `1..2` |
| 2 | `2..3` |
| 3 | `3..4` |
| 4 | `4..5` |

この budget は enemy tier ごとの group cost 合計に対する制約である。

## 9. tier costとcomposition候補

current tier cost:

| Tier | `enemy_group_cost` |
|---|---:|
| `TIER1` | 1 |
| `TIER2` | 2 |
| `TIER3` | 3 |
| `TIER4` | 5 |

current composition option table:

### `danger_score = 0`

| weight_percent | composition |
|---:|---|
| 100 | `TIER1` |

### `danger_score = 1`

| weight_percent | composition |
|---:|---|
| 70 | `TIER1` |
| 30 | `TIER1`, `TIER1` |

### `danger_score = 2`

| weight_percent | composition |
|---:|---|
| 50 | `TIER2` |
| 35 | `TIER1`, `TIER1` |
| 15 | `TIER2`, `TIER1` |

### `danger_score = 3`

| weight_percent | composition |
|---:|---|
| 45 | `TIER2`, `TIER1` |
| 30 | `TIER3` |
| 20 | `TIER1`, `TIER1`, `TIER1` |
| 5 | `TIER2`, `TIER2` |

### `danger_score = 4`

| weight_percent | composition |
|---:|---|
| 40 | `TIER3`, `TIER1` |
| 25 | `TIER2`, `TIER2` |
| 20 | `TIER2`, `TIER1`, `TIER1` |
| 10 | `TIER3`, `TIER2` |
| 5 | `TIER4` |

`validate_fixed_encounter_composition(...)` は次を要求する。

- composition が空でない
- tier cost 合計が budget range 内に入る
- 同じ `danger_score` の current option table のいずれかと tier multiset が一致する

順序そのものではなく `Counter(...)` による multiset 一致で照合するため、validation では composition の並び順は固定されない。

## 10. composition選択

current implementation は composition を RNG で自動選択しない。

現在の正本として固定できる contract は次である。

- `encounter_composition_options(danger_score)` が候補集合を返す
- `weight_percent` は option table 上に保持される
- success の `FixedEncounterRoll` または fixture 初期 encounter が、その候補の 1 つを `composition` として明示する
- `validate_fixed_encounter_composition(...)` がその候補であることを検証する

したがって current prototype では、`weight_percent` は current balance table の一部だが、一般ゲームプレイ中の選択 RNG はまだ文書化可能な実装経路を持たない。

## 11. spawn candidate point

spawn candidate は `spawn_candidate_points(state)` で決まる。

起点:

- `_warp_point_positions(state)` が返す passable warp point cell

warp point 判定:

- `cell.warp_flags` が空でない cell だけを候補にする

除外条件:

- `terrain` が `ASTEROID` または `RIFT_BARRIER`
- object が `STAR`、`PLANET`、`STATION`

補足:

- `RESOURCE_CACHE` や `SALVAGE` は warp point candidate 除外条件に含まれない
- candidate は `Position(y, x)` の昇順、正確には `(position.y, position.x)` で sort される

## 12. candidate filterとplayer近傍除外

`spawn_candidate_points(...)` は `_warp_point_positions(...)` の結果から、player を中心とする 3x3 を除外する。

除外条件:

```text
abs(position.x - player_position.x) <= 1
abs(position.y - player_position.y) <= 1
```

このため除外されるのは:

- player cell
- player の周囲 8 マス

candidate 不足時の current behavior:

- error にしない
- `apply_spawn_cap(...)` により、spawn できる数まで composition を切り詰める

## 13. spawn位置割り当て

`spawn_enemies_for_encounter(...)` の current flow:

1. `validate_fixed_encounter_composition(...)` で planned composition を検証する
2. `spawn_candidate_points(state)` で candidate を求める
3. `apply_spawn_cap(planned_enemies, len(candidates))` で tier list を切り詰める
4. `selected_tiers` と `candidates[:len(selected_tiers)]` を `zip(...)` する
5. 先頭から `enemy-1`, `enemy-2`, ... の id を振る
6. `create_enemy_combat_state(...)` で enemy state を作る

position assignment の current contract:

- candidate 側は `(y, x)` 昇順
- selected tier 側は `apply_spawn_cap(...)` 後の tier 昇順
- assignment はその 2 つを先頭から 1 対 1 で結ぶ

## 14. spawn cap

`apply_spawn_cap(...)` の current contract:

- `spawn_cap < 0` は error
- `spawn_cap == 0` なら空 tuple
- それ以外は、planned enemies をまず strongest-first で sort する
- strongest-first で先頭 `spawn_cap` 件を残す
- 残した tier を最後に ascending tier order へ戻す

実際の効果:

- candidate 数が足りないときは強い enemy を優先して残す
- 同 tier の tie-break は stable sort に依存し、tier 以外の新しい優先規則は追加しない

## 15. enemy identityと順序

enemy identity:

- id は `enemy-1`, `enemy-2`, ... の連番
- assignment 順に id が付く

combat state へ handoff した後の順序:

- `SrsCombatState` 側で enemy mapping は tier 昇順に正規化される
- そのため final `combat_state.enemies` も tier 昇順で観測される

current fixture regression で固定されていること:

- spawn cap fixture の final enemies 配列は `TIER1`, `TIER1`, `TIER2` の順になる
- wait-nebula fixture では `enemy-1` が唯一の spawned enemy になる

## 16. enemy SALVAGE drop判定

current `drop_salvage` 契約:

- encounter spawn 時に enemy ごとの `drop_salvage` を確定する
- combat 中には再 roll しない
- 確定値は `SrsEnemyCombatState.drop_salvage`、`salvage_drop_chance`、`salvage_drop_roll` に保持する

tier 別 chance:

| Tier | `salvage_drop_chance` |
|---|---:|
| `TIER1` | 0.25 |
| `TIER2` | 0.35 |
| `TIER3` | 0.50 |
| `TIER4` | 0.75 |

resolution:

```text
roll < chance  -> drop_salvage = true
roll >= chance -> drop_salvage = false
```

boundary 条件:

- `roll == chance` は failure
- `0.0 <= roll <= 1.0` 以外は error

fixture / fixed encounter override:

- fixture で enemy に `drop_salvage: true / false` を明示した場合、その値を優先する
- explicit 指定時は `salvage_drop_chance` と `salvage_drop_roll` は `None`
- explicit 指定時は random drop roll を消費しない

この文書は immediate reward や `salvage_reward` を再導入しない。destroy 後の object lifecycle は `srs_objects.md` に委譲する。

## 17. deterministic seed契約

current deterministic contract は 2 段階に分かれる。

### 17.1 `FixedEncounterRoll` 側

current implementation では、次は fixture / fixed input で与える。

- `roll_result`
- `danger_score`
- `composition`

したがって current code には、encounter chance に対する live RNG roll、budget 選択 RNG、composition 選択 RNG を生成する seed token はない。

### 17.2 enemy `drop_salvage` 側

enemy `drop_salvage` には専用 RNG がある。

seed token:

```text
{sector_id}|{sector_seed}|{srs_turn}|{player_x},{player_y}|{danger_score}|{composition_token}
```

`composition_token` は `selected_tiers` を `,` で結合した文字列である。

RNG 消費順:

1. `apply_spawn_cap(...)` 後の `selected_tiers` を決める
2. 上記 token で `random.Random(seed)` を初期化する
3. `selected_tiers` の順に `rng.random()` を 1 回ずつ消費する
4. 各 roll を `resolve_enemy_salvage_drop(...)` に渡す

current implementation では spawn position 自体は RNG ではなく deterministic sorting / slicing で決まる。

## 18. ENCOUNTER_ROLLED event / payload契約

event type は `ENCOUNTER_ROLLED` で固定する。

current failure payload:

- `command_type`
- `terrain`
- `terrain_modifier`
- `base_encounter_chance_per_srs_turn`
- `actual_encounter_chance`
- `roll_result = "failure"`
- `enemy_spawned = false`
- `outcome = "NO_ENCOUNTER"`

current success payload:

- `command_type`
- `terrain`
- `terrain_modifier`
- `base_encounter_chance_per_srs_turn`
- `actual_encounter_chance`
- `roll_result = "success"`
- `danger_score`
- `composition`
- `enemy_spawned = true`
- `spawned_enemy_ids`
- `spawned_enemies`
- `outcome = "ENCOUNTER_STARTED"`

`spawned_enemies[]` の field:

- `enemy_id`
- `enemy_tier`
- `position`
- `salvage_drop_chance`
- `salvage_drop_roll`
- `drop_salvage`

存在しない field は追加しない。current implementation には `reason`、raw encounter roll value、selected budget の payload key は存在しない。

## 19. combat handoff境界

この文書が責任を持つところ:

- encounter 成立判定
- enemy 生成
- initial `combat_state` 構築
- enemy ごとの `drop_salvage` 確定
- `ENCOUNTER_ROLLED` payload

`srs_combat.md` に委譲するところ:

- combat phase
- player attack
- enemy action
- damage
- destroy 後の action skip

`srs_objects.md` に委譲するところ:

- destroyed enemy からの dropped `SALVAGE` object lifecycle
- pickup / reward application

## 20. persistent / comparison state

encounter 後に比較対象として観測される current field:

- `combat_phase`
- `combat_turn`
- `enemy_presence`
- `combat_enemy_positions`
- `combat_enemy_durabilities`
- `combat_enemy_salvage_drops`

`combat_enemy_salvage_drops` の current summary には次が含まれる。

- `enemy_tier`
- `salvage_drop_chance`
- `salvage_drop_roll`
- `drop_salvage`

`SrsPersistentState` 自体は encounter 固有 field を持たない。encounter の結果は live `combat_state` に handoff され、その後の persistent object lifecycle は別仕様へ渡る。

## 21. 実装・回帰テスト参照

実装:

- `experiments/galactic_exodus/srs/encounter.py`
- `experiments/galactic_exodus/srs/engine.py`
- `experiments/galactic_exodus/srs/run_fixture.py`
- `experiments/galactic_exodus/srs/model.py`

回帰テスト:

- `experiments/galactic_exodus/srs/test_encounter.py`
- `experiments/galactic_exodus/srs/test_fixtures.py`
- `experiments/galactic_exodus/srs/test_fixture_regression.py`

fixtures:

- `experiments/galactic_exodus/srs/fixtures/combat_encounter_spawn_cap_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_encounter_wait_nebula_9x9.json`
- `experiments/galactic_exodus/srs/fixtures/combat_encounter_wait_base_docked_9x9.json`

関連 issue:

- #1178
- #1194
- #1257
- #1278
- #1297
- #1298
- #1299
- #1300
- #1301
- #1320

## 22. deferred項目

deferred:

- live RNG による encounter success / failure 判定の一般ゲームプレイ接続
- `danger_score` 自動導出
- budget / composition の RNG 自動選択
- spawn position seed token を明示的に仕様化するかどうかの再検討: #1301
- final encounter balance 調整: #1257
- final balance rationale の独立文書化
