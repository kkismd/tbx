# TBX クイックリファレンス

この文書は、TBX プログラムを書く人間およびエージェント向けの実用メモである。

TBX は開発中であり、実装が正である。この文書は完全な仕様書ではなく、よく使う構文・標準語彙・書き方の入口を示す。詳細な仕様、境界条件、エラー型は `src/`、`lib/`、テストを確認すること。

## この文書の位置づけ

- 正の実装定義: `src/`, `lib/`, テスト
- 設計意図: `blueprint.md`, `blueprint-language.md`, `blueprint-compiler.md`
- 実用入口: この文書

この文書は、既存の blueprint 系ドキュメントを置き換えない。実装の変化に追従しやすいように、代表的な構文、標準語彙、エージェントが間違えやすい点を中心にまとめる。

## 基本モデル

TBX は Tiny BASIC 風の表面構文を持つが、内部では Forth 的な辞書、実行トークン、スタックモデルの上で動く。

- プログラムはステートメント列として読まれる
- ワードは `DEF ... END` で定義する
- ステートメントとして呼ぶワードと、式内で関数のように呼ぶワードは同じ辞書上のワードである
- コアは小さく保ち、高水準構文は可能な限り TBX 側の IMMEDIATE ワードや標準ライブラリで組み立てる

## 字句・文終端

### 大文字小文字

ワード名・変数名は大文字小文字を区別しない。文字列リテラルの中身は区別される。

```tbx
PRINTLN "hello"
println "hello"  # 同じワード名として扱われる
```

### コメント

```tbx
# 行末コメント
PRINTLN "hello"  # ここもコメント

REM この物理行の残りはコメント
```

- `#` は行末コメント
- `REM` はステートメントコメント

### 文終端

論理ステートメントの終端は、原則として改行またはセミコロンである。

```tbx
PRINTLN "one"
PRINTLN "two"; PRINTLN "three"
```

括弧が開いている間は、改行を空白のように扱い、式を次の物理行へ継続できる。

```tbx
PRINTLN STR_CONCAT(
  "hello, ",
  "world"
)
```

括弧内のセミコロンは禁止される。

## 値

主な値は次の通り。

- 整数: `1`, `42`, `-3`
- 浮動小数点数: `1.5`, `8.0`, `1.0e3`
- 真偽値: `TRUE`, `FALSE`。比較や論理演算も `Bool` を返す
- 文字列: `"hello"`
- タプル: `TUPLE(...)` が返す immutable な値
- 配列: `DIM @A[n]` で作る名前付き mutable storage
- アドレス: `&X`, `&@A[i]` などが返す書き込み先

配列ハンドルそのものは surface の第一級値として扱わない。ユーザーコードでは `DIM @A[n]`, `@A[i]`, `&@A[i]`, `ARRAY_LEN(@A)` を使う。

## 呼び出し

### ステートメント呼び出し

ワードをステートメントとして実行する場合は、カッコなしで書く。

```tbx
PRINTLN "hello"
CR
```

引数がないステートメントもカッコなしで書く。

```tbx
DEF SAY_HELLO()
  PRINTLN "hello"
END

SAY_HELLO
```

ステートメント文脈で `SAY_HELLO()` のように書かない。

### 式内呼び出し

値を返すワードを式の一部として使う場合は、`NAME(args...)` 形式で書く。

```tbx
DEF ADD1(X)
  RETURN X + 1
END

PRINTLN ADD1(41)
PRINTLN STR_LEN("hello")
```

引数なしで値を返すワードは、式内では `NAME()` と書ける。

```tbx
PRINTLN UNIXTIME()
```

### 重要な使い分け

```tbx
CR          # ステートメント呼び出し
UNIXTIME()  # 式内呼び出し
```

同じワードでも、ステートメント文脈と式文脈で書き方が異なる。

## 変数と代入

### グローバル変数

トップレベルの `VAR` はグローバル変数を宣言する。トップレベルでは `VAR X = expr` は使わない。

