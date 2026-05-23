# Mike Mayfield STAR TREK 1972 HP BASIC ルール抽出メモ

Tracking issue: #470

## 目的

Mike Mayfield が 1972 年に HP 2000C / HP Time-Shared BASIC 向けに書き直した `STAR TREK` (`STTR1`) を、現行 TBX へ移植するためのルール抽出メモ。

このメモは、後続の TBX 実装が `Super Star Trek` 系の拡張へ流れないように、Mayfield 版のゲーム構造・データ構造・乱数分布・戦闘式・入出力を明文化するための作業台とする。

## 参照元

### Primary target

- Mike Mayfield / Centerline Engineering, HP 2000C BASIC `STTR1`
  - Public pointer: `http://bitsavers.informatik.uni-stuttgart.de/bits/HP/tapes/2000tsb/`
  - 現時点では、この作業メモ作成時にディレクトリ内の実ソースまでは取得できていない。後続作業で `STTR1` のテキストを取得し、行番号単位の抽出へ進む。

### Secondary references

- `Star Trek (1971 video game)` article
  - Mayfield が SDS Sigma 7 版の後、HP 2000C 向けに 1972 年に書き直したこと
  - HP public domain Contributed Program library に `STTR1` として入ったこと
  - 後に DEC BASIC-PLUS へ port され、Bob Leedom の `Super Star Trek` へ発展したこと
- `HP Time-Shared BASIC` article
  - HP TSB の構文差分を確認する補助資料
