# Galactic Exodus 表示入力仕様

Source issue: #1322
Parent issue: #1314
Depends on: #1313, #1314, #1317, #1319, #1320
Related: #1076, #1214, #1218, #1230, #1231, #1232, #1233, #1234, #1235, #1259, #1307, #1318, #1329
Base branch: `integration/882-galactic-exodus`

この文書は、Galactic Exodus の表示layerに対する `CURRENT_SOURCE` である。

- 対象は current implementation が受け取る表示入力、そこから導出してよい表示値、固定済み文言である
- design sample、legacy spec、fixtures、snapshots、tests、実装は根拠または回帰面であり、競合する正本ではない
- 新しい layout、glyph、payload key、model field、renderer behavior は追加しない
- 根拠が current implementation / regression tests で確定しない事項は `deferred` として分離する

## 1. 文書の位置付けと正本性

この文書は、LRS map、SRS map、compact HUD、event formatter、manual-eval 補助表示に関する current source である。

authority 優先順位:

1. merged decision issue と `experiments/galactic_exodus/docs/specs/` 配下の current docs
2. current implementation と regression tests
3. `experiments/galactic_exodus/docs/design/galactic_exodus_display_samples.md`
4. `experiments/galactic_exodus/srs/phase2_srs_spec.md`

参照根拠:

- 実装: `experiments/galactic_exodus/display.py`, `experiments/galactic_exodus/hud.py`, `experiments/galactic_exodus/event_format.py`, `experiments/galactic_exodus/integrated_play.py`, `experiments/galactic_exodus/srs/render.py`, `experiments/galactic_exodus/srs/event_format.py`, `experiments/galactic_exodus/srs/run_manual_eval.py`
- テスト: `experiments/galactic_exodus/test_display.py`, `experiments/galactic_exodus/test_hud.py`, `experiments/galactic_exodus/srs/test_render.py`, `experiments/galactic_exodus/srs/test_event_format.py`, `experiments/galactic_exodus/srs/test_run_manual_eval.py`, `experiments/galactic_exodus/srs/test_fixture_regression.py`
- design evidence: `experiments/galactic_exodus/docs/design/galactic_exodus_display_samples.md`

## 2. 対象範囲と他仕様との境界

この文書が固定するもの:

- display layer が受け取る raw input
- map / HUD / formatter が導出してよい表示値
- fixed wording と field contract only の境界
- LRS / SRS 座標の扱い
- normal / debug / manual-eval の責務境界
- `integrated_play.py` が map / HUD / summary block を組み立てる境界

他仕様へ委譲するもの:

- movement / observation / known-state 更新: [`srs_movement.md`](srs_movement.md)
- warp 成立条件と blocked edge rule: [`srs_warp.md`](srs_warp.md)
- object lifecycle / reward effect: [`srs_objects.md`](srs_objects.md)
- combat rule / phase / damage: [`srs_combat.md`](srs_combat.md)
- encounter roll / spawn payload: [`srs_encounter.md`](srs_encounter.md)
- integrated command parsing / loop: [`integrated_cli.md`](integrated_cli.md)

non-scope:

- 新しい表示 layout、panel、glyph
- renderer / HUD / formatter 実装変更
- model field / event type / payload key の追加
- snapshot 再生成
- debug 表示を normal 表示へ昇格する変更

## 3. raw input / derived value / wordingの区別

この文書では各項目を次の 3 種に分ける。

- raw input: renderer / formatter が直接受け取る model field、event type、payload key
- derived display value: raw input から決定的に導出してよい display coordinate、symbol、fallback `-`、enemy summary など
- user-facing wording: normal summary line や HUD 行の固定文字列、固定行構造

存在しない field や payload key を raw input として新設しない。

## 4. internal coordinate と display coordinate

SRS 座標契約は #1218 の current decision をそのまま採用する。

```text
internal coordinate:
  origin = lower-left
  0-origin
  x increases eastward
  y increases northward

display coordinate:
  origin = lower-left
  1-origin
  x increases eastward
  y increases northward
```

変換:

```text
display_x = internal_x + 1
display_y = internal_y + 1

internal_x = display_x - 1
internal_y = display_y - 1
```

current contract:

