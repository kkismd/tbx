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

### ミッション状態

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

### コースベクトルテーブル

行 420-440 で `C[1..9,1..2]` を初期化する。

| course | row delta | col delta | 方角 |
| --- | ---: | ---: | --- |
| 1 | 0 | 1 | 東 |
| 2 | -1 | 1 | 北東 |
| 3 | -1 | 0 | 北 |
| 4 | -1 | -1 | 北西 |
| 5 | 0 | -1 | 西 |
| 6 | 1 | -1 | 南西 |
| 7 | 1 | 0 | 南 |
| 8 | 1 | 1 | 南東 |
| 9 | 0 | 1 | コース補間センチネル |

コースは 1 以上 9 未満。実数値が許可され、`C2=INT(C1)` のあと `C[C2]` と `C[C2+1]` の線形補間で vector を作る。

## ギャラクシー生成

行 490-770 で、Klingon 数・starbase 数・star 数を生成する。

### クアドラントごとのクリンゴン分布

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
| それ以外 | 0 | 80% |

`K9` は total Klingons。各 quadrant の Klingon 数を加算する。

### クアドラントごとのスターベース分布

```basic
660  R1=RND(1)
670  IF R1>.96 THEN 700
680  B3=0
700  B3=1
```

| 条件 | Starbases | 確率 |
| --- | ---: | ---: |
| `R1 > .96` | 1 | 4% |
| それ以外 | 0 | 96% |

`B9` は total starbases。

### クアドラントごとの星分布

```basic
720  S3=INT(RND(1)*8+1)
```

Stars は 1..8 の一様分布。

### クアドラントエンコーディング

```basic
730  G[I,J]=K3*100+B3*10+S3
```

- 百の位: クリンゴン数
- 十の位: スターベース数
- 一の位: 星数

行 775 で `B9 <= 0 OR K9 <= 0` の場合は galaxy generation をやり直す。

## クアドラントセットアップ

行 810-1260 で、現在 quadrant の summary を読み出し、sector map を生成する。

### サマリーのデコード

```basic
830  X=G[Q1,Q2]*.01
840  K3=INT(X)
850  B3=INT((X-K3)*10)
860  S3=G[Q1,Q2]-INT(G[Q1,Q2]*.1)*10
```

### セクターシンボル

| Symbol | 意味 | 行 |
| --- | --- | --- |
| `<*>` | Enterprise | 980-1010 |
| `+++` | Klingon | 1020-1110 |
| `>!<` | Starbase | 1120-1180 |
| ` * ` | Star | 1190-1250 |
| `   ` | Empty | multiple |

### ランダム空セクターの配置

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

## コマンドループ

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

コマンドが無効な場合は 1300-1400 のヘルプテキストに流れ、再プロンプトされる。

## 航法 / ワープ

行 1410-2320。

### 入力とバリデーション

- コースプロンプト: `COURSE (1-9):`
- `C1=0` でコマンドループに戻る
- 有効範囲: `1 <= C1 < 9`
- ワーププロンプト: `WARP FACTOR (0-8):`
- 有効範囲: `0 <= W1 <= 8`
- ワープエンジンが損傷している場合（`D[1] < 0`）、最大ワープは `.2`

### 移動前の攻撃とエネルギーチェック

- クリンゴンがいる場合、移動前に攻撃してくる: `GOSUB 3790`。
- エネルギーが枯渇している場合、プレイヤーはシールドからエネルギーを移してから続行できる。

### 移動ごとのダメージ修理 / ランダムイベント

行 1610-1640: 損傷しているデバイス（`D[I] < 0`）はすべて移動ごとに `+1` 修理される。

行 1650-1800: 20% の確率でランダムな損傷イベントが発生する。

- デバイスを選択: `R1=INT(RND(1)*8+1)`。
- 50% の確率でダメージが悪化: `D[R1]=D[R1]-(RND(1)*5+1)`。
- 50% の確率で修理が進む: `D[R1]=D[R1]+(RND(1)*5+1)`。

