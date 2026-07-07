# Galactic Exodus integrated CLI specification

Source issue: #1241
Implementation issues: #1242, #1243, #1244, #1245, #1250, #1252, #1255
Traceability audit: #1260
Follow-up: #1268, #1279, #1281

この文書は、Galactic Exodus integrated command-response CLI の正本仕様である。
#1268 では特に `EXIT <dir>` 制約の implemented / deferred を明確化した。
#1279 では non-EXIT command surface を補完し、SRS移動入力を `MOVE <route>` に一本化する方針を正本化する。
#1281 では、敵エンカウンター後の SRS combat phase を含む command-response loop を正本化する。

## CLIの基本方針

integrated CLI は、単一の command-response loop として動作する。

```text
COMMAND> <user input>
```

入力modeを LRS / SRS で分けない。
LRS map、SRS map、HUD は入力modeではなく response panel として表示する。
combat中も別入力modeには切り替えず、同じ `COMMAND>` loop のまま phase に応じて受理commandを切り替える。

## response panel順

各command後の出力順は次を基本とする。

```text
RESULT
LRS
SRS
HUD
LOG  optional
```

現行 `integrated_play.py` では `RESULT`, `LRS`, `SRS`, `HUD` を返す。
詳細LOG panelは必要に応じて後続で追加する。

## command parsing

入力は次の正規化を受ける。

```text
- 前後空白を除去
- uppercase化
- commaをspaceへ変換
- 連続空白を1つへ圧縮
- control character / backspaceを入力処理で吸収
```

CLI command surface:

| Command | 意味 | LRS position change | SRS position change |
|---|---|---:|---:|
| `MOVE <route>` | SRS内route移動 | no | yes, if accepted |
| `WAIT` | 探索中に移動せずSRS turnを進める | no | no |
| `INTERACT` | SRS object interaction | no | no |
| `EXIT <dir>` | SRSから隣接LRS sectorへ移動 | yes, if accepted | yes, new sector entry |
| `ATTACK <target>` | combat player action | no | no |
| `ATTACK <weapon> <target>` | weapon指定つきcombat player action | no | no |
| `SKIP` | combat player attack phaseで攻撃しない | no | no |
| `COUNTER` | enemy attackへのphaser counterattack | no | no |
| `DEFEND` | enemy attackへのdefend reaction | no | no |
| `LOOK` | 現在状態を見る | no | no |
| `STATUS` | 状態を見る | no | no |
| `HELP` | command help | no | no |
| `Q` / `QUIT` | session終了 | no | no |
| unknown command | 未知command / parser error | no | no |

`MOVE <route>` は SRS movement専用である。
LRS positionを変更するのは `EXIT <dir>` のみである。

`N` / `E` / `S` / `W` は direction token としては有効だが、standalone CLI command としては受け付けない。

```text
MOVE N        accepted as MOVE route=(N)
MOVE N E W S  accepted as MOVE route=(N,E,W,S)
N             rejected as unknown command or explicit rejected standalone direction
E             rejected as unknown command or explicit rejected standalone direction
S             rejected as unknown command or explicit rejected standalone direction
W             rejected as unknown command or explicit rejected standalone direction
```

理由:

```text
- Phase 2 baselineでは movement_points_per_turn = 4
- 1 accepted movement command = 1 SRS turn
- `N` を4回入力すると4 SRS turnを消費し、1turnに最大4step移動できる設計と直感的にずれる
- `MOVE <route>` に一本化すると、ユーザー入力単位とSRS turn境界が一致する
- 単独方向入力を後から1turnにまとめる方式は、入力途中状態 / cancel / partial preview / encounter timing が複雑になる
```

現行実装では、`parse_integrated_command(...)` が `N/E/S/W` 単独入力を `COMMAND_MOVE` として受け付ける。
これは後続実装issueで `COMMAND_UNKNOWN` または explicit rejected command へ変更する。

## non-EXIT commands

### LOOK

`LOOK` は現在状態を再表示するcommandである。

```text
accepted = true
LRS position change = no
SRS position change = no
SRS turn change = no
```

