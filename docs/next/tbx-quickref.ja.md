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

乱数系列を再現したい場合は、ソースファイルより前に `--seed` と10進の
`0..=u64::MAX` を指定できる。ファイルを省略して標準入力からソースを読む場合も同じである。

```sh
cargo run -p tbx-next -- --seed 42 docs/next/examples/guess.tbx
printf 'PUTDEC RND(10)\n' | cargo run -p tbx-next -- --seed 42
```

同じソース・入力・seedを使う `--seed` 指定のCLI実行では、TBX Nextのバージョンや
実行プラットフォームをまたいでも同じ乱数系列を再現できる。seedを省略した通常の
CLI実行では、ホストが非決定的なseedを選ぶ。この場合の乱数生成器は将来変更される
可能性があり、明示seedの互換系列とは別の扱いになる。

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
- グローバル変数は `VAR` で明示的に公開する
- `DEF ... END` で定義したランタイムワード内では `I,J,K,L,M,N,X,Y` を呼出しごとのローカルスクラッチとして宣言なしで使える
- ランタイムワードはデータスタック上の値を消費・生成できる
- `VAR`, `LET`, `EVAL`, `DEF`, `IF`, `SYNTAX`, `USE`, `PRINT` はソース処理ワードであり、通常のランタイムワードとは役割が異なる

文字列は現在の一般的な実行時値ではない。`PRINT "text"` の文字列リテラルは出力構文の一部として扱われる。

## 名前

ワード名と変数名は ASCII の大文字小文字を区別しない。

```tbx
VAR COUNT
LET count = 1
putdec COUNT
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
VAR COUNT
# This is a comment.
LET COUNT = 42
LET COUNT = COUNT + 1 # 行の途中からもコメントにできる
REM This is also a comment.
```

`#` は文字列リテラルや文字リテラルの内部を除き、行の途中でもコメントを開始する。開始後の文字は字句検証されない。`REM` は大文字小文字を区別せず、局所行番号のない論理文の先頭でだけコメントを開始する。したがって `PUTDEC COUNT REM note` は行内コメントにならず、`REM` は通常のワード名や変数名としても使えない。

コメントだけの行には局所行番号を付けられない。実行可能な文には局所行番号と行内 `#` コメントを併用できる（例: `100 PRINT COUNT # note`）。局所行番号を分岐先として使う方法は「条件分岐」の `BIF` を参照する。

## 整数と式

整数リテラルと、次の主な演算子を式で使える。

| 種別 | 演算子 |
| --- | --- |
| 算術 | `+`, `-`, `*`, `/`, `%` |
| 比較 | `=`, `!=`, `<`, `<=`, `>`, `>=` |
| 論理 | `NOT`, `AND`, `OR` |
| グループ化 | `(`, `)` |

例:

```tbx
VAR VALUE
LET VALUE = 2 + 3 * 4
IF (VALUE % 2) = 0
  PRINT "even"
ENDIF
```

除算は整数除算である。0 除算、`i16` の範囲を超える演算、`ABS(-32768)` など表現不能な演算は実行時エラーになる。

論理演算では `0` が偽、0 以外が真で、結果は必ず `0` または `1` になる。優先順位は強い順に
`単項 -`、`* / %`、`+ -`、比較、`NOT`、`AND`、`OR`、カンマである。`AND` と `OR` は短絡評価せず、
左右を左から右へ必ず評価する。右辺を条件付きで実行したい場合は、`IF` などの制御構造を使う。

## 変数

### グローバル変数

グローバル変数は `VAR NAME` で明示的に宣言する。起動時に暗黙作成される `A` から `Z` の変数はない。

```tbx
VAR COUNT
LET COUNT = 3
PUTDEC COUNT
CR
```

現在の `VAR` はソース処理セッション全体へグローバルな名前を公開する。宣言された値はワード呼出しをまたいで保持される。現行 TBX のローカル `VAR` と同じものとして扱わないこと。

一文字名も特別扱いされないため、必要なら通常のグローバルとして宣言できる。

```tbx
VAR I
LET I = 10
PRINT I
CR
```

### `DEF ... END` で定義したワードのローカルスクラッチ

`DEF ... END` で定義したランタイムワード（以下、この節では「定義ワード」と呼ぶ）の本体では、`I,J,K,L,M,N,X,Y` の8個を宣言なしのローカルスクラッチとして使える。

```tbx
DEF SCRATCH_DEMO
  LET I = 1
  EVAL I + 1
END
```

