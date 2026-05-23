# Mike Mayfield STAR TREK 1972 HP BASIC ルール抽出メモ

Tracking issue: #470

## 目的

Mike Mayfield が 1972 年に HP 2000C / HP Time-Shared BASIC 向けに書き直した `STAR TREK` (`STTR1`) を、現行 TBX へ移植するためのルール抽出メモ。

このメモは、後続の TBX 実装が `Super Star Trek` 系の拡張へ流れないように、Mayfield 版のゲーム構造・データ構造・乱数分布・戦闘式・入出力を明文化するための作業台とする。

## 原典

### `docs/notes/sttr1.bas`

- `REM Extracted from HP tape image 16-Nov-2003 by Pete Turnbull`
- HP BASIC Program Library entry: `STTR1: STAR TREK`
- `36243 REV B -- 10/73`
- `STAR TREK: BY MIKE MAYFIELD, CENTERLINE ENGINEERING`
- `TOTAL INTERACTION GAME - ORIG. 20 OCT 1972`

このメモでは `docs/notes/sttr1.bas` を primary source とし、行番号を根拠として仕様を抽出する。

## Mayfield 版として実装対象に含めるもの

以前の推測では `library computer` を `Super Star Trek` 系拡張候補としていたが、原典 `STTR1` に command 7 として存在するため実装対象に含める。

- command 0: set course / warp engine control
- command 1: short range sensor scan
- command 2: long range sensor scan
- command 3: phaser control
- command 4: photon torpedo control
- command 5: shield control
- command 6: damage control report
- command 7: library computer
  - option 0: cumulative galactic record
  - option 1: status report
  - option 2: photon torpedo data

## Mayfield 版と派生版の境界

この issue では Mayfield `STTR1` を正とする。Mayfield `STTR1` に存在するものは、後年の派生版にも似た機能があっても実装対象に含める。

原典で確認できるまで入れないもの:

- 移動する Klingon
- quadrant 名や show character による拡張 status report
- Death Ray / shuttle / dilithium crystal など、派生版由来の緊急オプション
- `STTR1` にない追加 computer functions

## HP BASIC 読み替えメモ

- 行番号は mandatory。TBX では構造化された `DEF` / `WHILE` / `IF` へ分解する。
- `GOSUB` / `RETURN` は TBX の `DEF ... END` に対応させる。
- `GOTO A+1 OF ...` は command dispatch table として読む。
- HP BASIC の配列・文字列添字は 1-origin。TBX の配列も 1-origin なので、そのまま寄せやすい。
- HP BASIC の `RND(1)` は 0 以上 1 未満相当の浮動乱数として使われている。現行 TBX の `RND(n)` は整数乱数なので、Mayfield 版の確率分布を再現する helper が必要。
- HP BASIC の `SQR` は TBX の `SQRT` に対応させる。

### 座標 convention: row/column → [x, y]

Mayfield HP BASIC では `G[Q1, Q2]` の `Q1` が row（縦）、`Q2` が col（横）。
TBX 実装ではすべての座標を `[x, y]` convention（x = col 方向、y = row 方向）に正規化する。

| Mayfield HP BASIC | TBX 変数名 | 意味 |
| --- | --- | --- |
| `Q1` (quadrant row) | `ENT_QY` | Enterprise quadrant の y 座標 |
| `Q2` (quadrant col) | `ENT_QX` | Enterprise quadrant の x 座標 |
| `S1` (sector row) | `ENT_SY` | Enterprise sector の y 座標 |
| `S2` (sector col) | `ENT_SX` | Enterprise sector の x 座標 |
| `K[I,1]` (Klingon row) | `@K_Y[I]` | Klingon の y 座標 |
| `K[I,2]` (Klingon col) | `@K_X[I]` | Klingon の x 座標 |

配列アクセスも同様に正規化する: `G[Q1,Q2]` → `@GALAXY[QX, QY]`、`K[I,3]` は energy なので変更なし。

## データ構造

### HP BASIC 原典

```basic
260  DIM G[8,8],C[9,2],K[3,3],N[3],Z[8,8]
270  DIM C$[6],D$[72],E$[24],A$[3],Q$[72],R$[72],S$[48]
280  DIM Z$[72]
```

### 主な配列