### STATUS

`STATUS` は現在のship / sector状態を確認するcommandである。

```text
accepted = true
LRS position change = no
SRS position change = no
SRS turn change = no
```

現行minimal CLIでは、`STATUS` の詳細はHUD表示に寄せている。
必要なら後続で、status専用summaryを拡張する。

### HELP

`HELP` は使用可能commandを案内するcommandである。

```text
accepted = true
LRS position change = no
SRS position change = no
SRS turn change = no
```

HELPは、現在phaseで使用可能なcommandを案内する。
exploration phaseでは canonical commandとして `MOVE <route>` を案内する。
`N/E/S/W` 単独commandは案内しない。
combat phaseでは `ATTACK`, `SKIP`, `COUNTER`, `DEFEND` など、現在phaseで受理可能なcommandを案内する。

### Q / QUIT

`Q` / `QUIT` はsessionを終了するcommandである。

```text
accepted = true
should_quit = true
LRS position change = no
SRS position change = no
SRS turn change = no
```

### INTERACT

`INTERACT` は、現在のSRS positionに対して object interaction を試行するcommandである。

対象objectとrange条件は `srs_movement.md` を正本とする。

```text
RESOURCE_CACHE  SAME_CELL
SALVAGE         SAME_CELL
STATION         ADJACENT
```

accepted interaction は 1 SRS turn を消費する。
rejected interaction は SRS turn を消費しない。

現行minimal integrated CLIでは、target object idをCLI引数では指定しない。
現在位置・隣接位置から候補を探し、優先順で1つを選ぶ。
候補がない場合はrejectする。

### MOVE <route>

`MOVE <route>` は、SRS内movementのcanonical CLI commandである。

```text
MOVE <dir> [<dir> ...]
```

`<dir>` は direction token の列である。

```text
N
E
S
W
```

commaはspaceへ正規化されるため、次は同じrouteとして扱う。

```text
MOVE E,E,N
MOVE E E N
```

Phase 2 baselineでは、通常床で最大4stepまでを1 SRS turnとして解決する。
terrain movement cost、impassable cell、movement budgetにより途中停止する場合がある。
詳細は `srs_movement.md` の `MOVE_ROUTE` を正本とする。

`MOVE <route>` は1 accepted movement commandとして扱う。
routeが `movement_power` 未満で終わっても、そのcommandにより移動フェイズは終了し、SRS turn resolutionへ進む。

```text
MOVE N:
  1stepだけ移動
  movement_powerは余る
  移動フェイズは終了
  SRS turn resolutionへ進む
```

受理例:

```text
MOVE N
MOVE N E
MOVE N E W S
```

reject例:

```text
MOVE
MOVE X
MOVE N X
```

invalid directionを含む場合はrejectし、LRS / SRS positionを変更しない。

### WAIT

`WAIT` は、exploration phaseで移動せずにSRS turnを進めるcommandである。

```text
accepted = true, only in EXPLORATION
LRS position change = no
SRS position change = no
SRS turn advances = yes
encounter roll = yes, when enemy_presence is false
```

`WAIT` は「移動フェイズを任意に終了する」ための明示commandであり、movement_powerを消費しない。
ただし、SRS turnを進めるため、enemy_presenceがない場合は通常どおりencounter判定の対象となる。

### standalone direction token

`N` / `E` / `S` / `W` 単独入力は、CLI commandとしては非対応とする。

```text
N  rejected
E  rejected
S  rejected
W  rejected
```

ただし、これらは `MOVE <route>` と `EXIT <dir>` の direction token としては引き続き有効である。

```text
MOVE N
EXIT N
```

この区別により、SRS engine側の `MOVE_ROUTE` direction token仕様と、integrated CLI command surfaceを分離する。

### unknown command / parser error

未知commandや parser がcommandとして解釈できない入力はrejectする。

```text
accepted = false
LRS position change = no
SRS position change = no
SRS turn change = no
```

ユーザー向けsummaryは次を基本とする。

```text
COMMAND rejected: unknown command
```