スクラッチには次の規則がある。

- 各定義ワードの呼出しごとに8個の独立した領域を持つ
- 呼出し開始時はすべて `0`
- 呼出し元と呼出し先は別のスクラッチを持つため、呼出し先が同じ文字を使っても呼出し元の値を変更しない
- 通常の `END` 到達または `RETURN` でその呼出しが終わると、そのスクラッチの寿命も終わる
- トップレベルにはスクラッチはなく、未宣言の `I` などは通常の未定義名エラーになる
- 各文字の用途は言語仕様として固定されていない

`VAR I` のような同名グローバルを宣言すること自体は許される。ただし定義ワード内の**裸の値参照 `I`** と **`LET I = ...`** は、そのグローバルではなく現在の呼出しのスクラッチへ解決される。トップレベルの `I` は宣言済みグローバルへ解決される。

スクラッチ名はグローバルの名前表へ公開されない。そのため、構文上ワード呼出しや配列参照であることが明確な場合はスクラッチとして解決しない。たとえば、その名前のワードまたは配列が定義済みなら、文の先頭の `I` と `I()` はワード呼出し、`@I[index]` は配列参照として扱われる。

`DEF` ヘッダーの局所参照名にはスクラッチ名を使えない。次はエラーになる。

```tbx
DEF BAD I
  EVAL I
END
```

8個で足りない場合は、まず値の再利用・再計算・小さなデータスタック退避・処理の分割を検討する。スクラッチ不足だけを理由に短命値をグローバルへ移すことは避ける。長寿命または呼出しをまたいで共有すべき状態には、意味を表す名前の `VAR` を使う。

### LET

代入は `LET name = expression` で行う。トップレベルの代入先は宣言済みグローバルでなければならない。定義ワード内では、固定スクラッチ名への `LET` は現在の呼出しのスクラッチへ書き込む。

```tbx
VAR COUNT
LET COUNT = COUNT + 1
```

## グローバル配列

`DIM @NAME[n]` でグローバル配列を宣言する。宣言サイズは `1..=32767` の正の整数リテラルに限られ、要素は宣言時にすべて `0` で初期化される。

```tbx
DIM @SQUARE[10]
VAR INDEX

LET INDEX = 1
WHILE INDEX <= 10
  LET @SQUARE[INDEX] = INDEX * INDEX
  LET INDEX = INDEX + 1
ENDWH

LET INDEX = 1
WHILE INDEX <= 10
  PRINT INDEX, " ", @SQUARE[INDEX]
  CR
  LET INDEX = INDEX + 1
ENDWH
```

`@` は配列名を示し、`[]` は添字を囲む。要素の読み取りは式中の `@NAME[index]`、書き込みは `LET @NAME[index] = expression` と書く。添字は **1-origin** で、有効範囲は `1..=n`。`0`、負数、宣言サイズを超える添字は実行時エラーになる。`@NAME` 単体は実行時配列値ではなく、配列要素には添字が必要である。

### PACK

`PACK @NAME = expression` は、宣言済み固定長配列へ複数のstack値を格納するソースワードである。右辺を通常の式として評価した後、配列長を `n` としてデータスタック上位 `n` 値を消費し、値の順序を保って `@NAME[1]` から `@NAME[n]` へ格納する。

```tbx
DIM @DATA[3]
PACK @DATA = 10, 20, 30
```

`PACK` が消費する値数は右辺のカンマ区切り項数ではなく配列長で決まる。データスタック上に `n` を超える値がある場合、上位 `n` 値だけを消費し、それより下の値は保持する。`PACK` は配列をruntime valueとして公開せず、対象配列はsource-processing時に解決される。

配列名はワード、ソースワード、スカラー変数と同じ大文字小文字を区別しない名前空間を共有するため、同名のbindingは宣言できない。公開済み配列の再宣言やサイズ変更も現行仕様ではできない。

`USE` は同じsource-processing sessionで実行されるため、読み込み前に宣言した配列を読み込み先で使え、読み込み先で宣言した配列も復帰後のソースや入れ子の `USE` から使える。

現行の配列機能はグローバル配列の要素読み書きと `PACK` による固定長配列への複数値格納を提供する。`ARRAY_LEN`、実行時サイズ式、多次元配列、実行時配列値、配列要素アドレスは提供していない。

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
| `SWAP` | スタック最上位2値を交換する |
| `DEPTH` | 実行前のデータスタック深さを消費せずに `i16` として積む (`(-- depth)`) |

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

