# Galactic Exodus Phase 1 TBX移植仕様

## 1. 位置づけ

本書は、Python参照実装とPhase 1B評価結果を基に、TBXへ移植するPhase 1仕様を確定する正本である。

- GameLog schema version: 3
- reference fixture schema version: 1
- decision register: `experiments/galactic_exodus/phase1_decisions.csv`
- Phase 1B findings: `experiments/galactic_exodus/results/prototype_findings.csv`

fixture schema version 1はfixtureファイル形式のversionであり、GameLog schema version 3とは別である。

## 2. 盤面と既知情報

### 2.1 固定盤面

- 盤面は8x8とする。
- 座標は1始まりの`(x,y)`とする。
- `(1,1)`をS、`(8,8)`をHとする。
- 北は`y+1`、東は`x+1`、南は`y-1`、西は`x-1`とする。

### 2.2 開始時の情報（INFO-001）

- Hはゲーム開始時から既知とする。
- S周囲の3x3範囲を開始時に開示する。
- 開示対象は地形、B、R、S、Hである。
- 未観測セルは通常UIへ表示しない。

### 2.3 移動後の情報（INFO-002）

- 成功移動後、新しい現在地周囲の3x3範囲を開示する。
- 開示済みセルは累積保持する。
- 失敗移動、拒否入力、無効入力ではセル情報を追加開示しない。

### 2.4 既知状態（INFO-003）

TBX版は以下を区別して保持する。

- `known_cells`: スキャンで開示されたセルと記号
- `visited_cells`: 実際に到着したセル
- `known_routes`: 実際に通過してOPENと確定した辺、または失敗試行でRIFTと確定した辺

actual map全体はゲーム内部だけが保持する。通常UI、入力候補表示、プレイヤー向けヘルプへactual mapを漏らさない。

## 3. 移動

### 3.1 入力（MOVE-001）

- 移動入力はN/E/S/Wである。
- 入力は前後空白を除去して大文字化する。
- それ以外は無効入力とする。

### 3.2 盤面外と無効入力（MOVE-002）

盤面外入力または無効入力では、以下を変更しない。

- player position
- remaining fuel
- turn count
- known cells
- visited cells
- known routes

`invalid_or_rejected_action_count`だけを1増やす。

### 3.3 未知断層（MOVE-003）

通常セル情報はスキャンで開示される。断層は通常センサーでは確定できず、航行試行時の空間応答によって初めて既知になる。

未知断層辺へ移動を試みた場合:

1. 移動は失敗する。
2. positionは変化しない。
3. fuelを1消費する。
4. turnを1増やす。
5. その無向辺を`RIFT`として`known_routes`へ追加する。
6. `rift_attempt_count`を1増やす。
7. 断層は以後の表示へ永続的に反映する。

### 3.4 既知断層（MOVE-004）

既知断層への入力は実行前に拒否する。

- position、fuel、turnは変化しない。
- `invalid_or_rejected_action_count`を1増やす。
- 追加の断層発見イベントは発生しない。

### 3.5 燃料不足（MOVE-005）

移動先地形コストまたは未知断層試行コストを支払えない場合は実行前に拒否する。

- position、fuel、turnは変化しない。
- `required_fuel`をTurnEventへ記録する。
- `invalid_or_rejected_action_count`を1増やす。

### 3.6 成功移動の順序（SUPPLY-004）

成功移動では次の順で状態を確定する。

1. 移動先地形コストを計算する。
2. 燃料不足なら拒否する。
3. 燃料を消費する。
4. positionを移動先へ更新する。
5. turnを1増やす。
6. 辺をOPENとして記録する。
7. visited cellsとpathを更新する。
8. 周囲3x3を開示する。
9. 到着セルの補給を適用する。
10. 勝敗を確定する。

## 4. 燃料と補給

### 4.1 燃料（FUEL-001）

- `initial_fuel=16`
- `max_fuel=16`
- Phase 1中はこの値を変更しない。

### 4.2 B（SUPPLY-001）

- B到着時に自動補給する。
- fuelをmax fuelまで即時回復する。
- 追加turnを消費しない。
- 利用回数制限を設けない。
- B到着ごとに`base_visit_count`を1増やす。
- 実際にfuelが増えた場合だけ`base_refuel_count`を1増やす。
- 満タン到着では`last_supply_source`を変更しない。

### 4.3 R（SUPPLY-002、SUPPLY-003）

