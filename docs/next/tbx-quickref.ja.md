# TBX Next クイックリファレンス

この文書は、TBX Next のプログラムを書く人間およびエージェント向けの実用メモである。

TBX Next は開発中であり、**現在の挙動の正本は `crates/tbx-next/` のコードとテスト**である。この文書は完全な言語仕様書ではなく、現在実装済みの構文・標準語彙・書き方を素早く確認するための入口を示す。

## この文書の位置づけ

- 実装事実と現在の挙動: `crates/tbx-next/src/` と `crates/tbx-next/tests/`
- 重要な設計判断: TBX Next の ADR issue
- 実行例: `docs/next/examples/`
- 実用入口: この文書

現行 TBX の `blueprint.md`、`blueprint-language.md`、`blueprint-compiler.md` および `docs/tbx-quickref.ja.md` は TBX Next の仕様ではない。構文や語彙をそのまま流用しないこと。

## 実行方法

ファイルを実行する。

```sh
cargo run -p tbx-next -- docs/next/examples/prime.tbx
```

標準入力からソースを与える場合は、ファイル引数を省略する。

```sh
printf 'PUTDEC 2 + 3 * 4\nCR\n' | cargo run -p tbx-next --bin tbx-next
```

ファイルを実行する場合、プロセスの標準入力は `INPUT?` の実行時入力にも使われる。標準入力からソースを与えるモードでは、ソース読み込みに標準入力を使い切るため、同じ入力が `INPUT?` に暗黙に共有されることはない。

開発時の基本確認は次の通り。

```sh
cargo build -p tbx-next
cargo test -p tbx-next
```

## 基本モデル

TBX Next は BASIC 風のソース構文を持つ一方、実行時にはデータスタックとワードを使う。

- 現在の実行時値は符号付き 16 bit 整数 (`i16`) のみ
- 算術は checked arithmetic であり、オーバーフローは実行時エラー
- `0` は偽、`0` 以外は真として条件判定する
- `A` から `Z` までの 26 個のスカラー変数は起動時から存在する
- `VAR` で追加のグローバル変数を公開できる
- ランタイムワードはデータスタック上の値を消費・生成できる
- `VAR`, `LET`, `EVAL`, `DEF`, `IF`, `SYNTAX`, `USE`, `PRINT` はソース処理ワードであり、通常のランタイムワードとは役割が異なる

文字列は現在の一般的な実行時値ではない。`PRINT "text"` の文字列リテラルは出力構文の一部として扱われる。

## 名前

ワード名と変数名は ASCII の大文字小文字を区別しない。

```tbx
LET A = 1
putdec a
CR
```

名前は原則として次の形である。

```text
[A-Za-z_][A-Za-z0-9_]*\??
```

末尾の `?` は `PRIME?` や `INPUT?` のような名前に使える。

## コメント

TBX Next は `#` と `REM` の行コメントを使える。どちらも物理行末または EOF で終わる。

```tbx
# This is a comment.
LET A = 42
LET A = A + 1 # 行の途中からもコメントにできる
REM This is also a comment.
```

`#` は文字列リテラルや文字リテラルの内部を除き、行の途中でもコメントを開始する。開始後の文字は字句検証されない。`REM` は大文字小文字を区別せず、局所行番号のない論理文の先頭でだけコメントを開始する。したがって `PUTDEC A REM note` は行内コメントにならず、`REM` は通常のワード名や変数名としても使えない。

コメントだけの行には局所行番号を付けられない。実行可能な文には局所行番号と行内 `#` コメントを併用できる（例: `100 PRINT A # note`）。

## 整数と式

整数リテラルと、次の主な演算子を式で使える。

| 種別 | 演算子 |
| --- | --- |
| 算術 | `+`, `-`, `*`, `/`, `%` |
| 比較 | `=`, `!=`, `<`, `<=`, `>`, `>=` |
| グループ化 | `(`, `)` |

例:

```tbx
LET A = 2 + 3 * 4
IF (A % 2) = 0
  PRINT "even"
ENDIF
```

除算は整数除算である。0 除算、`i16` の範囲を超える演算、`ABS(-32768)` など表現不能な演算は実行時エラーになる。

## 変数

### 組み込み変数 A-Z

`A` から `Z` は最初から存在する。

```tbx
LET A = 10
LET B = A + 5
PUTDEC B
CR
```

### VAR

追加のグローバル変数は `VAR NAME` で宣言する。

```tbx
VAR COUNT
LET COUNT = 3
PUTDEC COUNT
CR
```

現在の `VAR` はグローバルな名前を公開する。現行 TBX のローカル `VAR` と同じものとして扱わないこと。

### LET

代入は `LET name = expression` で行う。

```tbx
LET A = A + 1
```

## グローバル配列

`DIM @NAME[n]` でグローバル配列を宣言する。宣言サイズは `1..=32767` の正の整数リテラルに限られ、要素は宣言時にすべて `0` で初期化される。

```tbx
DIM @SQUARE[10]

LET I = 1
WHILE I <= 10
  LET @SQUARE[I] = I * I
  LET I = I + 1
ENDWH

LET I = 1
WHILE I <= 10
  PRINT I, " ", @SQUARE[I]
  CR
  LET I = I + 1
ENDWH
```