standalone direction tokenを unknown command に丸めるか、次のような明示rejectにするかは後続実装issueで決める。

```text
MOVE rejected: use MOVE <route> instead of standalone direction
COMMAND rejected: use MOVE <route> for SRS movement
```

## CLI phase model

integrated CLI は、入力modeを増やさず、内部状態としてCLI phaseを持つ。

初期整理では、少なくとも次のphaseを定義する。

```text
EXPLORATION
COMBAT_PLAYER_ACTION
COMBAT_REACTION
GAME_OVER
```

必要なら詳細phaseとして、後続で次を分ける。

```text
SRS_MOVEMENT_PHASE
SRS_TURN_RESOLUTION
SRS_ENEMY_ACTION_PHASE
```

phase別のcommand availabilityは次を基本とする。

| Command | EXPLORATION | COMBAT_PLAYER_ACTION | COMBAT_REACTION | GAME_OVER |
|---|---:|---:|---:|---:|
| `MOVE <route>` | yes | no | no | no |
| `WAIT` | yes | no | no | no |
| `INTERACT` | yes | no | no | no |
| `EXIT <dir>` | yes | no | no | no |
| `ATTACK <target>` | no | yes | no | no |
| `ATTACK <weapon> <target>` | no | optional | no | no |
| `SKIP` | no | yes | no | no |
| `COUNTER` | no | no | yes | no |
| `DEFEND` | no | no | yes | no |
| `LOOK` | yes | yes | yes | no |
| `STATUS` | yes | yes | yes | no |
| `HELP` | yes | yes | yes | no |
| `Q` / `QUIT` | yes | yes | yes | yes |

phaseに合わないcommandはrejectし、LRS / SRS positionとSRS turnを進めない。

## combat phase command loop

敵エンカウンターは、新しい入力modeではなく、SRS commandの結果として `COMBAT_PLAYER_ACTION` へ遷移するeventとして扱う。

```text
EXPLORATION
  MOVE <route>
    -> movement resolved
    -> encounter roll if enemy_presence is false
    -> no encounter: EXPLORATION
    -> encounter: COMBAT_PLAYER_ACTION

  WAIT
    -> SRS turn advances
    -> encounter roll if enemy_presence is false
    -> no encounter: EXPLORATION
    -> encounter: COMBAT_PLAYER_ACTION

  INTERACT
    -> interaction resolved
    -> EXPLORATION

  EXIT <dir>
    -> exit resolved
    -> EXPLORATION in next sector
```

combat中は `MOVE`, `WAIT`, `INTERACT`, `EXIT` は受理しない。
特に `EXIT <dir>` は SRS engine側でも `combat_state.enemy_presence` によりrejectされる。

### COMBAT_PLAYER_ACTION

`COMBAT_PLAYER_ACTION` では、プレイヤーは攻撃するか、攻撃せずにskipするかを選ぶ。

```text
ATTACK <target>
ATTACK <weapon> <target>
SKIP
```

`ATTACK <target>` は、初期CLIでは既定武器または実装側の現行既定に委譲する。
weapon選択をCLIに露出する場合は、次の形式を使う。

```text
ATTACK PHASER E1
ATTACK TORPEDO E1
```

`SKIP` は、player attack phaseで攻撃せずにenemy action sequenceへ進めるcombat系commandである。
combat / enemy_presence active のため、追加のencounter rollは行わない。

```text
SKIP:
  player attack = none
  SRS turn/action sequence advances
  enemy action phase follows
  additional encounter roll = no, because combat / enemy_presence is active
```

### COMBAT_REACTION

`COMBAT_REACTION` では、enemy攻撃ごとにプレイヤーは `COUNTER` または `DEFEND` を選ぶ。

```text
COUNTER
DEFEND
```

`COUNTER` は初期モデルではphaser counterattackのみを扱う。
photon torpedo counterattackは扱わない。

```text
COUNTER:
  use phaser only in initial model
  photon torpedo counterattack = not supported
```

counterattack不能条件は #1199 の方針を参照する。

