# Galactic Exodus 仕様 traceability 棚卸し

Issue: #1260
Base branch: `integration/882-galactic-exodus`
作成日: 2026-07-06

この文書は、issue 上で確定した仕様が repository 内のどこへ反映されているかを確認するための traceability matrix である。
仕様本文を完全移植するものではなく、どの決定が code / tests / fixtures / docs に反映済みか、どの決定に正本ファイルまたは後続 issue が必要かを見えるようにする。

## 対象範囲

主対象:

```text
#1078 以降の Galactic Exodus 関連 issue
```

参照対象:

```text
#1078 より前でも、Phase 2 以降の仕様・実装が依存している親 issue / baseline issue
```

主な参照 issue:

```text
#882   Galactic Exodus 全体計画 / integration branch
#902   Phase 0 固定マップと断層航路モデル
#1040  Phase 0 初期推奨値
#1049  Phase 1 実装 tracker
#1059  Phase 1C TBX 移植仕様決定
#1073  Phase 1B 断層辺制約
#1076  Phase 2 表示 baseline
```

この棚卸しで行わないこと:

```text
- gameplay 実装変更
- 数値バランス変更
- fixture / snapshot 再生成
- issue本文の仕様正本ファイルへの完全移植
- docs/specs 配置ルールの最終決定（#1259で扱う）
```

## 分類ルール

issue 分類:

| 分類 | 意味 |
|---|---|
| A | 仕様確定 issue |
| B | 実装 issue |
| C | 調査・管理・評価 issue |
| D | obsolete / not planned / 統合済み issue |
| E | この棚卸しの対象外 |

反映状況:

| 状態 | 意味 |
|---|---|
| `implemented` | 現在の prototype 範囲で必要な code / tests / fixtures / docs へ反映済み。 |
| `partial` | 一部反映済み。ただし正本 docs がない、または意図的に後続へ送っている実装面がある。 |
| `missing` | issue 上で決定済みだが、この棚卸しでは repository 反映先が見つからない。 |
| `obsolete` | 後続 issue または新しい正本に置き換え済み。 |
| `needs_decision` | 仕様決定なのか実装メモなのか判別が必要。 |

## issue 棚卸し

この一覧は #1078 以降の Galactic Exodus 関連 issue と、依存する baseline issue を中心にしている。
純粋な実装 issue は仕様正本ではなく実装反映先として扱う。