```tbx
VAR X
SET &X, 10
PRINTLN X
```

### ローカル変数

`DEF ... END` の中の `VAR` はローカル変数を宣言する。

```tbx
DEF DOUBLE(A)
  VAR X
  SET &X, A * 2
  RETURN X
END
```

ローカル変数では初期化付き `VAR X = expr` が使える。

```tbx
DEF DOUBLE(A)
  VAR X = A * 2
  RETURN X
END
```

### 代入

代入の基本形は `SET &X, expr` である。

```tbx
VAR X
SET &X, 1
SET &X, X + 1
```

`SET` は「変数専用代入」ではなく、「アドレスへ値を書き込む」操作である。そのため左辺には `&X` のようにアドレスを明示する。

標準ライブラリには糖衣構文として `LET` がある。`LET` は IMMEDIATE ワードであり、`DEF ... END` の内部でのみ使える。

```tbx
DEF COUNT_UP(N)
  VAR I = 1
  WHILE I <= N
    PRINTLN I
    LET I = I + 1
  ENDWH
END
```

`LET X = expr` は `SET &X, expr` に近い書き方である。配列要素にも使える（DEF 内のみ）。

```tbx
DEF FILL_ARRAY()
  DIM @A[3]
  LET @A[1] = 10
  PRINTLN @A[1]
END
```

トップレベルで配列要素に書き込む場合は `SET &@A[i], expr` を使う。

```tbx
DIM @A[3]
SET &@A[1], 10
PRINTLN @A[1]
```

## ワード定義

### 基本形

```tbx
DEF ADD(A, B)
  RETURN A + B
END

PRINTLN ADD(20, 22)
```

### 値を返さないワード

```tbx
DEF BANNER()
  PRINTLN "===="
END

BANNER
```

`RETURN` だけを書くと void return になる。

```tbx
DEF CHECK_POSITIVE(X)
  IF X <= 0
    PRINTLN "not positive"
    RETURN
  ENDIF
  PRINTLN "positive"
END
```

### 可変長引数

`DEF WORD(...)` または `DEF WORD(X, ...)` で可変長引数ワードを定義できる。

可変長引数を扱う主な語彙は次の通り。

- `VA_COUNT()` — 現在の呼び出しで渡された実引数の総数を返す
- `ARG_ADDR(I)` — 0-based の引数番号から、その引数のアドレスを返す
- `FETCH(ARG_ADDR(I))` — I 番目の引数値を読む

例:

```tbx
DEF PRINT_ALL(...)
  VAR I = 0
  VAR N = VA_COUNT()
  WHILE I < N
    PRINTLN FETCH(ARG_ADDR(I))
    LET I = I + 1
  ENDWH
END

PRINT_ALL "a", "b", "c"
```

## 制御構造

制御構造の多くは `lib/basic.tbx` の IMMEDIATE ワードとして定義されている。`IF`, `WHILE`, `DO`, `SELECT`, `FOR` はいずれも `DEF ... END` の内部で使う。

### IF / ELSIF / ELSE / ENDIF

```tbx
DEF SIGN(X)
  IF X < 0
    PRINTLN "negative"
  ELSIF X = 0
    PRINTLN "zero"
  ELSE
    PRINTLN "positive"
  ENDIF
END
```

### WHILE / ENDWH

```tbx
DEF COUNT_TO(N)
  VAR I
  SET &I, 1
  WHILE I <= N
    PRINTLN I
    SET &I, I + 1
  ENDWH
END
```

### DO / UNTIL

```tbx
DEF COUNT_THREE()
  VAR I
  SET &I, 0
  DO
    SET &I, I + 1
    PRINTLN I
  UNTIL I >= 3
END
```

### SELECT / CASE / CASE_ELSE / ENDSEL

```tbx
DEF DESCRIBE(X)
  SELECT X
  CASE 1
    PRINTLN "one"
  CASE 2
    PRINTLN "two"
  CASE_ELSE
    PRINTLN "other"
  ENDSEL
END
```

