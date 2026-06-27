# Galactic Exodus Phase 2 SRS評価統合メモ

## 1. 対象と前提

本メモは issue #1162 の成果物として、Phase 2 SRS の移動・探索ルールに関する既存評価結果を decision issue へ渡しやすい形に整理したものである。

- 手動根拠:
  - `#1081` issue コメントの確認観点
  - `experiments/galactic_exodus/results/prototype_manual_sessions.csv`
  - 今回あらためて `experiments/galactic_exodus/srs/fixtures/*.json` を `run_fixture.py` で確認したメモ
- 自動根拠:
  - `experiments/galactic_exodus/srs/results/policy_runs.csv`
  - `experiments/galactic_exodus/srs/results/policy_summary.json`

注意:

- `#1081` 用の `srs/run_manual_eval.py` 出力 Markdown はリポジトリに保存されていない。
- そのため、手動根拠は「issue #1081 が要求した観点」「既存 prototype manual CSV の主観スコア」「今回の fixture 再確認」の3つに分けて扱う。
- 本メモは採否を最終決定しない。採否判断は #1163 以降で行う。

## 2. 手動評価の要約

### 2.1 prototype manual CSV から読めること

`experiments/galactic_exodus/results/prototype_manual_sessions.csv` は Phase 2 SRS 専用 runner の出力ではないが、探索価値、補給判断、断層理解、表示負荷に関する主観傾向を持っている。

- セッション数: 10
- 勝利: 9
- 燃料切れ敗北: 1
- `route_decision_score` 平均: 4.6
- `information_score` 平均: 4.3
- `fuel_tension_score` 平均: 4.2
- `supply_choice_score` 平均: 4.1
- `observation_range_score` 平均: 5.0
- `base_return_value_score` 平均: 4.4
- `rift_fairness_score` 平均: 2.3
- `readability_score` 平均: 2.6

主な観察:

- 探索と補給判断そのものは成立している。
- 観測範囲への不満は小さい。
- 問題は断層と使用済みリソースの読み取りであり、価値Objectや補給ルール自体よりも表示・説明の不足が強い。
- seed 2 と seed 6 では「Rへ寄る価値は感じるが、回復量や使用済み状態の見え方が弱い」という不満が出ている。

### 2.2 fixture 再確認から読めること

今回 `run_fixture.py` で 9 fixture を再確認し、最低限次を確認した。

- `move_route_basic_9x9.json`
  - 1 command で `MOVE_ROUTE` が進み、観測が更新される。
  - 価値Object `$` と warp flag `v` が map 上で同時に読める。
- `move_to_known_9x9.json`
  - 既知 map 上の自動移動として `MOVE_TO` が成立する。
- `resource_cache_single_9x9.json`
  - `INTERACT_ACCEPTED` と `OBJECT_CONSUMED` が分かれ、燃料 2 -> 7 を確認できる。
- `station_refuel_9x9.json`
  - `STATION_ACTIVATED` と満タン復帰を確認できる。
- `salvage_placeholder_9x9.json`
  - placeholder の取得と `OBJECT_CONSUMED` が分かる。
- `warp_exit_s_9x9.json`
  - warp flag 上で `WARP_EXIT_ACCEPTED` が 1 turn で完了する。
- `rift_blocked_n_9x9.json`
  - blocked edge では `WARP_EXIT_REJECTED` となり turn 非消費。
- `shared_fuel_cost_9x9.json`
  - `SHARED_FUEL` では 1 command で燃料が減る。
- `revisit_resource_consumed_9x9.json`
  - 使用済み RESOURCE_CACHE への再操作は `REJECTED_ALREADY_CONSUMED` かつ turn 非消費。

手動観察のまとめ:

- 仕様の機械的な一貫性は出ている。
- 一方で、プレイヤー視点の「何が blocked で、何が消費済みで、何がまだ価値Objectなのか」の説明負荷は残る。

## 3. 自動評価の要約

### 3.1 全体

`policy_runs.csv` は 8 case x 3 policies = 24 run。

- 総 run 数: 24
- `EXITED`: 15
- `ABORTED_NO_POLICY_ACTION`: 9
- `ABORTED_TURN_LIMIT`: 0
- `RESOURCE_DEPLETED`: 0
- `GENERATION_ERROR`: 0
- `median_srs_turn_count`: 9
- `p90_srs_turn_count`: 17
- `object_discovery_rate`: 0.666667
- `object_acquisition_rate`: 0.291667

