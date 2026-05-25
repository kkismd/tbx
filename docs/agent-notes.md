# エージェント向け実装ノート

実装エージェントが詰まりやすい箇所・PR レビューで繰り返し指摘された事項・後続タスクで再利用できる注意点をまとめる。

実装前に一読すること。詳細は各リンク先 docs や issue を参照。

---

## この文書の使い方

- 実装を始める前に、該当セクションを確認する
- 同じ落とし穴を踏んだら、このファイルに追記する
- 詳細設計は [`docs/notes/star-trek-mayfield-1972.md`](notes/star-trek-mayfield-1972.md) など個別 docs を参照する

---

## TBX 構文上の落とし穴

### 文終端と1行1文の運用

TBX の文終端は改行またはセミコロン。セミコロンで区切れば同一行に複数文を書けるが、セミコロンなしで横並びに複数文を書くとパースエラーになる。

```tbx
# NG: セミコロンなしで横並び -> パースエラー
LET @COURSE_DY[1] = 0  LET @COURSE_DX[1] = 1

# OK: セミコロン区切り
LET @COURSE_DY[1] = 0; LET @COURSE_DX[1] = 1

# OK (推奨): 可読性のため1行1文
LET @COURSE_DY[1] = 0
LET @COURSE_DX[1] = 1
```

エージェントはセミコロン区切りの存在を見落としやすいため、実装例や docs では可読性も兼ねて原則として1行1文で書く。

### 前方参照が解決されない

TBX は前方参照を解決しない。`DEF` の本体で呼び出す関数は、その `DEF` よりも前に定義する必要がある。

```tbx
# NG: INIT_COURSE_TABLE は後で定義されているため undefined symbol エラー
DEF INIT_GAME()
  INIT_COURSE_TABLE
END

DEF INIT_COURSE_TABLE()
  ...
END

# OK: 呼び出し前に定義する
DEF INIT_COURSE_TABLE()
  ...
END

DEF INIT_GAME()
  INIT_COURSE_TABLE
END
```

同一ファイル内・`USE` で読み込むファイル間の両方で、定義順に注意すること。

### `IF` はブロック構文

TBX の `IF` は HP BASIC の `IF ... THEN <行番号>` ではなく、構造化されたブロック構文。

```tbx
IF condition
  ...
ELSE
  ...
ENDIF
```

`IF condition THEN ...` の形は TBX 構文ではない。docs に擬似コードを書くときも `IF ... THEN` や `IF ... ;` は使わず、ブロック形式か `text` フェンスで擬似コードと明示する（PR #767 の事例）。

---

## 配列とストレージの注意

### 配列名と関数名の混同を避ける

`@COURSE_DX[I]` という配列があるとき、accessor 関数の名前を `COURSE_DX(COURSE)` にすると配列名と区別しにくい。accessor 関数には `GET_` prefix を付けて区別する（PR #765/#769 の事例）。

```tbx
# NG: 配列名 @COURSE_DX と紛らわしい
DEF COURSE_DX(COURSE)
  RETURN @COURSE_DX[COURSE]
END

# OK: GET_ prefix で区別する
DEF GET_COURSE_DX(COURSE)
  RETURN @COURSE_DX[COURSE]
END
```

### 2D 配列は `[x, y]` = `[col, row]` の順

TBX の 2D 配列アクセスは `[x, y]` convention（x = col 方向、y = row 方向）。Mayfield HP BASIC の `[row, col]` 順とは逆になる。

| Mayfield HP BASIC | TBX 変数名 |
| --- | --- |
| `G[Q1, Q2]` (row=Q1, col=Q2) | `@GALAXY[QX, QY]` |
| `K[I, 1]` (row) | `@K_Y[I]` |
| `K[I, 2]` (col) | `@K_X[I]` |

docs に `(Q1, Q2)` を TBX の変数に対応させるとき、`Q1 -> ENT_QY`、`Q2 -> ENT_QX` と明示すること。「`(Q1,Q2)` = `(ENT_QX, ENT_QY)`」と書くと列/行の読み替えが隠れてしまう（PR #762 の事例）。

---

## STTR1 実装の注意

### course table の row/col -> dx/dy 読み替え

Mayfield 原典の `C[course, 1]` は row delta（縦方向）、`C[course, 2]` は col delta（横方向）。TBX の `[x, y]` convention では次のように読み替える。

| Mayfield | 意味 | TBX |
| --- | --- | --- |
| `C[course, 1]` | row delta | `@COURSE_DY[course]` |
| `C[course, 2]` | col delta | `@COURSE_DX[course]` |

詳細は [`docs/notes/star-trek-mayfield-1972.md`](notes/star-trek-mayfield-1972.md) の「TBX `@COURSE_DX` / `@COURSE_DY` への読み替え」節を参照。

### `QUAD_IDX` / `SECTOR_IDX` 方針への回帰禁止

過去の計画では `QUAD_IDX` / `SECTOR_IDX` のような 1D index helper が存在したが、2D 配列採用に伴い廃止された。`@GALAXY[QX, QY]` / `@SECTOR[SX, SY]` を直接使うこと。1D index に戻す実装は行わない。