| Issue | 分類 | この棚卸しでの位置づけ | メモ |
|---:|---|---|---|
| #1076 | A | 表示 baseline 参照元 | `docs/design/galactic_exodus_display_samples.md` を入力として LRS/SRS/HUD/log baseline を固定。 |
| #1078 | C | Phase 2 SRS探索 parent | SRS探索モデル全体の tracker。具体仕様は子 issue 側にある。 |
| #1079 | C | Phase 2A 初期モデル・評価準備 | 初期仮説と評価条件。後続の #1085〜#1089 / #1083 で精緻化。 |
| #1080 | B | SRS prototype 実装 | Phase 2A model の実装 carrier。 |
| #1081 | C | 手動評価 | 評価 issue。後続で参照される finding を除き、安定仕様の正本ではない。 |
| #1082 | C | エージェント自動評価 | 評価 issue。後続で参照される finding を除き、安定仕様の正本ではない。 |
| #1083 | A | SRS移動・探索ルール統合決定 | 手動評価・自動評価を統合し、SRS移動・探索仕様へ接続。 |
| #1085 | A | SRS要素体系 | SectorType / terrain / feature / object / actor の分離と必要属性を定義。 |
| #1086 | A | SRS terrain 効果 | 通行可否、移動コスト、観測範囲、terrain/object互換性の具体値。 |
| #1087 | A | SRS object 状態・永続化 | object interaction と使用後状態の規則。 |
| #1088 | A | SRS map generation / WARP | terrain count profile、`warp_flags`、2x2 FLOOR cluster、RIFT_BARRIER規則。 |
| #1089 | A | SRS movement command 詳細 | movement command 解決と turn 処理。SRS移動docsから参照すべき。 |
| #1130 | B | SRS入力耐性・再開 | 手動評価からの実装 follow-up。 |
| #1132 | B | player/object 重なり表示 | 表示修正。独立した仕様正本ではない。 |
| #1134 | B | interaction event summary 詳細化 | event wording/detail 実装。 |
| #1136 | B | fixture 初期可視セル調整 | fixture/test 整合。 |
| #1137 | C/D | #1082でSHARED_FUEL詳細値を固定しない整理 | 後続の fixture regression / balance decision で扱う。 |
| #1138 | B | SRS fixture regression tests | test coverage issue。 |
| #1178 | C/A | combat / encounter / SALVAGE 管理 | #1194 を現在の combat 初期パラメータ正本として宣言し、統合済み決定を管理。 |
| #1179 | D | 旧 enemy/threat model issue | not planned。#1194 / #1178 に統合。 |
| #1180 | D | 旧 spawn/warp/terrain modifier issue | not planned。#1194 / #1178 に統合。 |
| #1181 | D | 旧 chase_pressure issue | not planned。#1194 / #1178 に統合。 |
| #1182 | D | 旧 enemy detection / warp restriction issue | not planned。#1194 / #1178 に統合。 |
| #1183 | D | 旧 combat rules issue | not planned。#1194 / #1178 に統合。距離減衰など古い案が残る可能性あり。 |
| #1184 | D | 旧 weapon / ammo / energy issue | not planned。#1194 / #1178 に統合。距離減衰など古い案が残る可能性あり。 |
| #1185 | D | 旧 SALVAGE effect issue | not planned。#1194 / #1178 に統合。 |
| #1186 | D | 旧 enemy AI / progression issue | not planned。#1194 / #1178 に統合。 |
| #1187-#1193 | D | 旧 simulation 分解 issue | 当面 #1194 により supersede。 |
| #1194 | A | combat / encounter 初期パラメータ | SRS combat、encounter率、weapon stats、enemy tier、enemy action、spawn composition の現行初期モデル。 |
| #1214 | C/A | 表示サンプル作成 | `docs/design/galactic_exodus_display_samples.md` を作成し #1076 の入力にした。 |
| #1218 | A | SRS座標方針 | internal 0-origin lower-left / display 1-origin lower-left を固定。 |
| #1220-#1223 | B | 座標方針実装 | fixture / validator / tests / render を #1218 へ同期。 |
| #1230 | C | 表示実装影響範囲調査 | #1076 表示 baseline 実装前の調査。 |
| #1231 | B | LRS border-light renderer | #1076 LRS baseline の実装。 |
| #1232 | B | SRS display renderer | #1076 SRS baseline の実装。 |
| #1233 | B | compact HUD | #1076 HUD baseline の実装。 |
| #1234 | B | log/debug event wording | #1076 wording baseline の実装。 |
| #1235 | B | display snapshot / fixture | #1076 表示 baseline の regression coverage。 |
| #1241 | A | integrated CLI command-response / EXIT 決定 | command loop、response panel順、parser正規化、EXIT駆動LRS移動の正本。 |
| #1242 | B | integrated CLI 土台 | #1241 command-response skeleton の実装。 |
| #1243 | B | SRS movement command 接続 | #1241 movement command mapping の実装。 |
| #1244 | B | EXIT / LRS移動接続 | #1241 EXIT transition の実装。 |
| #1245 | B | INTERACT command 接続 | #1241 interaction command path の実装。 |
| #1250 | B | readline / stdin decode 耐性 | CLI robustness 実装。 |
| #1252 | B | 初期SRS player display=(1,1) | integrated CLI 初期位置の targeted implementation decision。 |
| #1254 | B | `srs/generate.py` warp_flags同期 | #1088 WARP rule の実装修正。 |
| #1255 | B | `integrated_play.py` minimal SRS warp_flags同期 | #1088 WARP rule の実装修正。 |
| #1257 | C | encounter spawn / combat balance 再確認 | #1254/#1256 後の open follow-up 候補。#1178/#1194 を入力にする。 |
| #1259 | C/A | 仕様正本配置・運用 | 将来の `docs/specs/galactic_exodus/` 配置と運用手順を定義。 |
| #1260 | C | この棚卸し | 本 traceability matrix を作成。 |

## traceability matrix