### 移動ベクトル

```basic
1810  N=INT(W1*8)
1885  C2=INT(C1)
1890  X1=C[C2,1]+(C[C2+1,1]-C[C2,1])*(C1-C2)
1900  X2=C[C2,2]+(C[C2+1,2]-C[C2,2])*(C1-C2)
```

- `N` はセクターのステップ数。
- `X1`、`X2` は補間された row/col デルタ。

### セクター衝突

行 1910-2070:

- 各セクターステップで `S1`、`S2` が `X1`、`X2` ずつ更新される。
- クアドラント外に出た場合は新しいクアドラントへ移動する。
- そうでなければ、目標セクターが空かどうかを確認する。
- 障害物がある場合、ナビゲーションミスでワープエンジンが停止し、Enterprise は 1 ステップ戻る。

### エネルギー / 時間コスト

```basic
2120  E=E-N+5
2130  IF W1<1 THEN 2150
2140  T=T+1
```

同一クアドラント内・クアドラント間の移動ともに `E=E-N+5` を使用する。ワープファクターが 1 以上の場合、スターデートが 1 進む。

`T > T0 + T9` になると、タイムアウトで敗北する。

## 長距離センサースキャン

行 2330-2500。

- `D[3] < 0` の場合は無効化。
- Mayfield `(Q1,Q2)` を中心とした 3×3 の周辺クアドラントを表示する。TBX では `[x, y]` を使うので中心は `(ENT_QX, ENT_QY)` となる（`Q1 -> ENT_QY`、`Q2 -> ENT_QX`）。
- 範囲外のエントリは 0 のまま。
- If computer is operational (`D[7] >= 0`), updates `Z[I,J]=G[I,J]` for scanned quadrants. TBX: `LET @CHART[IX, IY] = @GALAXY[IX, IY]`（`I`=row なので `IY = I`、`J`=col なので `IX = J`）。

## フェイザーコントロール

行 2530-2790。

- 現在のクアドラントにクリンゴンが必要。いない場合は 3670 のメッセージへジャンプする。
- `D[4] < 0` の場合は無効化。
- コンピューターが損傷（`D[7] < 0`）している場合、命中精度が低下する。
- プレイヤーがエネルギー `X` を入力する。`E-X >= 0` が必要。
- エネルギーはすぐに差し引かれる: `E=E-X`。
- フェイザーダメージの前にクリンゴンが攻撃する: `GOSUB 3790`。
- コンピューターが損傷している場合、実際のフェイザーエネルギーは `X=X*RND(1)` になる。

生存クリンゴン 1 体あたりのダメージ:

```basic
2700  H=(X/K3/FND(0))*(2*RND(1))
2710  K[I,3]=K[I,3]-H
```

ここで:

```basic
360 DEF FND(D)=SQR((K[I,1]-S1)^2+(K[I,2]-S2)^2)
```

TBX: `SQRT((@K_Y[I] - ENT_SY)^2 + (@K_X[I] - ENT_SX)^2)`（Mayfield `K[I,1]`=row→`@K_Y`, `K[I,2]`=col→`@K_X`、`S1`=sector row→`ENT_SY`、`S2`=sector col→`ENT_SX`）。

注:

- `K3` は現在クアドラント内のクリンゴン数。
- ダメージは距離に反比例する。
- ランダム乗数は `2*RND(1)`。
- 撃破されたクリンゴンは行 3690 のヘルパーを呼び出し、`K3` と `K9` を減算し `G[Q1,Q2]` を更新する。TBX: `LET @GALAXY[ENT_QX, ENT_QY] = ...`。

## 光子魚雷コントロール

行 2800-3450。

- `D[5] < 0` の場合は無効化。
- `P > 0` の魚雷が必要。
- コースプロンプトはワープと同じコースシステムを使用する。
- 発射のたびに `P` が 1 減る。
- 魚雷は補間されたコースベクトルに沿って、クアドラント外または衝突するまで進む。

衝突時の挙動:

| 対象 | 行 | 効果 |
| --- | ---: | --- |
| `+++` Klingon | 3070-3210 | クリンゴンを撃破、`K3` と `K9` を減算、対応する `K[I,3]=0` に設定 |
| ` * ` Star | 3220-3280 | `YOU CAN'T DESTROY STARS SILLY` を表示、魚雷終了/外れ |
| `>!<` Starbase | 3290-3350 | スターベースを破壊、`B3` を減算、メッセージを表示 |
| Empty / no hit | 2960-3060, 3420 | 軌跡を継続、または `TORPEDO MISSED` を表示 |

魚雷の解決後、生存クリンゴンが `GOSUB 3790` で攻撃する。

## シールドコントロール

行 3460-3550。

- コードは `D[7] >= 0` をチェックし、負の場合は `SHIELD CONTROL IS NON-OPERATIONAL` を表示する。デバイス命名から、シールドコントロールは元のダメージインデックスのデバイス 7 であることがわかる。
- プロンプト: `ENERGY AVAILABLE = E+S   NUMBER OF UNITS TO SHIELDS:`
- `X <= 0` でキャンセル。
- `E+S-X >= 0` が必要。
- 代入: `E=E+S-X`、`S=X`。

## ダメージコントロールレポート

行 3560-3660。

- `D[6] < 0` の場合は無効化。
- ヘルパー 5610 を通じて `D$` / `E$` からデバイス名を表示する。
- デバイス名:
  1. Warp engines
  2. S.R. sensors
  3. L.R. sensors
  4. Phaser control
  5. Photon tubes
  6. Damage control
  7. Shield control
  8. Computer

## クリンゴン攻撃

行 3790-3910。

- ドッキング中はスターベースのシールドが Enterprise を守り、ダメージは発生しない。
- 生存しているクリンゴンがそれぞれ攻撃する。
- ヒット計算式:

```basic
3850  H=(K[I,3]/FND(0))*(2*RND(1))
3860  S=S-H
```

- ダメージはシールド `S` を減らす。
- `S < 0` になると Enterprise は破壊される。

## ドッキングと短距離センサースキャン

行 4120-4530。

### ドック状態

Enterprise に隣接するすべてのセクター（斜め含む）にスターベースシンボル `>!<` があるか確認する。

スターベースに隣接している場合:

```basic
4240  D0=1
4250  C$="DOCKED"
4260  E=3000
4270  P=10
4280  PRINT "SHIELDS DROPPED FOR DOCKING PURPOSES"
4290  S=0
```

ドッキングするとエネルギーと魚雷が回復し、シールドがゼロになる。

ドッキングしていない場合:

- `GREEN`: クリンゴンがおらず、エネルギーが最大の 10% 以上。
- `RED`: クリンゴンが存在する。
- `YELLOW`: クリンゴンがおらず、エネルギーが低い。

### 短距離センサースキャン

- `D[2] < 0` の場合、短距離センサーは機能しない。
- それ以外の場合、8×8 のセクター表示とステータスフィールドを表示する:
  - stardate
  - condition
  - quadrant
  - sector
  - energy
  - photon torpedoes
  - shields

## ライブラリコンピューター

行 4630-5320。

- Disabled if `D[8] < 0`.
- Prompt: `COMPUTER ACTIVE AND AWAITING COMMAND`.

オプション:

| option | 行 | 機能 |
| ---: | ---: | --- |
| 0 | 4740-4820 | cumulative galactic record from `Z[8,8]` |
| 1 | 4830-4870 | status report, then damage report |
| 2 | 4880-5320 | photon torpedo trajectory/distance helper |

オプション 2 は Enterprise から各生存クリンゴンへの方向と距離を計算する。ターゲットが残っていない場合やプレイヤーが要求した場合は計算機として使用できる。

## エンドゲーム

### タイムアウト / 敗北