| HP BASIC | 役割 | TBX 方針 |
| --- | --- | --- |
| `G[8,8]` | galaxy summary。各 quadrant を `K*100 + B*10 + S` で保持 | `DIM @GALAXY[8, 8]` |
| `Z[8,8]` | cumulative galactic record / computer memory | `DIM @CHART[8, 8]` |
| `C[9,2]` | course vector table | `DIM @COURSE_DX[9]`, `DIM @COURSE_DY[9]` |
| `K[3,3]` | 現 quadrant 内 Klingon: row, col, energy | `@K_X[3]`, `@K_Y[3]`, `@K_E[3]` |
| `N[3]` | long-range scan 1行ぶんの一時配列 | local scalar 3個 or `DIM @SCAN_ROW[3]` |
| `D[8]` | device damage state。負数が damaged | `DIM @DAMAGE[8]` |
| `Q$`, `R$`, `S$` | short-range sector display strings | 数値 `@SECTOR[8, 8]` + display helper へ置換 |

### TBX 配列方針

現行 TBX の 2D 配列機能（`DIM @A[w, h]`）を使い、8×8 の quadrant/sector マップを直接 2D で保持する。

```tbx
DIM @GALAXY[8, 8]
DIM @CHART[8, 8]
DIM @SECTOR[8, 8]
DIM @K_X[3]
DIM @K_Y[3]
DIM @K_E[3]
DIM @DAMAGE[8]
DIM @COURSE_DX[9]
DIM @COURSE_DY[9]
```

アクセスは `[x, y]` インデックスで直接行う。

```tbx
# quadrant (QX, QY) の galaxy summary を読む
@GALAXY[QX, QY]

# quadrant (QX, QY) の chart を更新する
LET @CHART[QX, QY] = @GALAXY[QX, QY]

# sector (SX, SY) のシンボルを読む
@SECTOR[SX, SY]

# sector (SX, SY) に Enterprise を置く
LET @SECTOR[SX, SY] = 1
```

### 座標変換 helper（historical / 不要）

> **Note**: 2D 配列が実装される以前の設計では、下記のような 1D index 変換 helper が必要だった。
> 現在の TBX では `@GALAXY[QX, QY]` と直接 2D アクセスできるため、これらは不要。
> 移植実装では使用しない。

```tbx
# 旧設計（参考のみ、使用不可）
# DEF QUAD_IDX(QR, QC)
#   RETURN (QR - 1) * 8 + QC
# END
#
# DEF SECTOR_IDX(SR, SC)
#   RETURN (SR - 1) * 8 + SC
# END
```

## 初期化

### Mission state

| 行 | 原典 | 意味 |
| --- | --- | --- |
| 290 | `T0=T=INT(RND(1)*20+20)*100` | 初期 stardate。2000, 2100, ..., 3900 のいずれか |
| 300 | `T9=30` | mission duration: 30 stardates |
| 320 | `E0=E=3000` | energy 初期値 / 最大値 |
| 330 | `P0=P=10` | photon torpedoes 初期値 / 最大値 |
| 340 | `S9=200` | Klingon 1隻の初期 shield/energy |
| 350 | `S=H8=0` | shield 0、torpedo data helper flag 0 |
| 370-400 | `Q1,Q2,S1,S2=INT(RND(1)*8+1)` | Enterprise 初期 quadrant / sector |
| 410 | `T7=TIM(0)+60*TIM(1)` | 実時間 timer |

### Course vector table

行 420-440 で `C[1..9,1..2]` を初期化する。

| course | row delta | col delta | 方角 |
| --- | ---: | ---: | --- |
| 1 | 0 | 1 | east |
| 2 | -1 | 1 | north-east |
| 3 | -1 | 0 | north |
| 4 | -1 | -1 | north-west |
| 5 | 0 | -1 | west |
| 6 | 1 | -1 | south-west |
| 7 | 1 | 0 | south |
| 8 | 1 | 1 | south-east |
| 9 | 0 | 1 | course interpolation sentinel |

Course は 1 以上 9 未満。real value が許可され、`C2=INT(C1)` のあと `C[C2]` と `C[C2+1]` の線形補間で vector を作る。

## Galaxy generation

行 490-770 で、Klingon 数・starbase 数・star 数を生成する。

### Klingon distribution per quadrant

```basic
520  R1=RND(1)
530  IF R1>.98 THEN 580
540  IF R1>.95 THEN 610
550  IF R1>.8 THEN 640
560  K3=0
580  K3=3
610  K3=2
640  K3=1
```