- `Position`、engine、fixture、validator、raw event payload は internal coordinate を使う
- `render_display_map(...)`、`render_compact_hud(...)`、normal event wording は display coordinate を使う
- `format_srs_debug_event(...)` と manual-eval 補助表示は internal/display pair を併記してよい
- normal HUD と normal summary は internal coordinate を表示しない

LRS 座標契約:

- LRS state 自体の `player_position`、`settings.start_position`、`settings.goal_position` は lower-left / 1-origin の display coordinate である
- LRS に SRS 用の `+1` / `-1` 変換 helper を適用しない

## 5. map row の保持順と走査順

SRS map row 契約:

```text
actual_map.cells[0] = south row
render は north row から south row へ走査する
```

current implementation:

- `render_row_for_internal_y(height, y)` は internal `y` を受け、row index `height - 1 - y` を返す
- `render_known_map(...)` / `render_known_map_spaced(...)` は internal north-to-south 順で行を出力する
- `render_display_map(...)` は display `y = height ... 1` で axis label を付ける
- LRS border-light map も north-to-south 順に行を出力する

## 6. LRS map入力契約

`render_lrs_border_light_map(state)` が利用する raw input:

- `state.player_position`
- `state.settings.start_position`
- `state.settings.goal_position`
- `state.known_cells`
- `state.used_resource_positions`
- `state.known_routes`

derived display value:

- outer border と x-axis label は固定 ASCII
- cell symbol は current known information だけから導出する
- blocked edge 表示は `known_routes[edge] == ROUTE_RIFT` の場合だけ導出する

secrecy contract:

- `actual_map.rift_edges` に存在するだけの未発見 edge は表示してはならない
- `known_routes[edge] == ROUTE_OPEN` は blocker として表示しない

current LRS cell symbol priority:

1. `player_position` -> `@`
2. `settings.start_position` -> `S`
3. `settings.goal_position` -> `H`
4. `used_resource_positions` かつ `known_cells[position] == "R"` -> `r`
5. `used_resource_positions` だが resource と確定していない -> `?`
6. `known_cells[position]` が `VALID_CELL_SYMBOLS` に含まれる -> その symbol
7. fallback -> `?`

## 7. SRS map入力契約

`render_known_map(...)`:

- fixture / validator / low-level compact body 用
- axis label を持たない
- 1 cell = 1 token の compact body を返す

`render_known_map_spaced(...)`:

- fallback / manual-eval body 用
- compact body と同じ symbol contract を、1 space 区切りで返す

`render_display_map(...)`:

- normal play 用
- display coordinate axis label を付ける
- combat target 表示がある場合は固定幅 token を使う

SRS renderer が使う raw input:

- `known_state.discovered_cells`
- `known_state.known_cells`
- `player_position`
- `objects`
- `combat_state.enemies`
- `actual_map.width`, `actual_map.height`
- `cell.terrain`
- `cell.object_id`
- `cell.warp_flags`

current renderer は `actor_id` や actor type を表示入力として使っていない。これらは必須入力として要求しない。

## 8. cell symbol と overlay優先順位

normal SRS overlay priority は current `render_display_map(...)` を正本とする。

```text
1. unknown / undiscovered cell -> ?
2. player                     -> @
3. visible enemy              -> enemy overlay
4. visible object             -> object glyph
5. visible warp flags         -> warp glyph
6. known terrain              -> terrain glyph
7. fallback                   -> ?
```

補足:

- issue #1322 の overlay class は `e` だが、current `render_display_map(...)` は combat enemy が存在する場合に `e1`, `e2`, ... の token を使う
- enemy overlay がない path では `render_display_map(...)` は `e` ではなく known/object/warp/terrain を返す
- `render_known_map(...)` / `render_known_map_spaced(...)` は combat enemy overlay を持たない

terrain glyph contract:

- `FLOOR` -> `.`
- `DEBRIS` -> `,`
- `NEBULA` -> `~`
- `ASTEROID_FIELD` -> `:`
- `ASTEROID` -> `#`
- `RIFT_BARRIER` -> `#`
- `GRAVITY_FIELD_VERTICAL` -> `.`
- `GRAVITY_FIELD_HORIZONTAL` -> `.`
- `RIFT_DISTORTION` -> `.`

object glyph contract:

- `STAR` -> `*`
- `PLANET` -> `o`
- `STATION` -> `S`
- `RESOURCE_CACHE` -> `R`
- `SALVAGE` -> `$`
- consumed `RESOURCE_CACHE` -> `r`
- consumed `SALVAGE` -> `s`

warp glyph contract:

- `frozenset({Direction.N})` -> `^`
- `frozenset({Direction.E})` -> `>`
- `frozenset({Direction.S})` -> `v`
- `frozenset({Direction.W})` -> `<`
- それ以外の非空 `warp_flags` -> `+`

## 9. player足元の重なり表現

normal display:

- `@` が最優先
- 足元の object / warp / terrain glyph は map 上では隠れる
- player cell の underlay を表す複合 glyph は追加しない

manual-eval overlay:

- `srs/run_manual_eval.py` の `_render_known_map_spaced_for_manual_eval(...)` は `render_display_map(...)` をベースに underlay 確認補助を追加できる
- 足元の underlay が `?` または `.` 以外のときだけ、`@` を隣接空白へ退避して underlay を残す
- これは manual-eval 専用 contract であり、normal renderer の必須 behavior ではない

## 10. known / discovered / unknown情報境界

current meaning:

- `discovered_cells`: 表示してよい cell の境界
- `known_cells`: renderer が参照する観測済み cell snapshot
- `visited_cells`: player が通過した履歴

display contract:

- `discovered_cells` に含まれない cell は `?` とし、terrain / object / enemy / warp を漏らさない
- `known_cells` は `discovered_cells` に対応する snapshot として symbol 決定に使う
- `visited_cells` は current renderer の symbol 決定に使っていないため、必須入力にしない
- `actual_map` は simulation の正本であり、unknown cell の描画根拠として直接使わない

## 11. warp point / blocked edge表示入力

SRS:

- `cell.warp_flags` は `^`, `>`, `v`, `<`, `+` の導出元である
- `RIFT_BARRIER` terrain は `#` の導出元である
- HUD の blocked summary は `descriptor.blocked_edges` と観測済み `RIFT_BARRIER` から導出する

LRS:

- `known_routes[edge] == ROUTE_RIFT` だけが border-light map の `|` / `---` 導出元である

この文書では次を再定義しない:

- `blocked_edges` の生成条件
- warp exit 可能条件
- unknown edge と blocked edge の game rule

## 12. compact HUD入力契約

`CompactHudContext` の current field:

- `lrs_state`
- `srs_state`
- `last_event_summary`
- `status`
- `cost_mode`

HUD は常に raw model を直接読むとは限らない。caller は display 向けに変換済みの summary を `CompactHudContext` に渡してよい。

current 8-line HUD が導出する値:

- `LRS` position
- sector type
- `SRS` display position
- sensor range
- LRS turn
- SRS turn
- cost mode
- fuel current / capacity
- status
- player durability / durability_capacity
- player energy / energy_capacity
- photon torpedo ammo / photon_torpedo_ammo_capacity
- salvage
- combat phase / selected enemy summary
- warp summary
- reward summary
- `last_event_summary`

fallback contract:

- value が存在しない場合は `-` を使う
- `status` は explicit 指定がなければ、`lrs_state` または `srs_state` があると `EXPLORING`、どちらもなければ `-`
- normal HUD は SRS internal coordinate を表示しない

current summary rules:

- sector type は `srs_state.descriptor.sector_type.value` を優先し、LRS-only では current cell symbol から `NORMAL` / `NEBULA` / `ASTEROID` / `GRAVITY` / `BASE` / `RESOURCE` / `START` / `HOME` / `UNKNOWN` へ変換する
- sensor range は `SectorType.NEBULA` のとき `3x3`、それ以外は `5x5`
- combat summary は `combat_state.enemies` が空なら `COMBAT  none`
- target enemy は `player_attack_target_id` を優先し、なければ `enemies.values()` の先頭を使う
- reward summary は player cell 上の reward object を優先し、なければ最短マンハッタン距離の未消費 reward object を使う

## 13. combat / enemy表示入力

map / HUD / formatter の責務を分ける。

map:

- raw input は `combat_state.enemies`
- discovered cell 上の enemy position から enemy overlay を導出する
- unknown cell 上の enemy は表示しない

HUD:

- raw input は `combat_state.phase`, `combat_state.enemies`, `player_attack_target_id`
- one-line enemy summary だけを出す

event formatter:

- raw input は `COMBAT_TRANSITIONED`, `COMBAT_REJECTED`, `ENCOUNTER_ROLLED` などの event type / payload
- combat damage rule や enemy action rule 自体は `srs_combat.md` に委譲する

`display.md` では曖昧な総称 `salvage_drop` を encounter payload の共通 field 名として使わない。current payload に存在する `drop_salvage`, `salvage_drop_chance`, `salvage_drop_roll` をそのまま参照する。

## 14. encounter表示入力

encounter rule の authority は [`srs_encounter.md`](srs_encounter.md) である。この文書は表示入力だけを固定する。

`ENCOUNTER_ROLLED` current failure payload:

- `command_type`
- `terrain`
- `terrain_modifier`
- `base_encounter_chance_per_srs_turn`
- `actual_encounter_chance`
- `roll_result = "failure"`
- `enemy_spawned = false`
- `outcome = "NO_ENCOUNTER"`

`ENCOUNTER_ROLLED` current success payload:

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

`spawned_enemies[]` current field:

- `enemy_id`
- `enemy_tier`
- `position`
- `salvage_drop_chance`
- `salvage_drop_roll`
- `drop_salvage`

重要な current boundary:

- skip / suppression 時には current implementation は `ENCOUNTER_ROLLED` event を生成しない
- `SKIPPED_*` や `SUPPRESSED_BASE_DOCKED` は disposition 判定の説明語であり、current event payload key ではない
- `combat_enemy_salvage_drops` は comparison summary state であり、`ENCOUNTER_ROLLED` payload key ではない

## 15. object interaction / reward表示入力

authority は [`srs_objects.md`](srs_objects.md) である。この文書は formatter が読む current payload key だけを固定する。

`INTERACT_ACCEPTED` / `INTERACT_REJECTED` current common payload:

- `command_type`
- `object_id`
- `object_type`
- `interaction_range`
- `effect`
- `position`
- `fuel_before`
- `fuel_after`
- `fuel_delta`
- `outcome`

player-state を伴う current payload:

- `player_durability_before`
- `player_durability_after`
- `player_energy_before`
- `player_energy_after`
- `player_torpedo_ammo_before`
- `player_torpedo_ammo_after`
- `salvage_before`
- `salvage_after`

resource / salvage / station 系で current implementation に存在する追加 key:

- `fuel_restore`
- `available_upgrades`
- `selected_upgrade`
- `applied_upgrade`
- `salvage_spent`
- `selected_salvage_choice`
- `reward_source`
- `salvage_value`
- `durability_delta`
- `energy_delta`
- `torpedo_delta`

`OBJECT_CONSUMED` current payload は object 種別に応じて次を持つ:

- `object_id`
- `object_type`
- `consumed`
- `outcome`
- `fuel_before` / `fuel_after` / `fuel_delta` または salvage reward key

`STATION_ACTIVATED` current payload は次を持つ:

- `object_id`
- `object_type`
- `fuel_before`
- `fuel_after`
- `fuel_delta`
- `activated`
- `reusable`
- `player_durability_before`
- `player_durability_after`
- `player_energy_before`
- `player_energy_after`
- `player_torpedo_ammo_before`
- `player_torpedo_ammo_after`
- `salvage_before`
- `salvage_after`
- `available_upgrades`
- `selected_upgrade`
- `applied_upgrade`
- `salvage_spent`

存在しない共通 field を作らず、event ごとに実在 key だけを扱う。

## 16. event formatter入力と出力責任

LRS:

- raw schema: `engine.TurnEvent`
- normal wording: `format_lrs_event_summary(...)`
- debug wording: `format_lrs_debug_event(...)`

SRS:

- raw schema: `srs/log.py` の `SrsTurnEvent`
- normal wording: `format_srs_event_summary(...)`, `format_srs_event_summary_lines(...)`
- debug wording: `format_srs_debug_event(...)`

manual-eval:

- section composition は `srs/run_manual_eval.py`
- formatter 自体を置き換えず、summary / map / HUD を束ねる

current supported SRS normal event types:

- `MOVE_ACCEPTED`
- `MOVE_REJECTED`
- `STOPPED_BEFORE_IMPASSABLE`
- `OBSERVATION_UPDATED`
- `INTERACT_ACCEPTED`
- `INTERACT_REJECTED`
- `OBJECT_CONSUMED`
- `STATION_ACTIVATED`
- `WARP_EXIT_ACCEPTED`
- `WARP_EXIT_REJECTED`
- `COMBAT_TRANSITIONED`
- `COMBAT_REJECTED`
- `ENCOUNTER_ROLLED`

fixed wording:

- `render_lrs_border_light_map(...)` の snapshot / shape
- `render_display_map(...)` の snapshot / axis / token width
- `render_compact_hud(...)` の 8-line row structure
- `format_srs_event_summary(...)` / `format_srs_event_summary_lines(...)` で exact-string test がある summary line

field contract only:

- `format_srs_debug_event(...)` の internal/display pair と compact payload exposure
- `format_lrs_debug_event(...)` の debug token 群
- unknown event fallback の `EVENT <event_type>`

## 17. command-response画面の組み立て境界

`render_integrated_response(...)` は current implementation で次の block order を組み立てる。

```text
RESULT
<summary lines>

LRS
<render_lrs_border_light_map(...)>

SRS
<render_display_map(...)>

HUD
<render_compact_hud(...)>
```

この文書が固定するのは renderer / HUD / formatter への input と返り値の契約である。

`integrated_cli.md` が責任を持つもの:

- command parsing
- command-response loop
- accepted / rejected command summary line の生成
- LRS / SRS mode selection

## 18. LRS / SRS切替時の入力

current display 切替契約:

- LRS map は `GameState` を入力に取る
- SRS map と HUD は `SrsGameState` または `CompactHudContext` を入力に取る
- integrated response は両 state を同時に保持してもよい
- HUD は `lrs_state` と `srs_state` の両方がある場合、LRS 座標と SRS display 座標を同時に表示する

normal display は mode 切替によって internal coordinate や debug payload を露出しない。

## 19. normal / debug / manual-evalの役割分担

normal:

- display coordinate のみ
- user-facing map
- compact HUD
- one-line または fixed multi-line summary

debug:

- raw payload
- internal/display pair
- compact token dump

manual-eval:

- evaluation 用 section 組み立て
- player cell detail
- underlay 補助表示
- detailed event summary の列挙

normal 表示へ debug / manual-eval 専用情報を要求しない。

## 20. design sampleとの役割分担

`docs/design/galactic_exodus_display_samples.md` は visual reference / design evidence であり、current display spec の正本ではない。

current boundary:

- `display.md`: input contract、coordinate contract、priority、responsibility boundary
- design sample: layout 例、比較観察、過去判断の補助根拠

両者が矛盾する場合は `display.md` を正本とする。ただし `display.md` は未実装の design sample behavior を current contract として採用しない。

## 21. 実装・回帰テスト参照

実装:

- `experiments/galactic_exodus/display.py`
- `experiments/galactic_exodus/hud.py`
- `experiments/galactic_exodus/event_format.py`
- `experiments/galactic_exodus/integrated_play.py`
- `experiments/galactic_exodus/srs/render.py`
- `experiments/galactic_exodus/srs/event_format.py`
- `experiments/galactic_exodus/srs/run_manual_eval.py`

回帰テスト:

- `experiments/galactic_exodus/test_display.py`
- `experiments/galactic_exodus/test_display_snapshot.py`
- `experiments/galactic_exodus/test_hud.py`
- `experiments/galactic_exodus/srs/test_render.py`
- `experiments/galactic_exodus/srs/test_display_snapshot.py`
- `experiments/galactic_exodus/srs/test_event_format.py`
- `experiments/galactic_exodus/srs/test_run_manual_eval.py`
- `experiments/galactic_exodus/srs/test_fixture_regression.py`

## 22. deferred項目

この issue では current implementation に存在しない次を追加しない。

- actor 表示や `actor_id` 表示入力
- encounter skip / suppression 専用 event type または payload
- normal map 上の player underlay 複合 glyph
- HUD への internal coordinate 表示
- blocked / exitable / unknown の専用 status field
- formatter 未対応 event type の新規 wording
- display sample の未実装 behavior の採用