- タイムアウト: 行 3970 で現在のスターデートを表示し、敗北サマリーを出力する。
- Enterprise 撃破: 行 4000 で破壊メッセージを表示する。
- 敗北サマリー: 行 4020 で残りクリンゴン数を表示し、行 230 からプログラムを再起動する。
- 宇宙で動けなくなった場合: 行 3920 で Enterprise が動けないと告げる。クリンゴンが残っていると繰り返し攻撃される。

### 勝利

行 4040 以降:

- 最後のクリンゴン撃破メッセージを表示する。
- 連邦救済メッセージを表示する。
- 効率レーティング:

```basic
4080  PRINT "YOUR EFFICIENCY RATING ="((K7/(T-T0))*1000)
```

- 実際のミッション時間を表示し、プログラムを再起動する。

## ライン範囲マップ

| HP BASIC line range | 役割 | TBX 移植先 | 状態 |
| --- | --- | --- | --- |
| 1-160 | ヘッダー | メモのみ | 確認済み |
| 170-230 | 起動 / 説明プロンプト | `MAIN` | 確認済み |
| 240-490 | グローバル初期化 | `INIT_GAME` | 確認済み |
| 500-770 | ギャラクシー生成 | `INIT_GALAXY` | 確認済み |
| 780 | ミッションブリーフィング | `PRINT_MISSION_BRIEFING` | 確認済み |
| 810-1260 | 現在クアドラントのセットアップ | `INIT_QUADRANT` | 確認済み |
| 1260-1400 | コマンドループ / ヘルプ | `GAME_LOOP` / `PRINT_COMMANDS` | 確認済み |
| 1410-2320 | 航法 | `CMD_NAV` | 確認済み |
| 2330-2500 | 長距離スキャン | `PRINT_LRS` | 確認済み |
| 2530-2790 | フェイザー攻撃 | `CMD_PHA` | 確認済み |
| 2800-3450 | 光子魚雷 | `CMD_TOR` | 確認済み |
| 3460-3550 | シールドコントロール | `CMD_SHE` | 確認済み |
| 3560-3660 | ダメージレポート | `PRINT_DAMAGE` | 確認済み |
| 3670-3780 | クリンゴン撃破ヘルパー / ターゲットなしメッセージ | `DESTROY_KLINGON` | 確認済み |
| 3790-3910 | クリンゴン攻撃 | `KLINGON_ATTACK` | 確認済み |
| 3920-4110 | エンドゲーム | `CHECK_ENDGAME` / `PRINT_VICTORY` / `PRINT_DEFEAT` | 確認済み |
| 4120-4530 | ドッキング + 短距離スキャン | `PRINT_SRS` / `CHECK_DOCKED` | 確認済み |
| 4630-5320 | ライブラリコンピューター | `CMD_COM` | 確認済み |
| 5380-5450 | ランダム空セクター | `RANDOM_EMPTY_SECTOR` | 確認済み |
| 5460-5500 | 画面クリアスペーサー | `CLEAR_SCREEN` | 確認済み |
| 5510-5600 | クアドラント文字列へのシンボル挿入 | `SET_SECTOR_SYMBOL` | 置換対象 |
| 5610-5670 | デバイス名表示 | `PRINT_DEVICE_NAME` | 確認済み |
| 5680-5810 | セクターシンボル比較 | `SECTOR_HAS` | 置換対象 |
| 5820-6410 | 説明 | `PRINT_INSTRUCTIONS` | 確認済み |

## 変数マップ