### RND の移植方針

Mayfield の `RND(1)` (0 以上 1 未満の浮動乱数) は TBX の整数乱数 `RND(N)` に次のように対応させる。

| Mayfield パターン | TBX 移植方針 |
| --- | --- |
| `INT(RND(1)*N + A)` | `RND(N) + A - 1` |
| `RND(1) > p` | `RND(100) > p*100` (percent check) |
| `2*RND(1)` | `RND(200) - 1` (scale 100 係数) |

`examples/trek/util.tbx` に `ROLL_PERCENT`・`RAND_FACTOR_0_TO_2_100`・`CHANCE` の helper がある。

---

## ブランチと PR の運用

### STTR1 integration branch workflow

STTR1 関連の作業はすべて integration branch 経由で行う。

- **作業ブランチ**: `issue/470-sttr1-porting` から切る
- **PR base branch**: `issue/470-sttr1-porting`（`main` に直接マージしない）

```bash
git switch issue/470-sttr1-porting
git checkout -b issue/<N>-short-description
# 実装後
gh pr create --base issue/470-sttr1-porting ...
```

### PR の粒度

1 issue に対して 1 PR を作成する。navigation 全体など大きな機能は issue を分割する。「navigation 本体の実装前に course table 初期化だけを先に完結させる」のような粒度が目安。

---

## docs 記述上の注意

### docs の方針説明と実装を必ず一致させる

docs に「後続実装では ○○ を使う」と書いたら、実装でも同じ方針を採用する。docs の方針と実装がズレていると PR レビューで指摘される（PR #769 の事例）。実装方針が変わったら、docs 側も同時に更新すること。

### code fence の backtick は3つ

Markdown の code fence は backtick 3つ（` ``` `）で統一する。backtick 4つにするとネストが崩れる（PR #767 の事例）。

---

## テスト記述上の注意

### `ASSERT` に 2D 配列アクセスを渡すときは括弧が必要

`ASSERT @ARRAY[X, Y] = VALUE` のように 2D 配列インデックスのカンマが引数トークン列に含まれると、TBX は `argc=2` の 2 引数呼び出しと解釈してアサーションが失敗する。

**根本原因**: TBX の `count_top_level_arity`（`interpreter.rs`）は `(` / `)` でのみネスト深度を追跡し、`[` / `]` は追跡しない。そのため `@SECTOR[ENT_SX, ENT_SY]` 内のカンマを深度0のトップレベル区切りとみなし、arity を 2 と算定する。

```tbx
# NG: [ENT_SX, ENT_SY] 内のカンマが argc=2 を引き起こし assertion failed
ASSERT @SECTOR[ENT_SX, ENT_SY] = 1

# OK: 外側の () でカンマを depth=1 に押し込み argc=1 にする
ASSERT (@SECTOR[ENT_SX, ENT_SY] = 1)
```

変数同士の比較（`ASSERT VAR1 = VAR2`）はカンマを含まないため argc=1 となり問題ない。
関数呼び出し結果との比較（`ASSERT FOO() = 1`）も `()` でカンマが囲まれるため問題ない。
**2D 配列アクセスを含む比較は必ず外側を `()` で包む**（PR #775 の事例）。

### `VAR x = expr` はトップレベルでは使えない

`VAR x = expr`（初期化付き宣言）は `DEF ... END` ブロックの中でのみ有効。トップレベルに書くと `VAR initializer '= expr' is not allowed outside DEF` エラーになる。

```tbx
# NG: DEF の外では使えない
VAR SUMMARY = @GALAXY[ENT_QX, ENT_QY]

# OK: DEF の中に書くか、トップレベルでは VAR と LET を分ける
VAR SUMMARY
LET SUMMARY = @GALAXY[ENT_QX, ENT_QY]
```

smoke test など、ロジックをトップレベルに書きたい場合は `DEF RUN_SMOKE_TEST() ... END` に包んで末尾で呼び出す（PR #775 の事例）。

### `USE` のパスはインクルード元ファイルのディレクトリ基準

`USE "path/to/file.tbx"` のパスは、`USE` を書いたファイルのディレクトリを起点とした相対パスで解決される（プロセスの CWD ではない）。

```tbx
# examples/trek/test_foo.tbx から同ディレクトリのファイルを読む場合
USE "state.tbx"          # OK: examples/trek/state.tbx
USE "util.tbx"           # OK: examples/trek/util.tbx

# lib/tests/helper.tbx を読む場合（上2階層）
USE "../../lib/tests/helper.tbx"  # OK

# NG: CWD基準の絶対的パスは動かない
USE "examples/trek/state.tbx"     # NG: examples/trek/examples/trek/state.tbx になる
```

`cargo test` 経由で `lib/tests/` 配下のファイルを実行するとき、テストランナーが `set_base_dir` でプロジェクトルートを設定するため `USE "lib/tests/helper.tbx"` が通る。`tbx` バイナリで直接実行するときはファイル基準の相対パスになる（PR #775 の事例）。