- Rを盤面に3個配置する。
- 各R座標は1回だけ使用できる。
- 使用時は最大+5回復し、max fuelを超えない。
- 補給は自動かつ即時であり、追加turnを消費しない。
- 実際にfuelが増えた場合だけRを消費済みにする。
- 満タンで未使用Rへ到着した場合、そのRを消費しない。
- 使用済みRへ再訪しても補給しない。
- 別座標のRは独立して使用できる。
- `resource_visit_count`はR到着ごとに増やす。
- `resource_refuel_count`は実際にfuelが増えた場合だけ増やす。

Rの補給量+5はPhase 1仕様として確定する。完成版バランス調整では再評価できるが、Phase 1実装中は未決値として扱わない。

## 5. 勝敗とabort

### 5.1 勝利（WIN-001）

- Hへ成功移動して到着した場合は勝利する。
- 移動コスト支払い後のremaining fuelが0でも、H到着を勝利として扱う。

### 5.2 燃料切れ敗北（WIN-002）

勝敗確定時はH到着判定を先に行う。Hにいない場合、actual map上の隣接4方向について、断層でなく、現在のremaining fuelで移動先地形コストを支払える辺が1本もなければ`LOST_FUEL`とする。

- remaining fuelが0の場合は必ずこの条件を満たす。
- remaining fuelが正でも、すべての実隣接辺が断層または高コストで支払い不能なら`LOST_FUEL`となる。
- この判定はゲーム内部の勝敗確定だけがactual mapを参照する。
- actual map由来の候補、理由、未発見断層を通常UIへ表示しない。

この挙動はPhase 1Bで評価済みのPython参照実装を維持する。

### 5.3 abort（WIN-003）

次は通常敗北と区別する。

- generation error
- turn limit
- コマンド列終了
- 自動方策のaction生成不能または非進行反復

通常UIは勝敗判定以外の目的でactual mapを参照して行動候補を提示しない。

## 6. 再抽選とseed

### 6.1 到達可能性（GEN-001）

- actual map上でSからHへ到達可能な盤面だけを採用する。
- requested seedを最初のcandidate seedとする。
- 候補が到達不能ならseedを1ずつ増やす。
- 最大100候補を試す。
- 100候補すべてが到達不能ならgeneration errorとする。

### 6.2 seed記録（GEN-002）

必ず以下を保持・記録する。

- requested seed
- effective seed
- reroll count

candidate seedがsigned 64-bit範囲を超える場合はgeneration errorとする。

### 6.3 Python/TBX RNG方針（RNG-001）

**fixture注入方式を採用する。**

- 通常ゲーム生成についてPython RNGとの完全互換を必須としない。
- 統合テストではJSON fixtureに記録したactual mapをTBXへ注入する。
- Python/TBX一致テストは、同一actual mapと初期状態に対する状態遷移を比較する。
- 通常生成ではTBX側の決定的seed生成と再現可能性を維持する。

## 7. Phase 1 UI契約

### 7.1 セル記号

最低限、次を区別する。

- 未観測: `?`
- 通常既知空間: `.`
- 現在位置: `P`
- 開始地点: `S`
- 目的地: `H`
- ベース: `B`
- 未使用リソース: `R`
- 使用済みリソース: `r`
- 地形: Python参照実装と同じ`N`、`A`、`@`

現在位置を描画する場合でも、下にあるセル種別を状態として失わない。

### 7.2 断層表示（UI-002、UI-003）

- 発見済み断層はターンをまたいで永続表示する。
- 現在地周辺表示では、どの方向の辺が閉鎖されているか判別可能にする。
- 断層だけが通常スキャンで見えないことをhelp/ルール文へ明記する。
- Phase 1実装は意味情報を満たす表示を用意する。
- Braille Patterns、局所SRSの最終レイアウト、色、端末幅別表現は#1076で確定する。

### 7.3 使用済みR（UI-001）

- 未使用Rを`R`、使用済みRを`r`として永続表示する。
- 凡例に両者を記載する。
- 補給時と再訪時に直前イベント文を表示する。

### 7.4 通常HUD（UI-004）

通常HUDへ表示する項目:

- known stateから生成したマップ
- player position
- remaining fuel / max fuel
- turn count
- game status
- 直前の移動結果
- 直前の補給結果
- helpまたはコマンド一覧への導線

通常HUDへ常時表示しない項目:

- requested/effective seed
- reroll count
- B/R visit/refuel counters
- used resource positionsの完全一覧
- rift attempt count
- invalid/rejected action count
- fuel before/afterの詳細

これらはログまたはdebug表示で確認可能にする。

### 7.5 Phase 2へ送る表示事項（UI-005）

以下を#1076へ延期する。

- STTR1/Rogue中間の最終レイアウト
- Unicode Braille 2文字による断層表示
- 局所SRS境界図
- 80桁以下の端末幅対応
- Unicodeフォールバック
- スクリーンリーダー向け代替表示
- 色、点滅、装飾

## 8. GameLog schema v3

### 8.1 GameLog必須項目（LOG-001）

- schema_version
- settings
- requested_seed
- effective_seed
- reroll_count
- initial_state
- events
- final_summary
- generation_error

### 8.2 TurnEvent必須項目

- turn
- command
- outcome
- from_position
- attempted_position
- to_position
- fuel_before
- fuel_spent
- fuel_after
- required_fuel
- discovered_cells
- discovered_rift
- supply_result
- supply_source
- fuel_before_supply
- fuel_after_supply
- supply_amount
- status_after

### 8.3 final summaryと継続計測（LOG-002）

- outcome
- turn_count
- remaining_fuel
- max_fuel
- used_resource_positions
- base_visit_count
- base_refuel_count
- resource_visit_count
- resource_refuel_count
- last_supply_source
- rift_attempts
- invalid_or_rejected_actions
- path

## 9. Python/TBX一致契約

### 9.1 一致必須

- requested seed
- effective seed
- reroll count
- 注入fixtureのactual map
- initial known state
- 各turnのcommand outcome
- position
- remaining fuel
- known cells
- visited cells
- known routes
- supply result/source/amount
- turn count
- game status
- final outcome

### 9.2 一致不要

- 内部配列レイアウト
- 型名、関数名、補助変数
- 一時キャッシュ
- 仕様で固定しない末尾空白、色、装飾
- 通常生成時のPython/TBX seed-to-map完全一致

## 10. reference fixture

- ファイル: `experiments/galactic_exodus/fixtures/phase1_reference.json`
- fixture schema version: 1
- 座標: `{"x": int, "y": int}`
- 無向辺: 2座標を辞書順に並べた2要素配列
- actual mapはGameLog initial stateと同じ`cells`、`rift_edges`、`base_position`、`resource_positions`形式で完全記録する。
- known routes: `{edge, state}`の配列
- fixture名は一意とする。

必須fixture:

1. no-reroll初期盤面
2. reroll発生
3. 通常地形移動
4. 未知断層移動失敗
5. 既知断層再試行
6. B補給
7. R補給
8. 2回目R補給なし
9. 残量0でH到着勝利
10. 燃料切れ敗北
11. generation error
12. turn limit abort

すべてのfixtureはCIでPython参照実装へロードして再生し、記載された期待フィールドと実結果を比較する。

## 11. #1040/#1047から維持する事項

- fixed 8x8 board
- S=(1,1)、H=(8,8)
- 到達不能盤面を再抽選する
- requested/effective seedとreroll countを記録する
- generation failureを通常敗北と区別する
- Phase 1初期値としてfuel 16、R=3、R +5を使用する

## 12. #1058から変更・追加した事項

- 使用済みRを`r`として区別する。
- 発見済み断層を永続表示する。
- 断層が通常スキャンで見えず航行試行時に確定することを明記する。
- 通常HUDとログ/debug情報を分離する。
- 表示詳細を#1076へ送る。

ゲームルールの数値変更は行わない。

## 13. 見送った案

- B利用回数制限: 固定解化の証拠がないため採用しない。
- B補給量逓減: 同上。
- R補給量増加: 表示問題と数値問題を分離できないため採用しない。
- 断層失敗コスト削除: 不公平感の主因が表示であり、コスト過大の証拠がないため採用しない。
- Python RNG完全互換: 言語間互換コストが高く、fixture注入で状態遷移一致を検証できるため採用しない。

## 14. TBX実装順

1. #1050 盤面生成
2. #1051 再抽選とseed
3. #1052 移動と既知情報
4. #1053 燃料・補給・勝敗
5. #1054 Phase 1最低表示契約
6. #1055 入力とゲームループ
7. #1056 fixture注入とPython/TBX統合検証

#1050を最初の着手Issueとする。