- `What to Do After You Hit Return` (People's Computer Company, 1975)
  - Mayfield 系の掲載版として確認対象

## 現時点で確認済みの高レベル仕様

以下は secondary reference から確認できる一般的な Mayfield 系 `STAR TREK` の構造。原典 `STTR1` ソース確認後、行番号・変数名・式レベルで検証する。

### ゲーム目的

- プレイヤーは Enterprise を操作し、制限時間内に全 Klingon を撃破する。
- Enterprise は energy を消費しながら移動・攻撃・防御を行う。
- Starbase で補給できる。
- 敗北条件には Enterprise の破壊、期限切れ、energy 枯渇系が含まれる可能性が高い。

### 空間構造

- Galaxy は 8 x 8 の quadrant で構成される。
- 各 quadrant は 8 x 8 の sector で構成される。
- 各 quadrant には Klingon / starbase / star の数が初期配置される。
- quadrant 内の実配置は、その quadrant に入ったときに sector map として扱われる。

### 表示・スキャン

- Short-range scan は現在 quadrant の 8 x 8 sector map を表示する。
- Long-range scan は Enterprise 周辺 quadrant の summary を表示する。
- Sector map には Enterprise, Klingon, starbase, star が記号で表示される。

### 戦闘

- Phaser は照準不要だが、距離によって攻撃力が減衰する。
- Photon torpedo は一撃で Klingon を破壊できるが、course / angle 指定が必要。
- Klingon は turn-based に反撃する。

### 移動

- Warp drive により sector 内・quadrant 間を移動する。
- 移動には energy と stardate/time が消費される。

## Mayfield 版と Super Star Trek 版の境界

この issue では Mayfield 版を正とする。Bob Leedom / Ahl 系の `Super Star Trek` で追加された可能性がある要素は、原典で確認できるまで実装対象にしない。

### 原則として入れないもの

- 3文字コマンド体系を前提にした UI
- 移動する Klingon
- 拡張 library computer
- quadrant 名や show character による status report
- fire control computer などの支援機能
- Death Ray / shuttle / dilithium crystal など、派生版由来の緊急オプション

### 例外

Mayfield `STTR1` 原典に存在することが確認できたものは、このリストにあっても実装対象に戻す。

## HP Time-Shared BASIC 読み替えメモ

原典ソースを読む際の注意点。

- 行番号は mandatory。TBX では構造化された `DEF` / `WHILE` / `IF` へ分解する。
- `GOSUB` / `RETURN` は TBX の `DEF ... END` に対応させる。
- `GOTO expr OF ...` / `GOSUB expr OF ...` は dispatch table として読む。
- HP TSB の配列・文字列添字は 1-origin。TBX の配列も 1-origin なので、そのまま寄せやすい。
- HP TSB には `SQR`, `RND`, `INT` などの数学関数がある。TBX 側では `SQRT`, `RND`, `INT` などへ対応させる。

## TBX 実装への写像

### 配列

現行 TBX では旧 `ARRAY(64)` / `A(i)` ではなく、`DIM @A[n]` と `@A[i]` / `LET @A[i] = expr` を使う。

```tbx
DIM @GALAXY[64]
DIM @CHART[64]
DIM @SECTOR[64]
DIM @K_R[3]
DIM @K_C[3]
DIM @K_E[3]
DIM @DAMAGE[8]
```

### 座標変換

```tbx
DEF QUAD_IDX(QR, QC)
  RETURN (QR - 1) * 8 + QC
END

DEF SECTOR_IDX(SR, SC)
  RETURN (SR - 1) * 8 + SC
END
```

### 入出力

- Numeric input: `GETDEC`
- String input: `GETSTR`
- String output: `PUTSTR`
- Numeric output: `PUTDEC` / `PUTVAL`
- String compare: `STR_EQ`

### 乱数

- TBX `RND(n)` は `[1, n]` の整数乱数として扱う。
- Mayfield 版で `RND(1)` floating distribution が使われている場合は、TBX で同じ分布を直接再現できるか別途判断する。
- 乱数分布が gameplay に効く箇所は、helper 化して式の意図を残す。

## 原典ソース取得後に埋める対応表

### 行番号レンジ別構造

| HP BASIC line range | 役割 | TBX 移植先 | 状態 |
| --- | --- | --- | --- |
| TBD | Mission initialization | `INIT_GAME` / `INIT_GALAXY` | 未確認 |
| TBD | Quadrant generation | `INIT_QUADRANT` | 未確認 |
| TBD | Main command loop | `GAME_LOOP` | 未確認 |
| TBD | Short-range scan | `PRINT_SRS` | 未確認 |
| TBD | Long-range scan | `PRINT_LRS` | 未確認 |
| TBD | Navigation | `CMD_NAV` | 未確認 |
| TBD | Phaser attack | `CMD_PHA` | 未確認 |
| TBD | Photon torpedo | `CMD_TOR` | 未確認 |
| TBD | Klingon attack | `KLINGON_ATTACK` | 未確認 |
| TBD | Docking / repair | `CHECK_DOCKED` / `REPAIR_DAMAGE` | 未確認 |
| TBD | Endgame / scoring | `CHECK_ENDGAME` / `PRINT_SCORE` | 未確認 |

### 変数対応表

| HP BASIC variable | 意味 | TBX name | 状態 |
| --- | --- | --- | --- |
| TBD | Enterprise quadrant row | `ENT_QR` | 未確認 |
| TBD | Enterprise quadrant col | `ENT_QC` | 未確認 |
| TBD | Enterprise sector row | `ENT_SR` | 未確認 |
| TBD | Enterprise sector col | `ENT_SC` | 未確認 |
| TBD | Energy | `ENERGY` | 未確認 |
| TBD | Shields | `SHIELDS` | 未確認 |
| TBD | Photon torpedoes | `TORPEDOES` | 未確認 |
| TBD | Stardate | `STARDATE` | 未確認 |
| TBD | Mission deadline | `MISSION_END` | 未確認 |
| TBD | Total Klingons left | `KLINGONS_LEFT` | 未確認 |
| TBD | Starbases left | `BASES_LEFT` | 未確認 |

## 抽出TODO

### 1. Source acquisition

- [ ] bitsavers `2000tsb` 配下から `STTR1` の HP BASIC source を特定する
- [ ] 取得したソースが tokenized / tape image の場合、テキスト化手順を記録する
- [ ] `STTR1` が複数 variant ある場合、Mayfield / Centerline Engineering 由来の版を優先する

### 2. Rule extraction

- [ ] 初期 Klingon 数・starbase 数・star 数の乱数分布を抽出する
- [ ] mission duration / stardate 初期化式を抽出する
- [ ] quadrant summary encoding を抽出する
- [ ] short-range scan の表示記号を抽出する
- [ ] long-range scan の表示形式を抽出する
- [ ] command number / prompt / dispatch を抽出する
- [ ] navigation course / warp factor の式を抽出する
- [ ] phaser damage formula を抽出する
- [ ] photon torpedo trajectory / hit 判定を抽出する
- [ ] Klingon attack formula を抽出する
- [ ] device damage / repair rules を抽出する
- [ ] docking rules を抽出する
- [ ] victory / defeat / scoring rules を抽出する

### 3. TBX design decisions

- [ ] Mayfield の 2D 配列を 64 要素 1D array に flatten する方針を確定する
- [ ] floating `RND` を使う箇所がある場合、TBX での再現方法を決める
- [ ] 表示幅・空白をどこまで原典に合わせるか決める
- [ ] `Super Star Trek` 由来の拡張を混入させないチェックリストを作る

## 実装前チェックリスト

- [ ] このメモに原典行番号との対応が入っている
- [ ] 各 gameplay formula に原典由来の line reference がある
- [ ] 不明点が `TBD` として残っている場合、実装側で仮定しない
- [ ] `Super Star Trek` 由来の追加仕様は別セクションに隔離されている

## 備考

この初版は、原典 `STTR1` ソース取得前の調査メモ骨格である。後続 commit で原典行番号に基づく具体値・式・変数対応表へ更新する。