### 3.2 policy 別の特徴

- `EXIT_GREEDY`
  - `exit_rate=0.5`
  - `median_srs_turn_count=4.5`
  - 最短脱出には寄るが、known route が作れない case を拾いきれない。
- `EXPLORE_THEN_EXIT`
  - `exit_rate=0.875`
  - `median_srs_turn_count=10.5`
  - 最も安定して出口へ到達する。NEBULA case も脱出している。
- `OBJECT_GREEDY`
  - `exit_rate=0.5`
  - `object_acquisition_rate=0.625`
  - STATION / RESOURCE / SALVAGE を最も積極的に回収するが、出口到達は安定しない。

解釈:

- 「出口へ向かうだけ」と「価値Objectへ寄る」のあいだに明確なトレードオフがある。
- したがって、SRS内の寄り道は少なくともルール上は成立している。

### 3.3 TURN_ONLY / SHARED_FUEL

- `TURN_ONLY`
  - `run_count=18`
  - `exit_rate=0.666667`
  - `object_acquisition_rate=0.333333`
  - `resource_use_rate=0.222222`
- `SHARED_FUEL`
  - `run_count=6`
  - `exit_rate=0.5`
  - `object_acquisition_rate=0.166667`
  - `resource_use_rate=0.0`
- `turn_only_vs_shared_fuel_failure_delta=-0.166667`

観察:

- 現状サンプルでは `TURN_ONLY` の方が失敗を増やしていない。
- `SHARED_FUEL` は BASE/STATION case では動くが、NORMAL case では policy が行動不能になりやすい。
- `TURN_ONLY` でも `p90_srs_turn_count=17` で、1 sector 内の所要 turn は破綻していない。

### 3.4 sector type 別の特徴

- `BASE`
  - `exit_rate=1.0`
  - `station_use_rate=0.333333`
  - STATION の価値は存在するが、全 policy が必ず使うほどではない。
- `RESOURCE`
  - `exit_rate=1.0`
  - `resource_use_rate=0.666667`
  - RESOURCE_CACHE は回り道の対象として十分機能している。
- `RIFT`
  - `exit_rate=1.0`
  - `blocked_edge_attempt_rate=0.0`
  - 仕様破綻は見えないが、自然さの人手評価は別途必要。
- `NEBULA`
  - `exit_rate=0.333333`
  - `observation_3x3_count=1` は `EXPLORE_THEN_EXIT` のみ
  - 3x3視界は成立しているが、探索成功率を押し下げている。
- `NORMAL`
  - `exit_rate=0.222222`
  - `no_policy_action_rate=0.777778`
  - baseline の通常区画は、detour価値よりも policy の探索能力不足が強く出ている。

## 4. 論点別観察

### TURN_ONLY / SHARED_FUEL

- 現状の自動評価では `TURN_ONLY` が優勢。
- 手動側でも「SRSで燃料を直接減らすべき」という強い根拠はなく、むしろ補給や表示の分かりやすさが主論点。
- `SHARED_FUEL` は比較条件として残す理由はあるが、baseline 候補としては弱い。

### RESOURCE_CACHE

- `resource-cache-first-visit` では `OBJECT_GREEDY` のみ取得し、`resource_use_rate=0.666667`。
- `resource-cache-revisit` は全 policy が脱出し、再訪後の状態保持も一貫している。
- 手動メモでは回復量自体への軽い不満はあるが、「寄る価値がゼロ」という評価ではない。

### STATION

- `base-station-first-visit` は全 policy が脱出し、`OBJECT_GREEDY` は `station_used=1`。
- `station_refuel_9x9` fixture でも `STATION_ACTIVATED` と満タン化が明確。
- 内部到達の意味はあるが、ルート最適化より「選択肢の一つ」として働いている。

### SALVAGE placeholder

- `salvage-placeholder-first-visit` では `OBJECT_GREEDY` が取得、他 policy は脱出優先。
- placeholder としての取得動作は成立している。
- ただし NORMAL sector 内で探索価値をどこまで押し上げるかはまだ弱い根拠しかない。

### NEBULA 3x3

- `nebula-local-3x3-first-visit` では `EXPLORE_THEN_EXIT` のみ脱出成功。
- 3x3 観測は実装どおり効いている。
- ただし現状では「特徴」と言うより「探索成功率を下げる制約」として見えやすい。
- threat / encounter と接続した価値づけは未実装であり、ここは後続設計に依存する。