`@` は配列名を示し、`[]` は添字を囲む。要素の読み取りは式中の `@NAME[index]`、書き込みは `LET @NAME[index] = expression` と書く。添字は **1-origin** で、有効範囲は `1..=n`。`0`、負数、宣言サイズを超える添字は実行時エラーになる。`@NAME` 単体は実行時配列値ではなく、配列要素には添字が必要である。

配列名はワード、ソースワード、スカラー変数と同じ大文字小文字を区別しない名前空間を共有するため、同名のbindingは宣言できない。公開済み配列の再宣言やサイズ変更も現行仕様ではできない。

`USE` は同じsource-processing sessionで実行されるため、読み込み前に宣言した配列を読み込み先で使え、読み込み先で宣言した配列も復帰後のソースや入れ子の `USE` から使える。

現行の配列機能はグローバル配列の要素読み書きに限られる。`ARRAY_LEN`、実行時サイズ式、多次元配列、実行時配列値、配列要素アドレスは提供していない。

## EVAL とデータスタック

`EVAL expression` は式を評価し、結果をデータスタックへ残す。

```tbx
EVAL 6
EVAL 7
ADD
PUTDEC
CR
```

上のコードは `13` を出力する。

スタック上の値を操作する主なランタイムワード:

| ワード | 動作 |
| --- | --- |
| `DUP` | スタック最上位を複製する |
| `DROP` | スタック最上位を捨てる |

式の中ではランタイムワードを呼び出して、そのスタック結果を式の値として使える。

```tbx
IF DUP() = 0
  DROP
ENDIF
```

## ランタイムワード定義

### 基本形

```tbx
DEF DOUBLE
  DUP
  ADD
END

EVAL DOUBLE(7)
PUTDEC
CR
```

`DOUBLE(7)` は引数をスタックへ積んで `DOUBLE` を呼び出し、ワードが残したスタック上の値を式側で使う。

### 局所参照名

`DEF` の名前の後ろに、呼び出し時のスタック位置へ名前を付けられる。

```tbx
DEF AREA width, height
  EVAL width * height
END
```

局所参照名は大文字小文字を区別せず、call-base の下位から上位の順に対応する。

重要な点として、これは**実行時 arity の宣言ではなく、戻るときに引数を自動消費する仕組みでもない**。必要な値の消費はワード本体のスタック操作として記述する。`docs/next/examples/prime.tbx` では、局所参照 `candidate` を使った計算の後に `DROP` で元の引数を明示的に消費している。

実際のスタック効果はワード本体に依存するため、定義を読むときは局所参照名だけで引数個数や戻り値個数を判断しないこと。

## 条件分岐

組み込みの `IF` は `ELSIF`, `ELSE`, `ENDIF` を持つ。

```tbx
IF A < 0
  PRINT "negative"
ELSIF A = 0
  PRINT "zero"
ELSE
  PRINT "positive"
ENDIF
```

- `ELSIF` は 0 回以上使える
- `ELSE` は省略可能で、使う場合は 1 回
- `ENDIF` は必須

条件値は `0` が偽、0 以外が真である。

## 反復

`WHILE` と `DO` は起動時に読み込まれる標準ライブラリ `crates/tbx-next/stdlib/basic.tbx` で TBX Next 自身の `SYNTAX` を使って定義されている。

### WHILE / ENDWH

```tbx
LET A = 0
WHILE A < 3
  PUTDEC A
  CR
  LET A = A + 1
ENDWH
```

### DO / UNTIL

```tbx
LET A = 0
DO
  LET A = A + 1
UNTIL A >= 3
```

## 出力

### PRINT

`PRINT` は複数の文字列リテラルと整数式をカンマ区切りで連結して出力する。自動では改行しない。

```tbx
LET A = 4
LET B = 5
PRINT "TOTAL = ", A + B, "!"
CR
```

出力:

```text
TOTAL = 9!
```

### PUTDEC

スタック最上位の整数を 10 進で出力し、その値を消費する。式を続けて書いて値を供給することもできる。

```tbx
PUTDEC 2 + 3 * 4
CR
```

### PUTCHR

スタック最上位を ASCII コードとして 1 文字出力し、その値を消費する。現在受け付ける範囲は `0..=127`。

```tbx
PUTCHR 65
CR
```

### CR

改行を出力する。データスタックは変更しない。

## 入力

`INPUT?` は 1 行を読み、スタックへ **値、成功フラグ** の順で 2 値を積む。

- 正しい `i16` 10進整数: `value 1`
- 不正な入力または EOF: `0 0`
- 実行時入力能力がない場合や入力I/Oに失敗した場合: 実行時エラー
- 先頭・末尾の空白とタブ、先頭の `+` / `-` は受け付ける
- 部分的な数値や `i16` 範囲外は失敗する

成功フラグがスタック最上位になる。

```tbx
INPUT?
IF DUP() = 0
  DROP
  DROP
  PRINT "Please enter a number."
  CR
ELSE
  DROP
  PRINT "value = ", DUP()
  CR
  DROP
ENDIF
```