```text
counterattack unavailable when:
  target is out of range
  line of sight is blocked
  phaser energy is insufficient
```

counterattackが不能な場合は、`DEFEND` へfallbackする。

```text
COUNTER
  -> if available: phaser counterattack
  -> if unavailable: DEFEND fallback
```

`DEFEND` はincoming damageにdefend軽減を適用する。
具体的なdamage計算は #1199 または将来の `srs_combat.md` を正本とし、`integrated_cli.md` ではCLI入力契約のみを扱う。

reaction phaseでは `SKIP` を作らない。
enemy攻撃への非反撃は `DEFEND` に寄せる。

### WAIT / SKIP / encounter roll policy

`WAIT` と `SKIP` は分けて扱う。

```text
WAIT:
  exploration phase command
  movement phaseを行わずにSRS turnを進める
  enemy_presence が false の場合、通常どおり encounter判定対象

SKIP:
  combat player attack phase command
  攻撃せずにenemy actionへ進む
  combat / enemy_presence active のため、追加の encounter roll は行わない
```

将来、EXPLORATION に `END_MOVE` / `SKIP_MOVE` のような探索系skip commandを追加する場合は、`WAIT` と同じくSRS turnを進め、enemy_presence が false の場合は encounter roll の対象とする。

### combat state transition

combat phaseの状態遷移は次を基本とする。

```text
COMBAT_PLAYER_ACTION
  ATTACK <target>
    -> enemy destroyed: enemy removed
    -> remaining enemy exists: COMBAT_REACTION or enemy action sequence
    -> no enemy remains: EXPLORATION

  SKIP
    -> enemy action sequence
    -> no additional encounter roll while combat / enemy_presence is active

COMBAT_REACTION
  COUNTER
    -> if available: phaser counterattack
    -> if unavailable: DEFEND fallback
    -> next enemy attack or COMBAT_PLAYER_ACTION / EXPLORATION

  DEFEND
    -> reduced incoming damage
    -> next enemy attack or COMBAT_PLAYER_ACTION / EXPLORATION

GAME_OVER
  Q / QUIT
```

combat計算本体、enemy行動順、damage計算、resource消費、enemy撃破、SALVAGE rewardは #1199 または将来の `srs_combat.md` へ分離する。
この文書では integrated CLI の phase / command availability / state transition のみを正本化する。

## 初期状態

新規game開始時の minimal integrated SRS は、internal coordinate `(0,0)` から開始する。

```text
internal = Position(0, 0)
display  = (1, 1)
```

EXIT後に隣接sectorへ入る場合は、entry directionに対応する外周entry positionへ置く。

```text
entry from N -> internal (4,8)
entry from E -> internal (8,4)
entry from S -> internal (4,0)
entry from W -> internal (0,4)
```

## EXIT command

`EXIT <dir>` は、SRS local map上の現在cellから、指定方向の隣接LRS sectorへ移動するcommandである。

`EXIT <dir>` は、SRS内移動commandではない。
SRS外周の `warp_flags` と LRS側の移動可能性を両方確認する。

## EXIT成功条件

#1241で想定された成功条件を、#1268時点の実装状況とともに整理する。