| 条件 | Klingons | 確率 |
| --- | ---: | ---: |
| `R1 > .98` | 3 | 2% |
| `.95 < R1 <= .98` | 2 | 3% |
| `.8 < R1 <= .95` | 1 | 15% |
| otherwise | 0 | 80% |

`K9` は total Klingons。各 quadrant の Klingon 数を加算する。

### Starbase distribution per quadrant

```basic
660  R1=RND(1)
670  IF R1>.96 THEN 700
680  B3=0
700  B3=1
```

| 条件 | Starbases | 確率 |
| --- | ---: | ---: |
| `R1 > .96` | 1 | 4% |
| otherwise | 0 | 96% |

`B9` は total starbases。

### Star distribution per quadrant

```basic
720  S3=INT(RND(1)*8+1)
```

Stars は 1..8 の一様分布。

### Quadrant encoding

```basic
730  G[I,J]=K3*100+B3*10+S3
```

- hundreds digit: Klingons
- tens digit: starbases
- units digit: stars

行 775 で `B9 <= 0 OR K9 <= 0` の場合は galaxy generation をやり直す。

## Quadrant setup

行 810-1260 で、現在 quadrant の summary を読み出し、sector map を生成する。

### Summary decode

```basic
830  X=G[Q1,Q2]*.01
840  K3=INT(X)
850  B3=INT((X-K3)*10)
860  S3=G[Q1,Q2]-INT(G[Q1,Q2]*.1)*10
```

### Sector symbols

| Symbol | 意味 | 行 |
| --- | --- | --- |
| `<*>` | Enterprise | 980-1010 |
| `+++` | Klingon | 1020-1110 |
| `>!<` | Starbase | 1120-1180 |
| ` * ` | Star | 1190-1250 |
| `   ` | Empty | multiple |

### Random empty sector placement

行 5380-5450 の helper が empty sector を探す。

```basic
5380  R1=INT(RND(1)*8+1)
5390  R2=INT(RND(1)*8+1)
5400  A$="   "
5410  Z1=R1
5420  Z2=R2
5430  GOSUB 5680
5440  IF Z3=0 THEN 5380
5450  RETURN
```

`A$="   "` と一致する sector を探す。`Z3=1` が match。

## Command loop

行 1260-1400 が command loop。

```basic
1260  GOSUB 4120
1270  PRINT "COMMAND:";
1280  INPUT A
1290  GOTO A+1 OF 1410,1260,2330,2530,2800,3460,3560,4630
```

| command | 行 | 機能 | TBX 移植先 |
| ---: | ---: | --- | --- |
| 0 | 1410 | set course / warp | `CMD_NAV` |
| 1 | 1260 | short range sensor scan | `PRINT_SRS` |
| 2 | 2330 | long range sensor scan | `PRINT_LRS` |
| 3 | 2530 | phaser control | `CMD_PHA` |
| 4 | 2800 | photon torpedo control | `CMD_TOR` |
| 5 | 3460 | shield control | `CMD_SHE` |
| 6 | 3560 | damage control report | `PRINT_DAMAGE` |
| 7 | 4630 | library computer | `CMD_COM` |

Invalid command falls through to help text at 1300-1400 and reprompts.

## Navigation / warp

行 1410-2320。

### Inputs and validation

- Course prompt: `COURSE (1-9):`
- `C1=0` cancels to command loop
- valid range: `1 <= C1 < 9`
- Warp prompt: `WARP FACTOR (0-8):`
- valid range: `0 <= W1 <= 8`
- If warp engines damaged (`D[1] < 0`), maximum warp is `.2`

### Pre-move attack and energy checks

- If Klingons are present, they attack before movement: `GOSUB 3790`.
- If energy is depleted, player may move energy from shields before proceeding.

### Damage repair / random event per move

行 1610-1640: all damaged devices (`D[I] < 0`) repair by `+1` per move.

行 1650-1800: 20% chance of random damage event.

- Pick device `R1=INT(RND(1)*8+1)`.
- 50% chance damage worsens: `D[R1]=D[R1]-(RND(1)*5+1)`.
- 50% chance repair improves: `D[R1]=D[R1]+(RND(1)*5+1)`.

### Movement vector

```basic
1810  N=INT(W1*8)
1885  C2=INT(C1)
1890  X1=C[C2,1]+(C[C2+1,1]-C[C2,1])*(C1-C2)
1900  X2=C[C2,2]+(C[C2+1,2]-C[C2,2])*(C1-C2)
```