## 5. Q1〜Q10 への暫定回答材料

### Q1. SRS移動はLRSの経路判断へ価値を追加するか

- 追加する材料あり。
- 根拠:
  - manual `route_decision_score=4.6`
  - manual `base_return_value_score=4.4`
  - `OBJECT_GREEDY object_acquisition_rate=0.625`
  - RESOURCE / SALVAGE / STATION へ寄る policy と直行 policy の差が出ている

### Q2. 9x9と11x11で1区画の探索時間は長すぎないか

- 9x9 baseline は現状許容。
- 11x11 は未評価。
- 根拠:
  - `p90_srs_turn_count=17`
  - 11x11 case が `policy_runs.csv` に存在しない

### Q3. warp可能領域までの移動が毎回単調にならないか

- 単調すぎるとまでは言えないが、根拠はまだ薄い。
- 根拠:
  - `EXPLORE_THEN_EXIT` は 17 turn 前後まで探索を継続
  - manual では補給判断・回り道の言及が複数ある
  - ただし多様性専用の case 群が足りない

### Q4. BASEとRESOURCEで内部到達させることに意味があるか

- ある。
- 根拠:
  - `station_use_rate=0.333333` in BASE
  - `resource_use_rate=0.666667` in RESOURCE
  - manual でも R/B を経路判断材料として扱っている

### Q5. NORMALの価値Object配置量は探索価値を生むか

- まだ判断材料不足。
- 根拠:
  - NORMAL では取得成功が薄い
  - SALVAGE placeholder の取得成功は主に dedicated case 依存

### Q6. RIFTのblocked edgeとwarp flag表現は自然か

- 仕様破綻は見えない。
- ただし自然さの人手根拠は弱い。
- 根拠:
  - `blocked_edge_attempt_rate=0.0`
  - `rift_blocked_n_9x9` で rejection が一貫
  - manual では断層の分かりにくさが残る

### Q7. warp flag付きセルからLRSとの方角対応を維持できるか

- 現状の fixture と policy run の範囲では維持できている。
- 根拠:
  - `warp_exit_s_9x9` で accepted
  - case 定義上、entry/selected exit と outcome が破綻していない

### Q8. 再訪時の状態保持は理解しやすいか

- 契約は成立している。
- 表示面は引き続き要整理。
- 根拠:
  - `resource-cache-revisit` が全 policy で成立
  - `revisit_resource_consumed_9x9` が `REJECTED_ALREADY_CONSUMED`

### Q9. LRSとSRSのfuel/turnを共通化すべきか

- 現状根拠では `TURN_ONLY` 優勢。
- 根拠:
  - `TURN_ONLY exit_rate=0.666667`
  - `SHARED_FUEL exit_rate=0.5`
  - `TURN_ONLY object_acquisition_rate=0.333333`
  - `SHARED_FUEL object_acquisition_rate=0.166667`

### Q10. SRSを毎回必須にするとテンポが悪化しないか

- 9x9単体では破綻していない。
- ただし multi-sector sequence と 11x11 が未評価。
- 根拠:
  - `p90_srs_turn_count=17`
  - 24 run は単一 sector 完結 case のみ

## 6. finding の整理

集計結果は `phase2_findings.csv` を正とする。要点だけ先に書く。

- `NO_CHANGE`
  - Q1 routing value
  - Q4 BASE / RESOURCE internal value
  - Q7 warp orientation
  - Q8 revisit persistence
  - Q9 TURN_ONLY baseline
- `ADJUSTMENT`
  - Q3 route variety evidence is thin
  - Q6 RIFT explanation / representation
- `PHASE_LATER`
  - Q5 NORMAL value-object amount
- `BLOCKER`
  - Q2 9x9 vs 11x11 exploration time
  - Q10 mandatory SRS tempo across sectors

## 7. decision issue へ渡すメモ

- `TURN_ONLY` は DECIDED 候補でよい。
- `RESOURCE_CACHE` と `STATION` は「存在意義あり」として decision log に進めてよい。
- `SALVAGE` は placeholder 維持を前提に、配置量や実効果は後続へ送るのが安全。
- `RIFT` はルール本体より表現改善の問題が強い。
- `NEBULA 3x3` は feature としては成立しているが、現状の根拠だけではストレスと価値のバランスをまだ断定しにくい。
- Q2 / Q10 は 11x11 と multi-sector sequence の欠落を明示したまま次へ渡す必要がある。