| 条件 | 現行状態 | 実装箇所 / 備考 |
|---|---|---|
| commandが `EXIT <dir>` 形式である | implemented | parserが `EXIT` + 1方向tokenのみを `COMMAND_EXIT` にする。 |
| `<dir>` が `N/E/S/W` のいずれか | implemented | `_execute_exit_command()` で不正方向をrejectする。 |
| 現在SRS cellがmap内にある | implemented in SRS engine | `WARP_EXIT` 側が out-of-bounds をrejectする。 |
| 現在cellにmatching `warp_flags` がある | implemented | `srs_engine.apply_srs_command(... WARP_EXIT ...)` が確認する。 |
| SRS descriptor上のblocked edgeではない | implemented in SRS engine | `WARP_EXIT` 側が `descriptor.blocked_edges` をrejectする。 |
| LRS destinationがboard内にある | implemented | `_exit_destination()` 後に `lrs_engine.is_inside_board()` を確認する。 |
| known RIFT edgeではない | implemented | `known_routes[edge] == ROUTE_RIFT` をrejectする。 |
| accepted時にLRS positionを更新する | implemented | `_apply_lrs_exit_move()` がplayer_position, visited_cells, known_routes, turn_count, path, reveal, game_statusを更新する。 |
| accepted時に新しいSRS sectorへ入る | implemented minimal | `_create_minimal_srs_for_sector()` でminimal SRSを再作成する。 |
| combat中またはenemy presence中はexit不可 | partial | SRS engineの `WARP_EXIT` は `combat_state.enemy_presence` をrejectする。integrated minimal SRSは通常combat_stateなし。 |
| fuel / durability / resource制約 | deferred | integrated CLIのminimal prototypeではEXIT時のfuel消費・durability制約は扱わない。 |
| full generated SRS mapとの統合 | deferred | 現行integrated CLIはminimal all-FLOOR SRSを作る。full `create_sector()` 統合は後続。 |
| LRS board外縁方向をSRS generation時点でwarp禁止にする | deferred / #1264 | 現行minimal SRSは全edgeへwarp candidateを付与し、EXIT時にboard外をrejectする。 |

## EXIT rejected outcomes

現行integrated CLIでユーザーに返る主なrejectは次である。

```text
EXIT rejected: invalid direction
EXIT rejected: no <dir> warp point at SRS=(x,y)
EXIT rejected: <dir> would leave LRS map
EXIT rejected: <dir> edge is blocked by RIFT
```

SRS engine側にはより詳細な `WARP_EXIT_REJECTED` outcome が存在する。
ただし、現行 integrated CLI のsummaryでは、多くのSRS側rejectを `no <dir> warp point` に丸めている。
必要なら後続で、SRS event payloadに応じて次のような詳細summaryへ分ける。

```text
REJECTED_OUT_OF_BOUNDS
REJECTED_BLOCKED_EDGE
REJECTED_NO_WARP_FLAG
REJECTED_ENEMY_PRESENCE
```

## accepted時の処理順

現行 `_execute_exit_command()` の処理順は次の通りである。

```text
1. direction tokenを検証する
2. SRS `WARP_EXIT` commandを実行する
3. SRS側でrejectされた場合、integrated commandもrejectする
4. LRS destinationを計算する
5. destinationがboard外ならrejectする
6. known RIFT edgeならrejectする
7. LRS stateを移動更新する
8. destination sector symbolに応じてminimal SRSを作り直す
9. opposite directionをentry directionとして、新SRSのplayer entry positionを決める
10. RESULT summaryを返す
```

## deferred constraints

#1268 / #1279 / #1281時点で、次は意図的に deferred とする。

```text
- integrated CLI parserからstandalone `N/E/S/W` commandを廃止する実装
- HELP summaryからstandalone `N/E/S/W` を外し、`MOVE <route>` を案内する実装
- integrated CLIに `WAIT` / `ATTACK` / `SKIP` / `COUNTER` / `DEFEND` parserを追加する実装
- integrated CLIに `cli_phase` / `pending_enemy_attack` を追加する実装
- EXIT時のLRS fuel消費
- EXIT時のdurability / ship status制約
- full combat phaseとintegrated CLI command loopの実装統合
- full `srs/generate.py:create_sector()` との接続
- LRS board外縁情報をSRS generationへ渡す設計
- SRS engine側reject outcomeをintegrated CLI summaryへ詳細反映すること
```

これらは、minimal command-response loop と SRS/LRS接続が安定したあとに、個別issueで扱う。

## 関連spec

| Spec | 関係 |
|---|---|
| `srs_movement.md` | `MOVE_ROUTE`, movement points, terrain movement cost, interaction lifecycleのSRS側正本。 |
| `srs_warp.md` | `warp_flags` と RIFT_BARRIER / blocked edgeのSRS側正本。 |
| `srs_map_generation.md` | minimal SRS generation と full terrain-count profile deferred範囲。 |