| HP BASIC variable | 意味 | TBX name | 状態 |
| --- | --- | --- | --- |
| `Q1` (row) | Enterprise quadrant の y 座標 | `ENT_QY` | 確認済み |
| `Q2` (col) | Enterprise quadrant の x 座標 | `ENT_QX` | 確認済み |
| `S1` (row) | Enterprise sector の y 座標 | `ENT_SY` | 確認済み |
| `S2` (col) | Enterprise sector の x 座標 | `ENT_SX` | 確認済み |
| `T0` | 初期スターデート | `START_STARDATE` | 確認済み |
| `T` | 現在のスターデート | `STARDATE` | 確認済み |
| `T9` | ミッション期間 | `MISSION_DAYS` | 確認済み |
| `E0` | 最大/初期エネルギー | `MAX_ENERGY` | 確認済み |
| `E` | 現在のエネルギー | `ENERGY` | 確認済み |
| `P0` | 最大/初期魚雷数 | `MAX_TORPEDOES` | 確認済み |
| `P` | 現在の魚雷数 | `TORPEDOES` | 確認済み |
| `S` | シールド | `SHIELDS` | 確認済み |
| `S9` | クリンゴン初期エネルギー | `KLINGON_INIT_ENERGY` | 確認済み |
| `K9` | 残りクリンゴン総数 | `KLINGONS_LEFT` | 確認済み |
| `K7` | クリンゴン初期数 | `KLINGONS_INITIAL` | 確認済み |
| `B9` | 残りスターベース総数 | `BASES_LEFT` | 確認済み |
| `K3` | 現在クアドラント内のクリンゴン数 | `KLINGONS_HERE` | 確認済み |
| `B3` | 現在クアドラント内のスターベース数 | `BASES_HERE` | 確認済み |
| `S3` | 現在クアドラント内の星数 | `STARS_HERE` | 確認済み |
| `C$` | コンディション文字列 | `CONDITION` | 確認済み |
| `D0` | ドックフラグ | `DOCKED` | 確認済み |
| `D[1..8]` | デバイス損傷状態 | `@DAMAGE[1..8]` | 確認済み |

## TBX 設計方針

### `RND(1)` 互換性

Mayfield HP BASIC の `RND(1)` は `0 <= r < 1` の浮動小数乱数として使われている。TBX の `RND(n)` は `1..n` の整数乱数なので、STTR1 移植では次の方針で読み替える。

| Mayfield pattern | TBX policy |
| --- | --- |
| `INT(RND(1)*N + A)` | `RND(N) + A - 1` |
| `RND(1) > p` | `RND(100)` による percent check |
| `R1=RND(1)` 後に複数 `IF` | TBX でも乱数を1回だけ引き、同じ値で区間分岐する |
| `2*RND(1)` | `RND(200)-1` を scale 100 の `0..199` 係数として扱う |

Examples:

```tbx
# INT(RND(1)*20+20)*100
(RND(20) + 19) * 100

# R1=RND(1); IF R1>.98; IF R1>.95; IF R1>.8
VAR R1 = RND(100)
IF R1 > 98
  # line 580 相当
ELSIF R1 > 95
  # line 610 相当
ELSIF R1 > 80
  # line 640 相当
ELSE
  # fallthrough
ENDIF
```


`2*RND(1)` は phaser / Klingon attack の damage multiplier として使われる。整数除算の早すぎる丸めを避けるため、可能な限り先に乱数係数を掛けてから割る。

`FND(0)` は Enterprise と Klingon `I` の距離であり、TBX では `DIST_TO_KLINGON(I)` helper に寄せる。

### セクター表現

原典は `Q$`, `R$`, `S$` という 3 本の固定長文字列で 8 x 8 sector display を保持する。TBX ではロジック用に `@SECTOR[8, 8]` を使い、表示時に symbol へ変換するのがよい。

```text
0 = 空
1 = Enterprise
2 = Klingon
3 = Starbase
4 = Star
```

### ライブラリコンピューター

`CMD_COM` は Mayfield `STTR1` の正規機能なので実装対象。とくに option 0 の cumulative galactic record は `@CHART[8, 8]` と連動する。

## 実装前チェックリスト

- [x] 原典 `STTR1` を repository に保存した
- [x] このメモに原典行番号との対応が入っている
- [x] 主要 gameplay formula に原典由来の line reference がある
- [x] `RND(1)` compatibility 方針を決める
- [ ] `Q$`/`R$`/`S$` string-map を `@SECTOR[8, 8]` に置き換える詳細設計を決める
- [ ] 表示幅・空白をどこまで原典に合わせるか決める
- [ ] `docs/notes/sttr1.bas` の扱い（原典保存として残す / ライセンス・出典注記を追加する）を確認する