| 仕様領域 | Source issue | 決定概要 | 期待されるrepo反映先 | 現在の反映状況 | 状態 | Gap / action |
|---|---:|---|---|---|---|---|
| Phase 2 表示 baseline | #1076 | border-light LRS macro map、north-to-south SRS map、compact HUD、one-line last event、debug/log分離、ASCII fallbackを採用。 | `docs/design/galactic_exodus_display_samples.md`、LRS/SRS renderer、HUD、event formatter、display snapshot。 | `docs/design/galactic_exodus_display_samples.md`、#1231 LRS renderer、#1232 SRS renderer、#1233 compact HUD、#1234 event wording、#1235 snapshot。 | `implemented` | #1259 後に `docs/specs/galactic_exodus/display.md` へ canonical summary を追加またはミラーする。 |
| SRS座標契約 | #1218 | engine / fixture / validator / raw payload は internal 0-origin lower-left、render / manual eval / HUD / docs は display 1-origin lower-left。 | SRS model / tests / fixtures / render / manual docs。 | #1220〜#1223 で座標変換・表示同期を実装。#1076 もこの方針を参照。 | `implemented` | 将来の display spec に短い正本メモを置き、upper-leftやdisplay 0-originの再導入を防ぐ。 |
| SRS要素体系 | #1085/#1086 | SectorType、terrain、feature/object/actor を分離。terrainの通行可否・移動コスト・観測効果を定義。現行terrain setでは汎用`WALL`を使わない。 | `phase2_srs_elements.md`、JSON、validator、tests、model enum、movement/observation engine。 | `experiments/galactic_exodus/srs/phase2_srs_elements.md`、JSON、validator/tests、model enumに反映済み。 | `implemented` | 既存docに古い`WARP_POINT`用語が残る。#1259後のspec作成時にcleanupする。 |
| SRS object状態・interaction | #1085/#1087 | STATIONは隣接interaction・再利用可。RESOURCE_CACHE/SALVAGEは同一セルinteraction、使用後は除去。STAR/PLANETはstatic impassable。使用/取得はSRS turnを1消費。fuel満タン時のstation/cacheはno-op。 | SRS model/object state、interaction engine、fixtures/tests、event formatter。 | object type/stateは`srs/model.py`に存在。#1245でintegrated CLIへINTERACT接続。#1085コメントに#1087決定が記録済み。 | `partial` | object lifecycle の canonical spec が必要。fuel満タンno-opとturn消費のtest coverageもspec追加時に再確認する。 |
| SRS WARP flags | #1088 | `WARP_POINT`、辺中央固定、Feature warp point、WarpZoneを廃止。各FLOOR cellが方向別`warp_flags`を持つ。辺に接する2x2 FLOOR clusterを構成する外周cellにflagを付与。四隅は2方向を持ち得る。 | `srs/generate.py`、`srs/test_generate.py`、`integrated_play.py`、`test_integrated_play.py`、render/HUD/docs。 | #1254で`srs/generate.py`を更新。#1255でminimal integrated SRSを同期。各PRでtest更新。 | `partial` | #1259配下で`srs_warp.md`を作成する。古いdocの`WARP_POINT`表現も、現行仕様を指す箇所は更新する。 |
| RIFT edge / RIFT_BARRIER対応 | #1088 | RIFT blocked edgeは対応方向のwarp flagを禁止し、RIFT_BARRIERを配置する。non-blocked edgeは通常の2x2 FLOOR warp rule。銀河外縁方向はwarp flag禁止。 | SRS generator、RIFT fixtures/tests、LRS EXIT validation、renderer/HUD wording。 | `srs/generate.py`は`descriptor.blocked_edges`方向のwarp flagをskipし、RIFT_BARRIERを配置。integrated CLIはblocked/out-of-bounds EXITをreject。 | `partial` | `create_sector()`はboard境界情報を持たないため、non-blocked directionをopen扱いする。LRS descriptor統合時に解消、または制限として明文化する。 |
| SRS terrain density / generation profile | #1088 | `obstacle_density`をやめ、SectorType別terrain count range / limitで生成する。FLOORは残余。passability/terrain countはSectorTypeとmap sizeに依存。 | generator、generation contracts/fixtures、validator/tests、generation notes/spec。 | #1088コメントに決定あり。現行`srs/generate.py`は多くの経路でminimal all-floor + barrier generatorのまま。 | `partial` | full terrain-count profileを実装するか、deferredとして`srs_map_generation.md`へ明記する後続判断が必要。 |
| SRS移動・探索ルール | #1083/#1089 | 手動/自動評価から、movement command解決、observation update、cost model、revisit persistenceを含むSRS移動・探索ルールを確定。 | SRS engine、fixtures、regression tests、docs。 | SRS engine/tests/fixturesは存在。#1138でfixture regression追加。`phase2_srs_elements.md`に観測・移動関連のterrain効果を記録。 | `partial` | `srs_movement.md`を作るか、`srs_map_generation.md` / `integrated_cli.md`へ分割して記録する。移植前に#1083/#1089の最終決定本文を再監査する。 |
| combat初期player/enemy stats | #1194/#1178 | player durability=100、movement_power=4、torpedo ammo=6、energy=6、recovery=1。enemy movement_power=3。torpedo damage/range=3/3、phaser damage/range=1/2。enemy tier statsはT1=3/6、T2=5/7、T3=8/8、T4=12/10。 | `srs/model.py`、combat tests、HUD。 | `srs/model.py`にplayer default、weapon profiles、enemy tier defaults、enemy movement_powerが反映済み。 | `implemented` | `srs_combat.md`を追加し、#1178はfull specではなくindex/managementとして扱う。 |
| encounter率・terrain modifier | #1194/#1178 | `T_srs_expected=4`、`E_base_per_lrs_step=0.75`、`base_encounter_chance_per_srs_turn=0.18`、NEBULA modifier=0.7、その他terrain=1.0。 | encounter module、tests、balance notes。 | `srs/encounter.py`に`EXPECTED_SRS_TURNS=4`、`ENCOUNTERS_PER_LRS_STEP=0.75`、`BASE_ENCOUNTER_CHANCE_PER_SRS_TURN=0.18`、NEBULA modifier=0.7がある。 | `implemented` | `srs_encounter.md`を追加する。#1257はWARP/spawn変更後のrecheck follow-upとして維持。 |
| encounter group budget / tier composition | #1178/#1194 | danger score別budget rangeとfixed tier composition table。spawn capでは強いenemyを残し、行動配列は弱い順。 | encounter module、tests。 | `srs/encounter.py`にgroup cost、budget range、composition table、spawn cap、tier sort orderがある。 | `implemented` | #1257後にspawn-cap truncationとaction-order sortのfixture coverageを確認する。 |
| enemy spawn candidate points | #1178/#1194 | passable warp pointsからspawn。player cellと周囲8マスは除外。enemy_presence中はrollしない。combat中に追加spawnしない。 | encounter module、engine turn advancement tests。 | `srs/encounter.py`はwarp positionsから候補を作り、player周囲3x3を除外。 | `partial` | engine側のroll suppression / no-additional-spawnが明示的にtestされているか確認し、`srs_encounter.md`へ記録する。 |
| enemy action model | #1194/#1178 | 攻撃できなければ攻撃可能位置へ移動し、攻撃できれば攻撃。enemy rangeはphaser rangeと同じ2。破壊済みenemyはcounterattackしない。 | combat/engine実装、tests。 | combat statsは`srs/model.py`に反映済み。この棚卸しでは独立した`srs/combat.py`は見つからなかった。 | `partial` | enemy actionの実装面とtest coverageを追加監査し、不足があればfocused follow-upを作る。 |
| SALVAGE combat/resource効果 | #1178/#1194および#1185統合 | SALVAGE inventoryとrecovery/upgrade choiceはmodel conceptとして存在。具体的な適用タイミングは旧sub issueでは固定しない扱い。 | model、interaction、combat/resource recovery tests、future base upgrade docs。 | `SrsSalvageChoice`と`SrsBaseUpgrade` enumは`srs/model.py`に存在。効果・lifecycle specはencounter/combat constantsほど明確でない。 | `needs_decision` | 現在のSALVAGE挙動を固定仕様にするかprototype-onlyにするか、後続decision issueまたは`srs_combat.md` / `balance.md`で決める。 |
| integrated CLI command-response loop | #1241 | 単一`COMMAND>` loop。parse/execute/render。出力順は`RESULT`, `LRS`, `SRS`, `HUD`, optional `LOG`。LRS/SRSは入力modeではなくresponse panel。 | `integrated_play.py`、`test_integrated_play.py`、README。 | #1242でskeleton追加。testsでstartup sectionsとparser behaviorを確認。#1250でstdin耐性追加。 | `implemented` | #1259後に`integrated_cli.md`を追加する。 |
| integrated CLI movement commands | #1241/#1243 | `N/E/S/W`と`MOVE ...`はSRS内移動のみ。直接方向commandではLRS positionを変更しない。 | `integrated_play.py`、tests。 | #1243でSRS movement接続。testsでdirection commandがSRSだけを変更することを確認。 | `implemented` | `integrated_cli.md`に例を載せる。 |
| integrated CLI EXIT command | #1241/#1244/#1255 | LRS positionを変えるのは`EXIT <dir>`のみ。現在SRS cellのmatching warp flag、board内destination、known blocked RIFTなし、combat/fuel等の制約を満たす場合に成功。 | `integrated_play.py`、`test_integrated_play.py`、LRS engine。 | #1244でEXIT接続。#1255でminimal SRS warp flags同期。#1252/#1255でlower-left out-of-bounds rejectionを確認。 | `partial` | combat/fuel制約はfuture/needed constraintとして書かれている。minimal CLIでの実装済み/非対象を`integrated_cli.md`で明確化する。 |
| integrated CLI INTERACT | #1241/#1245 | `INTERACT`は現在SRS cellまたは適用可能objectに対するinteractionを実行し、command resultを返す。 | `integrated_play.py`、tests、SRS interaction engine。 | #1245でconnectionとminimal SRS object placementを実装。 | `implemented` | canonical docs追加時にno-target/cache/station/salvage例を載せる。 |
| 初期SRS player位置 | #1252 | integrated CLI新規gameはinternal=(0,0), display=(1,1)から開始。EXIT後entry pointは従来どおり。 | `integrated_play.py`、`test_integrated_play.py`、display/HUD snapshots。 | #1252 closed。testsで`Position(0,0)`とlower-left discovered windowを確認。 | `implemented` | 将来の本格SRS生成がminimal start placementを置き換える場合のみ再検討。 |
| readline / stdin decode耐性 | #1250 | readlineなしやstdin decode errorでもtracebackにせず終了できる。 | `integrated_play.py`、tests。 | decode errorでtracebackなしにsession終了するtestあり。 | `implemented` | integrated CLI operations noteに残す程度でよい。 |
| 仕様正本運用 | #1259 | 将来の正本は`docs/specs/galactic_exodus/`配下とし、issue決定だけで完了扱いにしない運用を定義する。 | 新規docs/specs layoutとREADME。 | この棚卸し時点で#1259はopen。 | `partial` | 本棚卸しを#1259へ入力する。このfileは#1259の最終配置決定後に移動・再配置される可能性がある。 |