`RETURN` は `DEF ... END` 本体から呼出し元へ早期に正常復帰する独立文である。`RETURN` 自体は値を指定せず、戻り値が必要な場合は従来どおりデータスタックへ残す。処理系は復帰時にデータスタックを自動調整しないため、不要な値や引数の消費はワード本体で `DROP` などを明示する。`RETURN` はトップレベルや quotation 内では使えない。

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

局所参照名は同じ定義内のスクラッチと名前を共有できない。
`I,J,K,L,M,N,X,Y` をヘッダーの局所参照名に指定すると定義時にエラーになる。

## 条件分岐

### 局所行番号と BIF

`BIF condition, line-number` は条件付きの直接分岐である。conditionを通常の式として評価し、結果が `0` のとき、指定した局所行番号へ分岐する。0以外なら次の文へ進む。

```tbx
VAR FLAG
LET FLAG = 0
BIF FLAG, 100
PRINT "not reached"
100 PRINT "done"
```

この例では `FLAG` が0なので、`BIF FLAG, 100` は局所行番号 `100` の文へ分岐する。

- 分岐先の数値は通常の式ではなく、局所行番号の識別子である
- 局所行番号は、分岐先として必要な実行可能文だけに付ければよい
- 分岐先は同じ局所行番号の有効範囲内に限られる
- 通常の条件分岐や反復には、`IF`、`WHILE`、`DO` などの構造化制御も利用できる

### IF / ELSIF / ELSE / ENDIF

`IF` は `ELSIF`, `ELSE`, `ENDIF` を持つ。

```tbx
VAR VALUE
IF VALUE < 0
  PRINT "negative"
ELSIF VALUE = 0
  PRINT "zero"
ELSE
  PRINT "positive"
ENDIF
```

- `ELSIF` は 0 回以上使える
- `ELSE` は省略可能で、使う場合は 1 回
- `ENDIF` は必須

条件値は `0` が偽、0 以外が真である。

### SELECT / CASE / CASE_ELSE / ENDSEL

`SELECT` は1つのselectorを複数の `CASE` と比較する。

```tbx
VAR CHOICE
SELECT CHOICE
CASE 1
  PRINT "one"
CASE 2
  PRINT "two"
CASE_ELSE
  PRINT "other"
ENDSEL
```

- selectorは `SELECT` 開始時に1回だけ評価する
- `CASE` はソース順に評価する
- 最初に一致した `CASE` bodyだけを実行し、後続CASEへfall-throughしない
- `CASE` は1回以上必要
- `CASE_ELSE` は省略可能で、使う場合は最大1回かつ最後に置く
- 一致するCASEがなく `CASE_ELSE` もなければ何も実行しない

## 反復

`WHILE`, `DO`, `SELECT`, `FOR` は起動時に読み込まれる標準ライブラリ `crates/tbx-next/stdlib/basic.tbx` で TBX Next 自身の `SYNTAX` を使って定義されている。

### WHILE / ENDWH

```tbx
VAR COUNT
LET COUNT = 0
WHILE COUNT < 3
  PUTDEC COUNT
  CR
  LET COUNT = COUNT + 1
ENDWH
```

`WHILE` の正規終端は `ENDWH` である。旧終端 `WEND` は現行構文として残していない。

### DO / UNTIL

```tbx
VAR COUNT
LET COUNT = 0
DO
  LET COUNT = COUNT + 1
UNTIL COUNT >= 3
```

### FOR / NEXT

```tbx
VAR LOOP_INDEX
FOR LOOP_INDEX = 1 TO 5
  PRINT LOOP_INDEX
  CR
NEXT
```

- ループ変数は既存のスカラー変数を使う
- `start` と `end` はループ開始時にそれぞれ1回だけ、`start` → `end` の順で評価する
- `start` をループ変数へ代入してから最初の反復可否を判定する
- 各反復のbody実行前に `loop_variable <= end` を判定する
- body実行後、現在のループ変数を `+1` する
- `start > end` の場合は0回反復する
- end値は開始時の評価結果で固定され、body内の他変数変更によって再評価しない
- body内でループ変数を書き換えた場合、`NEXT` はその時点の値へ1を加える
- `STEP`、降順FOR専用構文、`NEXT` への変数名指定は現在未対応

### BREAK