### FOR / NEXT

```tbx
DEF PRINT_FIVE()
  VAR I
  FOR &I, 5
    PRINTLN I
  NEXT
END
```

現在の `FOR` は開始値 1、ステップ 1 固定である。0-based range、任意の開始値、任意ステップが必要な場合は `WHILE` を使う。

## 行番号とジャンプ

行番号はすべての行に付けるものではなく、ジャンプ先が必要な箇所だけに置くローカルラベルである。

```tbx
DEF LOOP_EXAMPLE(N)
  VAR I
  SET &I, 0
100
  LET I = I + 1
  PRINTLN I
  BIF I < N, 100
END
```

`GOTO` / `BIF` / `BIT` のジャンプ先は同じワード本体内のローカル行番号に限定する。ワード境界をまたぐジャンプを書かない。

通常の制御には `IF`, `WHILE`, `DO`, `FOR`, `SELECT` を優先する。

## 配列

配列は `DIM @A[n]` で宣言する。添字は 1-based である。

```tbx
DIM @A[3]
SET &@A[1], 10
SET &@A[2], 20
SET &@A[3], @A[1] + @A[2]
PRINTLN @A[3]
```

配列長は `ARRAY_LEN(@A)` で得る。

```tbx
PRINTLN ARRAY_LEN(@A)
```

配列要素のアドレスは `&@A[i]` で表せる。

```tbx
SET &@A[1], 42
```

配列ハンドルそのものを変数に代入したり、戻り値にしたり、比較したりしない。

避ける例:

```tbx
# 避ける: 配列ハンドルそのものを値として扱おうとしている
SET &B, A
RETURN A
PRINTLN A
```

## タプルと Result 風ヘルパ

### タプル

`TUPLE(...)` は複数の値を immutable な値としてまとめる。

```tbx
VAR P
SET &P, TUPLE(10, 20)
PRINTLN P[1]
PRINTLN P[2]
PRINTLN TUPLE_LEN(P)
```

タプル添字も 1-based である。

### Result 風ヘルパ

`lib/result.tbx` には `TUPLE(value, ok)` 形式を扱うヘルパがある。

- `RESULT_VAL(R)` — 値を取り出す
- `RESULT_OK(R)` — 成功フラグを取り出す
- `RESULT_OR(R, DEFAULT)` — 成功なら値、失敗ならデフォルト値
- `RESULT_EXPECT(R, MSG)` — 成功なら値、失敗ならメッセージ付きで失敗
- `RESULT_OK_OF(V)` — 成功 result を作る
- `RESULT_ERR_OF(V)` — 失敗 result を作る

`GETDEC?` は recoverable な入力 API として `TUPLE(n, ok)` を返す。

```tbx
DEF READ_NUMBER()
  VAR R
  SET &R, GETDEC?()
  IF RESULT_OK(R)
    PRINTLN RESULT_VAL(R)
  ELSE
    PRINTLN "invalid input"
  ENDIF
END
```

## 入出力

### 出力

よく使うもの:

- `PRINTLN(...)` — 複数値を出力して改行
- `PRINT(...)` — 複数値を出力、改行なし
- `CR` — 改行
- `PUTVAL` — 値を出力
- `PUTSTR` — 文字列を出力
- `PUTDEC` — 数値を10進で出力
- `PUTHEX` — 整数を16進で出力
- `PUTCHR` — ASCII コードを1文字として出力

```tbx
PRINTLN "score=", 100
PRINT "loading"
CR
PUTCHR 10
```

### 入力

- `GETDEC()` — 1行読んで数値として返す。失敗時はエラー
- `GETDEC?()` — 1行読んで `TUPLE(n, ok)` を返す
- `GETSTR()` — 1行読んで文字列として返す

```tbx
DEF READ_INPUT()
  PRINT "number? "
  VAR R
  SET &R, GETDEC?()
  IF RESULT_OK(R)
    PRINTLN "got ", RESULT_VAL(R)
  ELSE
    PRINTLN "not a number"
  ENDIF
END
```