## Gap / 後続 issue 候補

| 優先度 | Gap | 後続 action 候補 |
|---:|---|---|
| 1 | #1088 WARP決定は概ね実装済みだが、canonical `srs_warp.md` がない。古いdocに`WARP_POINT`も残る。 | #1259後に`docs/specs/galactic_exodus/srs_warp.md`を作成し、現行仕様を指す箇所の`WARP_POINT`を`warp_flags`へ更新する。 |
| 2 | #1088 terrain-count generation profile が minimal generator へ明確に反映されていない。 | full terrain-count profileを実装するか、deferredとして`srs_map_generation.md`へ明記する。 |
| 3 | `create_sector()`は銀河外縁方向を判定できず、descriptor上non-blockedな方向をopen扱いする。 | SRS descriptor pathへLRS board境界情報を追加するか、full LRS/SRS generation integrationまで制限として明文化する。 |
| 4 | combat constantsは反映済みだが、enemy action flowとdestroyed-enemy counterattack skipの実装・test traceが弱い。 | engine/combat testsを監査し、coverage不足ならfocused follow-upを作る。 |
| 5 | SALVAGE効果・適用タイミングがcombat/encounter constantsほど固定されていない。 | decision issueを作るか、`srs_combat.md` / `balance.md`へ統合して固定する。 |
| 6 | #1083/#1089のSRS移動・探索最終決定に直接対応するcanonical docがない。 | `srs_movement.md`を作るか、`integrated_cli.md`とSRS engine specへ分割して記録する。 |
| 7 | integrated CLI EXIT specにはfuel/combat制約が書かれているが、minimal CLIがすべて実装しているとは限らない。 | `integrated_cli.md`で現在のimplemented constraintとdeferred constraintを分けて書く。 |

## 棚卸しメモ

- code searchだけを正本にしない。未実装仕様はcode searchでは見つからないため。
- #1179〜#1186はactive source issueとして扱わない。#1178が#1194をcombat初期パラメータの現行正本として明示しているため。
- #1254/#1255は、#1088の決定がissueコメントに留まり、repository反映が遅れたことで必要になった修正である。この事実は、#1088にcanonical spec fileが必要であることを示している。
- この文書はtraceability matrixであり、最終的な仕様本文集ではない。