`BREAK` は、現在実行中の最内側のループを抜ける独立した文である。対象になるのは
`WHILE`、`DO`、`FOR` である。

```tbx
VAR COUNT
LET COUNT = 0
WHILE COUNT < 10
  LET COUNT = COUNT + 1
  IF COUNT = 3
    BREAK
  ENDIF
ENDWH
```

- `IF` と `SELECT` は `BREAK` の対象ではなく透過するため、その内側から外側のループを抜けられる
- ループが入れ子になっている場合は、最内側のループだけを抜ける
- ループの外で `BREAK` を使うとエラーになる
- `BREAK` による内部処理の後始末は実装詳細であり、利用者のデータスタックを操作する構文ではない
- `CONTINUE` は現在未対応

`IF` / `WHILE` / `DO` / `SELECT` / `FOR` は相互に入れ子にできる。

## 出力

### PRINT

`PRINT` は複数の文字列リテラルと整数式をカンマ区切りで連結して出力する。自動では改行しない。

```tbx
VAR LEFT_VALUE
VAR RIGHT_VALUE
LET LEFT_VALUE = 4
LET RIGHT_VALUE = 5
PRINT "TOTAL = ", LEFT_VALUE + RIGHT_VALUE, "!"
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
VAR ROLL
LET ROLL = RND(100)
```

通常の CLI 実行では乱数生成器はホスト側で seed されるため、ユーザープログラムで seed を設定しなくても利用できる。
`--seed` を指定した実行では、指定したseedに対応する互換乱数系列が使われる。既存の
seedの意味を変えず、同じsource・入力・seed・`RND`呼出し列から同じ系列を得られる
ことが契約されている。

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

演算子を使える場所では通常 `LEFT_VALUE + RIGHT_VALUE` や `VALUE < LIMIT` の方が読みやすい。これらの名前付きワードは、スタック志向の定義や低水準の組み合わせで有用である。

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

VAR VALUE
SLET VALUE = 10
```

標準ライブラリの `WHILE`, `DO`, `SELECT`, `FOR` も `SYNTAX` で実装されている。`SYNTAX` の操作語彙はコンパイラ拡張向けの低水準 API であり、このクイックリファレンスでは網羅しない。新しい構文を書く場合は `crates/tbx-next/stdlib/basic.tbx`、`crates/tbx-next/src/source_word*.rs` と関連テストを確認すること。

## 現行 TBX と混同しやすい点

TBX Next は現行 `tbx` の互換実装ではない。特に次をそのまま持ち込まないこと。

| 項目 | TBX Next の現在の形 |
| --- | --- |
| ワード定義 | `DEF NAME local1, local2 ... END` |
| 戻り値 | ワードがデータスタックへ残す値。`RETURN` は値指定ではなく、ワードからの早期復帰 |
| 式をスタックへ積む | `EVAL expression` |
| 値 | 現在は `i16` 整数のみ |
| 真偽 | `0` が偽、0 以外が真 |
| 変数 | 長寿命の状態は `VAR` で宣言するグローバル変数。定義ワード内では `I..N,X,Y` が呼出しごとのローカルスクラッチ |
| 配列 | `DIM @NAME[n]` で宣言し、`@NAME[index]` で要素を読む。`@NAME` 単体は値ではない |
| 文字列 | 一般的な実行時値ではなく、現在は主に `PRINT` / `USE` などのソース構文で使う |

## 代表的なサンプル

- `docs/next/examples/demo.tbx` — 最小実行例
- `docs/next/examples/stack_function.tbx` — スタックを使うワード
- `docs/next/examples/prime.tbx` — 局所参照名、条件分岐、反復
- `docs/next/examples/guess.tbx` — `RND`, `INPUT?`, `PRINT`
- `docs/next/examples/mandelbrot.tbx` — 整数演算、反復、`ABS`, `PUTCHR`
- `docs/next/examples/squares.tbx` — グローバル配列への保存と読み出し
- `docs/next/examples/grades.tbx` — `FOR` で配列を走査し、`SELECT` で成績区分を判定する統合例
- `docs/next/examples/sttr1/main.tbx` — 銀河探索、戦闘、補給、装置損傷、勝敗を含む中規模の対話サンプル
- `docs/next/examples/eightqueen.tbx` — 1次元配列と明示状態による非再帰の8クィーン全解探索
- `docs/next/examples/maze.tbx` — 1次元配列を明示スタックとして使う非再帰DFS、backtrack、探索済み領域と最終経路のASCII表示

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