## 文字列

主な文字列語彙:

- `STR(V)` — 値を文字列に変換する
- `STR_CONCAT(A, B)` — 文字列連結
- `STR_LEN(S)` — 文字数
- `STR_EQ(A, B)` — 文字列比較
- `STR_INDEXOF(S, NEEDLE)` — 1-based 位置。見つからなければ 0
- `STR_SLICE(S, START, LEN)` — 部分文字列。`START` は 1-based、負数は末尾から
- `STR_TRIM(S)` — 前後の空白を除去
- `STR_UPPER(S)` — 大文字化
- `STR_LOWER(S)` — 小文字化
- `STR_REPLACE_FIRST(S, NEEDLE, REPLACEMENT)` — 最初の一致を置換
- `STR_REPLACE_ALL(S, NEEDLE, REPLACEMENT)` — すべて置換

```tbx
VAR S
SET &S, STR_CONCAT("hello, ", "world")
PRINTLN STR_LEN(S)
PRINTLN STR_UPPER(S)
PRINTLN STR_INDEXOF(S, "world")
```

## 数値・比較・論理

式では演算子を書ける。

```tbx
PRINTLN 1 + 2 * 3
PRINTLN X <= 10
PRINTLN (A = B) || (A = C)
```

内部的な primitive 名も存在する。IMMEDIATE ワードや低レベルコードを書く場合に使う。

### 数値

- `ADD`, `SUB`, `MUL`, `DIV`, `MOD`
- `SQRT`
- `NEGATE`
- `INT`

### 比較

- `EQ`, `NEQ`
- `LT`, `GT`, `LE`, `GE`

### 論理・ビット演算

- `AND`, `OR`
- `BAND`, `BOR`

主な演算子:

- 算術: `+`, `-`, `*`, `/`, `%`
- 比較: `=`, `<>`, `<`, `>`, `<=`, `>=`
- 論理: `&&`, `||`
- ビット演算: `&`, `|`

`&&` / `||` は short-circuit 評価する。`A && B` は `A` が falsy なら
`B` を評価せず、`A || B` は `A` が truthy なら `B` を評価しない。
どちらも operand 自体ではなく `Bool` を返す。範囲チェック後の配列アクセスなど、
右辺を条件付きでのみ安全に評価できる場合の guard として使える。

## 乱数・時刻

- `RND(N)` — `1..N` の整数乱数
- `RANDOMIZE` — 乱数生成器を OS entropy で再シード
- `UNIXTIME()` — Unix timestamp を Float で返す
- `HOUR(T)`, `MINUTE(T)`, `SECOND(T)` — UTC の時分秒を取り出す
- `HMS(T)` — JST の `(hour, minute, second)` タプルを返す標準ライブラリ関数

```tbx
RANDOMIZE
PRINTLN RND(6)

VAR T
SET &T, UNIXTIME()
PRINTLN HMS(T)
```

## USE

`USE` は別の TBX ソースを読み込む compile-time directive である。import というより include に近い。

```tbx
USE "lib/foo.tbx"
```

- 引数は文字列リテラル1個だけ
- `DEF ... END` の内部では使わない
- 同じファイルを複数回読めば、そのたびに効果が発生しうる

## 標準語彙の一覧

実装が正であり、この一覧は代表的な入口である。完全性が必要な場合は `src/primitives.rs` の `register_all()` と `lib/*.tbx` を確認する。

### スタック・メモリ

- `DROP`, `DUP`, `SWAP`
- `FETCH`, `STORE`, `SET`

### 出力・入力

- `PUTSTR`, `PUTCHR`, `PUTDEC`, `PUTHEX`, `PUTVAL`
- `PRINT`, `PRINTLN`, `CR`
- `GETDEC`, `GETDEC?`, `GETSTR`
- `GET_OUTPUT`（主にテスト用）

### 数値・比較・論理