- `N` is number of sector steps.
- `X1`, `X2` are interpolated row/col deltas.

### Sector collision

行 1910-2070:

- Each sector step updates `S1`, `S2` by `X1`, `X2`.
- If outside quadrant, transfer to new quadrant.
- Otherwise checks whether target sector is empty.
- If blocked, warp engines shut down at the current sector due to bad navigation and Enterprise backs up one step.

### Energy / time cost

```basic
2120  E=E-N+5
2130  IF W1<1 THEN 2150
2140  T=T+1
```

Same-quadrant and cross-quadrant movement both use `E=E-N+5`; warp factor >= 1 advances stardate by 1.

If `T > T0 + T9`, lose by timeout.

## Long-range sensor scan

行 2330-2500。

- Disabled if `D[3] < 0`.
- Prints 3 x 3 surrounding quadrants centered on Mayfield `(Q1,Q2)`. TBX uses `[x, y]`, so the center is `(ENT_QX, ENT_QY)` where `Q1 -> ENT_QY` and `Q2 -> ENT_QX`.
- Out-of-bounds entries remain 0.
- If computer is operational (`D[7] >= 0`), updates `Z[I,J]=G[I,J]` for scanned quadrants. TBX: `LET @CHART[IX, IY] = @GALAXY[IX, IY]`（`I`=row なので `IY = I`、`J`=col なので `IX = J`）。

## Phaser control

行 2530-2790。

- Requires Klingons in current quadrant; otherwise jumps to no-Klingons message at 3670.
- Disabled if `D[4] < 0`.
- If computer is damaged (`D[7] < 0`), accuracy is hampered.
- Player inputs energy `X`; requires `E-X >= 0`.
- Energy is deducted immediately: `E=E-X`.
- Klingons attack before phaser damage: `GOSUB 3790`.
- If computer is damaged, actual phaser energy becomes `X=X*RND(1)`.

Damage per live Klingon:

```basic
2700  H=(X/K3/FND(0))*(2*RND(1))
2710  K[I,3]=K[I,3]-H
```

where:

```basic
360 DEF FND(D)=SQR((K[I,1]-S1)^2+(K[I,2]-S2)^2)
```

TBX: `SQRT((@K_Y[I] - ENT_SY)^2 + (@K_X[I] - ENT_SX)^2)`（Mayfield `K[I,1]`=row→`@K_Y`, `K[I,2]`=col→`@K_X`、`S1`=sector row→`ENT_SY`、`S2`=sector col→`ENT_SX`）。

Notes:

- `K3` is current number of Klingons in quadrant.
- Damage is inversely proportional to distance.
- Random multiplier is `2*RND(1)`.
- Destroyed Klingons call line 3690 helper, decrementing `K3` and `K9` and updating `G[Q1,Q2]`. TBX: `LET @GALAXY[ENT_QX, ENT_QY] = ...`。

## Photon torpedo control

行 2800-3450。

- Disabled if `D[5] < 0`.
- Requires `P > 0` torpedoes.
- Course prompt uses same course system as warp.
- Each shot decrements `P`.
- Torpedo track advances by interpolated course vector until out of quadrant or collision.

Collision behavior:

| Target | 行 | Effect |
| --- | ---: | --- |
| `+++` Klingon | 3070-3210 | destroy Klingon, decrement `K3` and `K9`, set corresponding `K[I,3]=0` |
| ` * ` Star | 3220-3280 | print `YOU CAN'T DESTROY STARS SILLY`, torpedo ends/misses |
| `>!<` Starbase | 3290-3350 | destroy starbase, decrement `B3`, print congratulatory message |
| Empty / no hit | 2960-3060, 3420 | continue track or print `TORPEDO MISSED` |

After torpedo resolution, surviving Klingons attack via `GOSUB 3790`.

## Shield control

行 3460-3550。

- The code checks `D[7] >= 0`, but prints `SHIELD CONTROL IS NON-OPERATIONAL` when negative. Device naming suggests shield control is device 7 in the original damage index.
- Prompt: `ENERGY AVAILABLE = E+S   NUMBER OF UNITS TO SHIELDS:`
- `X <= 0` cancels.
- Requires `E+S-X >= 0`.
- Assignment: `E=E+S-X`, `S=X`.

## Damage control report

行 3560-3660。