対話入力を使う完全な例は `docs/next/examples/guess.tbx` を参照する。

## 乱数

`RND(N)` は `1..=N` の疑似乱数整数を返す。`N` は正でなければならない。

```tbx
LET A = RND(100)
```

通常の CLI 実行では乱数生成器はホスト側で seed されるため、ユーザープログラムで seed を設定しなくても利用できる。

## 主な算術・比較ランタイムワード

演算子に対応するランタイムワードも公開されている。

| ワード | 対応する処理 |
| --- | --- |
| `ADD` | 加算 |
| `SUBTRACT` | 減算 |
| `MULTIPLY` | 乗算 |
| `DIVIDE` | 整数除算 |
| `REMAINDER` | 余り |
| `NEGATE` | 符号反転 |
| `EQUAL?` | 等しい |
| `NOT_EQUAL?` | 等しくない |
| `LESS?` | 小さい |
| `LESS_EQUAL?` | 以下 |
| `GREATER?` | 大きい |
| `GREATER_EQUAL?` | 以上 |
| `ABS` | 絶対値 |

演算子を使える場所では通常 `A + B` や `A < B` の方が読みやすい。これらの名前付きワードは、スタック志向の定義や低水準の組み合わせで有用である。

## USE

別の TBX Next ソースを読み込むには文字列リテラルでパスを指定する。

```tbx
USE "sub/B.tbx"
```

ファイルから実行している場合、相対パスは読み込みを要求したソースを基準に解決され、入れ子の `USE` も扱える。循環読み込みはエラーになる。すでに正常完了した同一ソースの再読み込みは no-op になる。
グローバル配列も同じ session で共有されるため、読み込み元と読み込み先の双方から利用できる。

## SYNTAX によるソース構文拡張

`SYNTAX` はソース処理ワードを TBX Next 自身で定義するための入口である。単純なステートメント構文の例:

```tbx
SYNTAX SLET
  STATEMENT
  READ_NAME AS name
  RESOLVE_VAR name AS target
  EXPECT "="
  READ_EXPR AS expr
  EMIT_EXPR expr
  EMIT_STORE target
ENDS

SLET A = 10
```

標準ライブラリの `WHILE` と `DO` も `SYNTAX` で実装されている。`SYNTAX` の操作語彙はコンパイラ拡張向けの低水準 API であり、このクイックリファレンスでは網羅しない。新しい構文を書く場合は `crates/tbx-next/stdlib/basic.tbx`、`crates/tbx-next/src/source_word*.rs` と関連テストを確認すること。

## 現行 TBX と混同しやすい点

TBX Next は現行 `tbx` の互換実装ではない。特に次をそのまま持ち込まないこと。

| 項目 | TBX Next の現在の形 |
| --- | --- |
| ワード定義 | `DEF NAME local1, local2 ... END` |
| 戻り値 | `RETURN` ではなく、ワードがデータスタックへ残す値 |
| 式をスタックへ積む | `EVAL expression` |
| 値 | 現在は `i16` 整数のみ |
| 真偽 | `0` が偽、0 以外が真 |
| 変数 | A-Z 組み込み + `VAR` によるグローバル変数 |
| 配列 | `DIM @NAME[n]` で宣言し、`@NAME[index]` で要素を読む。`@NAME` 単体は値ではない |
| 文字列 | 一般的な実行時値ではなく、現在は主に `PRINT` / `USE` などのソース構文で使う |

## 代表的なサンプル

- `docs/next/examples/demo.tbx` — 最小実行例
- `docs/next/examples/stack_function.tbx` — スタックを使うワード
- `docs/next/examples/prime.tbx` — 局所参照名、条件分岐、反復
- `docs/next/examples/guess.tbx` — `RND`, `INPUT?`, `PRINT`
- `docs/next/examples/mandelbrot.tbx` — 整数演算、反復、`ABS`, `PUTCHR`
- `docs/next/examples/squares.tbx` — グローバル配列への保存と読み出し

## 実装を確認する場所

挙動の詳細やエラー条件を確認するときは、まず次を見る。

- CLI と縦断動作: `crates/tbx-next/tests/cli_e2e.rs`
- グローバル配列のstorage: `crates/tbx-next/src/global_array.rs`
- ソース処理: `crates/tbx-next/src/source_processor.rs`, `crates/tbx-next/src/source_word.rs`
- 式: `crates/tbx-next/src/expression.rs`, `crates/tbx-next/src/operator.rs`
- 実行時値: `crates/tbx-next/src/value.rs`
- VM / スタック: `crates/tbx-next/src/vm.rs`, `crates/tbx-next/src/stack_primitive.rs`
- 入出力: `crates/tbx-next/src/input_primitive.rs`, `crates/tbx-next/src/output_primitive.rs`
- 乱数: `crates/tbx-next/src/random_primitive.rs`
- 標準制御構文: `crates/tbx-next/stdlib/basic.tbx`

設計理由が必要な場合は、コード中で参照されている ADR issue を確認すること。