- `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `SQRT`, `NEGATE`, `INT`
- `EQ`, `NEQ`, `LT`, `GT`, `LE`, `GE`
- `AND`, `OR`, `BAND`, `BOR`

### 文字列

- `STR`, `STR_CONCAT`, `STR_LEN`, `STR_EQ`
- `STR_INDEXOF`, `STR_SLICE`, `STR_TRIM`
- `STR_UPPER`, `STR_LOWER`
- `STR_REPLACE_FIRST`, `STR_REPLACE_ALL`

### 配列・タプル

- `DIM @A[n]`
- `@A[i]`, `&@A[i]`, `ARRAY_LEN(@A)`
- `TUPLE`, `TUPLE_LEN`
- `R[1]` のようなタプル projection

### 制御構造

- `IF`, `ELSIF`, `ELSE`, `ENDIF`
- `WHILE`, `ENDWH`
- `DO`, `UNTIL`
- `SELECT`, `CASE`, `CASE_ELSE`, `ENDSEL`
- `FOR`, `NEXT`
- `GOTO`, `BIF`, `BIT`

### ワード定義・コンパイル時語彙

- `DEF`, `END`, `RETURN`
- `VAR`, `DIM`
- `IMMEDIATE`, `HEADER`, `LITERAL`
- `APPEND`, `ALLOT`, `HERE`, `STATE`, `LOOKUP`
- `CS_PUSH`, `CS_POP`, `CS_SWAP`, `CS_DROP`, `CS_DUP`, `CS_OVER`, `CS_ROT`
- `CS_OPEN_TAG`, `CS_CLOSE_TAG`, `PATCH_ADDR`, `COMPILE_EXPR`, `COMPILE_LVALUE`, `SKIP_EQ`, `SKIP_COMMA`, `COMPILE_LVALUE_SAVE`

通常の TBX プログラムを書く場合、コンパイル時語彙を直接使う必要は少ない。IMMEDIATE ワードや新しい構文糖衣を書くときに確認する。

## エージェント向け注意事項

- TBX は Python / JavaScript / Rust ではない。似た名前の標準関数を推測して使わない。
- ステートメント呼び出しでは `NAME()` と書かず、`NAME` と書く。
- 値を返すワードを式内で使うときだけ `NAME(args...)` と書く。
- トップレベルでは `VAR X = expr` を使わず、`VAR X` の後に `SET &X, expr` を使う。
- 代入は `SET &X, expr` を使う。`LET X = expr` は `DEF ... END` の内部でのみ使える。
- `IF`, `WHILE`, `DO`, `SELECT`, `FOR` などの制御構造は IMMEDIATE ワードであり、`DEF ... END` の内部でのみ使える。トップレベルでは動かない。
- `FOR` の変数参照には `&` が必要: `FOR &I, 5`。
- 配列要素の添字は 1-based。`@A[0]` は通常使わない。
- `FOR` は 1 始まり・ステップ 1 固定。必要なら `WHILE` を使う。
- 文字列、タプル、Result 風ヘルパ、配列、乱数、時刻関数がある。自前で再実装する前にこの文書と `src/primitives.rs` / `lib/*.tbx` を確認する。
- 未確認の構文や標準語彙を発明しない。

## 実装を確認する場所

- `src/lexer.rs` — トークン、コメント、文字列リテラル、演算子
- `src/statement_reader.rs` — 論理ステートメント、改行、セミコロン、行番号ラベル
- `src/expr.rs` — 式、演算子、関数呼び出し、配列・タプル projection
- `src/primitives.rs` / `src/primitives/` — primitive の登録と実装
- `lib/basic.tbx` — 制御構造、`LET`, `FOR`, `PRINT`, `PRINTLN`, `CR`, `HMS`
- `lib/result.tbx` — Result 風ヘルパ
- `blueprint-language.md` — 言語仕様上の設計意図
- `blueprint-compiler.md` — コンパイル時語彙と IMMEDIATE ワードの設計意図