- Disabled if `D[6] < 0`.
- Prints device names from `D$` / `E$` via helper 5610.
- Device names:
  1. Warp engines
  2. S.R. sensors
  3. L.R. sensors
  4. Phaser control
  5. Photon tubes
  6. Damage control
  7. Shield control
  8. Computer

## Klingon attack

行 3790-3910。

- If docked, starbase shields protect Enterprise and no damage occurs.
- Each live Klingon attacks.
- Hit formula:

```basic
3850  H=(K[I,3]/FND(0))*(2*RND(1))
3860  S=S-H
```

- Damage reduces shields `S`.
- If `S < 0`, Enterprise destroyed.

## Docking and short-range sensor scan

行 4120-4530。

### Docked condition

Checks all sectors adjacent to Enterprise, including diagonals, for starbase symbol `>!<`.

If adjacent to starbase:

```basic
4240  D0=1
4250  C$="DOCKED"
4260  E=3000
4270  P=10
4280  PRINT "SHIELDS DROPPED FOR DOCKING PURPOSES"
4290  S=0
```

Docking restores energy and torpedoes, drops shields to zero.

If not docked:

- `GREEN` if no Klingons and energy is at least 10% of max.
- `RED` if Klingons are present.
- `YELLOW` if no Klingons and low energy.

### Short-range scan

- If `D[2] < 0`, short-range sensors are out.
- Otherwise prints 8 x 8 sector display plus status fields:
  - stardate
  - condition
  - quadrant
  - sector
  - energy
  - photon torpedoes
  - shields

## Library computer

行 4630-5320。

- Disabled if `D[8] < 0`.
- Prompt: `COMPUTER ACTIVE AND AWAITING COMMAND`.

Options:

| option | 行 | 機能 |
| ---: | ---: | --- |
| 0 | 4740-4820 | cumulative galactic record from `Z[8,8]` |
| 1 | 4830-4870 | status report, then damage report |
| 2 | 4880-5320 | photon torpedo trajectory/distance helper |

Option 2 computes direction and distance from Enterprise to each live Klingon, or can be used as a calculator if no target remains / player asks.

## Endgame

### Timeout / defeat

- Timeout: line 3970 prints current stardate, then defeat summary.
- Enterprise destroyed: line 4000 prints destruction message.
- Defeat summary: line 4020 prints remaining Klingon count, then restarts program at line 230.
- Dead in space: line 3920 says Enterprise is dead in space; if Klingons remain, repeated attacks occur.

### Victory

Line 4040 onward:

- Prints the last Klingon destroyed message.
- Prints Federation saved message.
- Efficiency rating:

```basic
4080  PRINT "YOUR EFFICIENCY RATING ="((K7/(T-T0))*1000)
```

- Then prints actual mission time and restarts program.

## Line range map

| HP BASIC line range | 役割 | TBX 移植先 | 状態 |
| --- | --- | --- | --- |
| 1-160 | Header | note only | 確認済み |
| 170-230 | Startup / instructions prompt | `MAIN` | 確認済み |
| 240-490 | Global initialization | `INIT_GAME` | 確認済み |
| 500-770 | Galaxy generation | `INIT_GALAXY` | 確認済み |
| 780 | Mission briefing | `PRINT_MISSION_BRIEFING` | 確認済み |
| 810-1260 | Current quadrant setup | `INIT_QUADRANT` | 確認済み |
| 1260-1400 | Command loop / help | `GAME_LOOP` / `PRINT_COMMANDS` | 確認済み |
| 1410-2320 | Navigation | `CMD_NAV` | 確認済み |
| 2330-2500 | Long-range scan | `PRINT_LRS` | 確認済み |
| 2530-2790 | Phaser attack | `CMD_PHA` | 確認済み |
| 2800-3450 | Photon torpedo | `CMD_TOR` | 確認済み |
| 3460-3550 | Shield control | `CMD_SHE` | 確認済み |
| 3560-3660 | Damage report | `PRINT_DAMAGE` | 確認済み |
| 3670-3780 | Klingon destroyed helper / no-target message | `DESTROY_KLINGON` | 確認済み |
| 3790-3910 | Klingon attack | `KLINGON_ATTACK` | 確認済み |
| 3920-4110 | Endgame | `CHECK_ENDGAME` / `PRINT_VICTORY` / `PRINT_DEFEAT` | 確認済み |
| 4120-4530 | Docking + short-range scan | `PRINT_SRS` / `CHECK_DOCKED` | 確認済み |
| 4630-5320 | Library computer | `CMD_COM` | 確認済み |
| 5380-5450 | Random empty sector | `RANDOM_EMPTY_SECTOR` | 確認済み |
| 5460-5500 | Clear screen spacer | `CLEAR_SCREEN` | 確認済み |
| 5510-5600 | Insert symbol into quadrant strings | `SET_SECTOR_SYMBOL` | 置換対象 |
| 5610-5670 | Device name printer | `PRINT_DEVICE_NAME` | 確認済み |
| 5680-5810 | Sector symbol comparison | `SECTOR_HAS` | 置換対象 |
| 5820-6410 | Instructions | `PRINT_INSTRUCTIONS` | 確認済み |

