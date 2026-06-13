# Phase 1参照fixture実装計画

## 1. 対象範囲

通常の生成ゲーム挙動を変更せず、Python参照実装へ決定的なfixture注入・再生機能を追加する。

## 2. `engine.py`の変更

### 2.1 `validate_actual_map`を追加する

```python
def validate_actual_map(actual_map: ActualMap, settings: GameSettings) -> None:
    ...
```

以下を明示エラーにする。

- 盤面座標の不足・余分
- `. N A @ B R S H`以外のセル記号
- S/Hが固定settings座標にない
- `base_position`のセルが`B`でない
- `resource_positions`のセルが`R`でない
- R座標の重複
- 断層辺が盤面外、非隣接、重複、未正規化

### 2.2 `create_game_from_actual_map`を追加する

```python
def create_game_from_actual_map(
    actual_map: ActualMap,
    *,
    settings: GameSettings = DEFAULT_SETTINGS,
    requested_seed: int,
    effective_seed: int,
    reroll_count: int,
) -> GameState:
    ...
```

次を行う。

1. settingsとactual mapを検証する
2. `create_game`と同じ初期状態を作る
3. HとS周囲3x3を開示する
4. pathをSで初期化する
5. `determine_game_status`を呼ぶ

`create_game`は盤面生成・選択後、この関数へ処理を委譲する。

### 2.3 `run_state_commands`を追加する

```python
def run_state_commands(
    state: GameState,
    commands: Iterable[str],
    *,
    max_turns: int = 256,
) -> GameLog:
    ...
```

現在の`run_commands`にあるコマンドループをこの関数へ移す。`run_commands`の既存signatureと通常挙動は維持し、生成済みstateを作った後に委譲する。

### 2.4 評価済みの敗北条件を維持する

`determine_game_status`と`can_continue`は変更しない。

Phase 1Bで評価済みの挙動は次である。

- H到着を最初に勝利判定する
- それ以外では、actual map上に「断層ではなく、現在燃料で移動先地形コストを支払える隣接辺」がなければ`LOST_FUEL`
- remaining fuelが正でも`LOST_FUEL`になり得る

通常UIへactual map由来の敗北理由や未発見断層を漏らさない。

### 2.5 盤面生成依存を注入可能にする

通常挙動を維持するdefault引数として、候補生成と到達判定を注入可能にする。

```python
CandidateGenerator = Callable[[int, int, float], simulate.GalacticMap]
ReachabilityPredicate = Callable[[simulate.GalacticMap], bool]


def create_playable_map(
    requested_seed: int,
    settings: GameSettings,
    *,
    generate_candidate: CandidateGenerator = simulate.generate_map,
    is_reachable: ReachabilityPredicate = is_goal_reachable,
) -> tuple[simulate.GalacticMap, int, int]:
    ...
```

この注入経路はgeneration error fixtureの決定的再生に使う。既存callerは変更不要とする。

## 3. 再生モジュール

以下を追加する。

```text
experiments/galactic_exodus/replay_phase1_reference.py
```

必須関数:

```python
load_fixture_file(path) -> dict
settings_from_dict(value) -> GameSettings
actual_map_from_dict(value) -> ActualMap
replay_fixture(fixture) -> GameLog
assert_partial_match(expected, actual, path="$") -> None
replay_all(path) -> None
```

CLI:

```bash
python experiments/galactic_exodus/replay_phase1_reference.py \
  --fixtures experiments/galactic_exodus/fixtures/phase1_reference.json
```

最初の不一致で非0終了し、fixture名とJSON pathを表示する。

## 4. actual map形式

`engine.actual_map_to_dict`と同じ形式を使用する。

```json
{
  "cells": [
    {"position": {"x": 1, "y": 1}, "symbol": "S"}
  ],
  "rift_edges": [
    [
      {"x": 1, "y": 1},
      {"x": 2, "y": 1}
    ]
  ],
  "base_position": {"x": 4, "y": 4},
  "resource_positions": []
}
```

- 64セルをすべて記録する
- 現行参照mapには必ずBがあるため`base_position`をnullにしない
- 全断層辺を正規化・ソートする

## 5. fixture mode

### `generated`

- `create_game(requested_seed, settings)`を実行する
- requested/effective/rerollメタデータを検証する
- 生成されたactual mapが`initial_actual_map`と完全一致することを検証する
- その生成stateへcommandsを適用する

### `injected`

- `initial_actual_map`をロードする
- `create_game_from_actual_map`を呼ぶ
- `run_state_commands`でcommandsを実行する

### `generation_error`

fixtureへ次を記録する。

```json
"generation_stub": {
  "reachable_sequence": [false]
}
```

seed overflow fixtureでは`requested_seed = 9223372036854775807`を使う。候補生成は正規の固定mapを返し、到達判定は最初の候補に`false`を返す。次candidate計算で実際の`SEED_OVERFLOW`を発生させる。

## 6. 12fixtureの修正

### no-reroll初期盤面

- 現行engineを実行して`reroll_count=0`を確認したseedを採用する
- 選択されたactual map全体をシリアライズする
- effective seedを手書きしない

### reroll requested/effective seed

- `reroll_count>0`となるseedを決定的に探索する
- actual metadataと選択map全体をシリアライズする
- seed 123がrerollするという仮定を置かない

### 通常移動・断層・B・R fixture

- S=(1,1)、H=(8,8)を維持する
- 全injected mapに有効なBセルと非nullの`base_position`を含める
- 64セルすべてを記録する

### 残量0でH到着

- S=(1,1)、H=(8,8)を維持する
- 経路上をすべてコスト1にする
- `initial_fuel=14`
- commandsは`E`を7回、その後`N`を7回
- 最後のH到着でfuel 0、status `WON`を検証する

### 燃料切れ敗北

- 現行`determine_game_status`が実際に`LOST_FUEL`と判定する固定map・command列を使う
- remaining fuelと、actual map上に支払える隣接移動がないことの両方を検証する

### generation error

- 上記の到達判定注入を使う
- reason、attempts、requested seed、last candidate seedを検証する

### turn limit

- injected map、`max_turns=1`、有効commandを2件以上使う
- event 1件と`ABORTED_TURN_LIMIT`を検証する

## 7. 比較規則

`expected_initial`、各`expected_turns[i]`、`expected_final`は再帰的partial matchとする。

- expectedにある全keyがactualに存在し一致する
- actual側の余分なkeyは許可する
- 配列は順序・indexを一致させる
- `known_cells_include`のような独自subset keyを作らない
- subsetが必要な場合も参照実装から得た正確な配列を記録する

## 8. テスト

以下を追加する。

```text
experiments/galactic_exodus/test_phase1_reference_fixtures.py
```

必須テスト:

- 12fixtureがすべて再生成功する
- fixture schema不一致を拒否する
- 不正actual mapを拒否する
- generated actual map不一致を拒否する
- expected turn不一致でfixture名とpathを表示する
- 注入経路でgeneration errorが発生する
- 固定S/Hの残量0到着fixtureが勝利する
- remaining fuelが正でもactual move不能なら`LOST_FUEL`となる既存挙動を保持する

`validate_phase1_spec.py`は静的構造検証のまま維持し、再生テストが意味的整合性を保証する。