## Variable map

| HP BASIC variable | 意味 | TBX name | 状態 |
| --- | --- | --- | --- |
| `Q1` (row) | Enterprise quadrant の y 座標 | `ENT_QY` | 確認済み |
| `Q2` (col) | Enterprise quadrant の x 座標 | `ENT_QX` | 確認済み |
| `S1` (row) | Enterprise sector の y 座標 | `ENT_SY` | 確認済み |
| `S2` (col) | Enterprise sector の x 座標 | `ENT_SX` | 確認済み |
| `T0` | Initial stardate | `START_STARDATE` | 確認済み |
| `T` | Current stardate | `STARDATE` | 確認済み |
| `T9` | Mission duration | `MISSION_DAYS` | 確認済み |
| `E0` | Max/initial energy | `MAX_ENERGY` | 確認済み |
| `E` | Current energy | `ENERGY` | 確認済み |
| `P0` | Max/initial torpedoes | `MAX_TORPEDOES` | 確認済み |
| `P` | Current torpedoes | `TORPEDOES` | 確認済み |
| `S` | Shields | `SHIELDS` | 確認済み |
| `S9` | Klingon initial energy | `KLINGON_INIT_ENERGY` | 確認済み |
| `K9` | Total Klingons remaining | `KLINGONS_LEFT` | 確認済み |
| `K7` | Initial Klingon count | `KLINGONS_INITIAL` | 確認済み |
| `B9` | Total starbases remaining | `BASES_LEFT` | 確認済み |
| `K3` | Klingons in current quadrant | `KLINGONS_HERE` | 確認済み |
| `B3` | Starbases in current quadrant | `BASES_HERE` | 確認済み |
| `S3` | Stars in current quadrant | `STARS_HERE` | 確認済み |
| `C$` | Condition string | `CONDITION` | 確認済み |
| `D0` | Docked flag | `DOCKED` | 確認済み |
| `D[1..8]` | Device damage states | `@DAMAGE[1..8]` | 確認済み |

## TBX design decisions

### `RND(1)` compatibility

原典は floating `RND(1)` を多用する。現行 TBX の `RND(n)` は整数 `[1,n]` なので、Mayfield 版を自然に移植するには以下のどちらかが必要。

1. `RND_FLOAT()` primitive を追加する
2. 高解像度整数乱数から `0 <= x < 1` を作る helper を用意する

Gameplay formulas が `RND(1)>.98` や `2*RND(1)` に依存するため、単純に `RND(100)` へ置き換える場合も helper 名を分けて意図を残す。

### Sector representation

原典は `Q$`, `R$`, `S$` という 3 本の固定長文字列で 8 x 8 sector display を保持する。TBX ではロジック用に `@SECTOR[8, 8]` を使い、表示時に symbol へ変換するのがよい。

```text
0 = empty
1 = Enterprise
2 = Klingon
3 = Starbase
4 = Star
```

### Library computer

`CMD_COM` は Mayfield `STTR1` の正規機能なので実装対象。とくに option 0 の cumulative galactic record は `@CHART[8, 8]` と連動する。

## 実装前チェックリスト

- [x] 原典 `STTR1` を repository に保存した
- [x] このメモに原典行番号との対応が入っている
- [x] 主要 gameplay formula に原典由来の line reference がある
- [ ] `RND(1)` compatibility 方針を決める
- [ ] `Q$`/`R$`/`S$` string-map を `@SECTOR[8, 8]` に置き換える詳細設計を決める
- [ ] 表示幅・空白をどこまで原典に合わせるか決める
- [ ] `docs/notes/sttr1.bas` の扱い（原典保存として残す / ライセンス・出典注記を追加する）を確認する
