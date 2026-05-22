# TBX コア言語仕様

> このファイルはTBX設計書の一部です。`blueprint.md`（VM・辞書のアーキテクチャ）と合わせて参照してください。

## この文書の役割

この文書は、TBX のコア言語について **ソースコードだけでは読み取りにくい設計意図・規約・制約** を残すためのものとする。

- ワード一覧、内部データ構造、個々の primitive の詳細な入出力、具体的なエラー型などの **実装事実** はソースコードを正とする
- この文書では、実装を読んでも「なぜその形なのか」が自明でない事項を優先して記述する
- 低レベルなコンパイル手順やコンパイルスタック操作の詳細は `blueprint-compiler.md` に委譲する

## 文法と評価モデル

> Issue #404「大文字小文字を区別しないようにする」に基づく設計方針

### 大文字小文字

ワード名・変数名の大文字小文字は区別しない。文字列リテラルの内容は対象外。

### ステートメント形

TBX は BASIC 風の表面構文を持つが、コアでは次のように単純化する。

- 1 文は `ステートメント + 引数式 + 文終端` という形を基本とする
- 文終端は改行またはセミコロンとする
- 引数の並びは専用の引数リスト構文ではなく、低優先度のカンマ演算子で表現する

この方針により、呼び出し側は複数値を左から順に積むだけでよく、各ステートメントは必要な個数だけ取り出せばよい。

### 物理行と論理ステートメント

> Issue #521「論理ステートメント仕様を blueprint-language.md に反映する」に基づく設計方針

TBX のソースには **物理行**（改行で区切られたテキスト行）と **論理ステートメント**（実行の単位）という二つのレベルがある。

通常、物理行 1 行が論理ステートメント 1 つに対応する。例外は括弧内の継続である。

#### 文終端の規則

- 論理ステートメントの終端は、原則として **改行またはセミコロン** とする
- セミコロンで複数のステートメントを同一物理行に並べることができる
- 括弧が開いている間は、改行を **空白と等価** として扱い、論理ステートメントを次の物理行へ継続する

#### 括弧内の制約

括弧内の継続中は次の制約が適用される。

- **セミコロンは禁止** とする（未閉じ括弧内でのステートメント分割は意味が曖昧になるため）
- 括弧が閉じられないまま EOF に達した場合はエラーとする
- `)`が括弧の対応なしに現れた場合もエラーとする

#### コメントの扱い

- `#` コメントは括弧内でも **行末コメント** として扱い、その物理行の末尾まで読み飛ばした後、次の物理行で式を継続できる
- `REM` は **ステートメントコメント** として扱う。`REM` が現れると、残りのトークンを含む物理行全体が 1 つの論理ステートメントになり、括弧内コメントとしては機能しない

#### 行番号の扱い

- 行番号（`100`, `200` などの整数ラベル）は、**論理ステートメント先頭** にあるときだけジャンプラベルとして解釈する
- **括弧内継続行の行頭に現れた数値** は行番号ではなく **整数リテラル** として解釈する
- 浮動小数点数（`1.5` など）も同様に、括弧内継続行の行頭では数値リテラルとして回復される

#### exec_line / REPL の扱い

`exec_line()` および REPL の継続入力（未閉じ括弧時のプロンプト継続）は、上記の `StatementReader` ベースの規則とは独立した設計とする。これらは別途定義する。

## 制御構造

### 行番号

旧来の BASIC のように全行へ行番号を振る方式は採らない。行番号はジャンプ先が必要な箇所にだけ書くローカルなラベルとみなす。

### ワード定義

GOSUB/RETURN 形式のサブルーチンは採らず、手続きや関数は `DEF` によるワード定義で表す。文は値を返すことができ、式の一部にもなれる。

### ジャンプのスコープ制約

> Issue #116「ワード本体の終端判定機構（EXIT規約）が仕様に明記されていない」に基づく設計方針

`GOTO` / `BIF` / `BIT` のジャンプ先は、そのワード本体内のローカル行番号に限定する。ワード境界をまたぐジャンプは禁止する。

この制約の目的は以下。

- ワードの終端規約を単純に保つ
- 制御フロー解析をワード単位に閉じる
- コンパイル時のラベル解決とエラー報告を局所化する

## 値と参照

### 値の分類

> Issue #10「Cell型の定義が『値の種類』テーブルとRustコード例で不一致」に基づく設計方針
> Issue #93「アドレス空間の統一: dictionary と data_stack が別々で Cell::Addr が曖昧」に基づく設計方針

TBX は数値・真偽値・文字列・配列・実行トークン・アドレスを同一の値空間で扱う。

ここで重要なのは **参照先の違いを値の型で区別する** ことである。

- グローバル領域を指す参照
- 現在フレームのローカル領域を指す参照
- 配列要素を指す参照

この区別により、`FETCH` / `STORE` 系の意味論を一貫させつつ、ワード境界をまたぐ安全性も説明しやすくなる。

### アドレスの非対称性

ローカル参照は現在のフレームに束縛されるため、ワード境界を越える汎用的な参照としては扱えない。グローバル参照のみが安定した共有先になる。

この非対称性は意図的なものであり、参照渡しに見える書き方を許しつつ、局所変数の寿命を単純に保つための制約である。

## コアと標準ライブラリの境界

TBX では、構文的に便利な高水準機能をできるだけ標準ライブラリ側へ寄せる。

- コアは小さく、直交的で、自己拡張しやすい primitive 群に保つ
- 構造化制御構文や糖衣構文は、可能ならライブラリの compile-time word として実装する

この方針により、言語機能の追加はまずライブラリで試せる。コアへ入れるのは、ライブラリ実装の土台として必要なものに絞る。

## `USE` の位置付け

`USE` は単なるファイル読み込み構文ではなく、**ソース処理中に別のソースを実行へ組み込む仕組み** として扱う。

このため、次の性質を持つ。

- import ではなく include に近い
- 同じファイルを複数回読めば、そのたびに効果が発生しうる
- 呼び出し位置と読み込み元の文脈が意味を持つ

また、アクティブなワード定義の途中で `USE` を許すと、外側のコンパイル状態と衝突しやすい。そのため `DEF ... END` の内部では使えないものとする。

## 即時実行ワード

`IMMEDIATE` は Forth 風の「直前定義への暗黙作用」ではなく、対象ワードを明示して属性を付与する方式を採る。

この方針の意図は以下。

- 定義位置に依存しない
- 「どのワードに即時性を与えるか」をソース上で明示できる
- アウターインタプリタの規則を局所的に理解しやすくする

## 式評価

### ステートメントと式の関係

ステートメントとして呼ぶワードと、式の中で関数のように呼ぶワードは、同じ辞書上のワードである。TBX では「文用」と「式用」で別の実体を持たせない。

### ステートメント呼び出しと式内呼び出しの構文

> Issue #544「ステートメント呼び出しと式内呼び出しの構文差を blueprint-language.md に明記する」に基づく設計方針

同じワードでも、呼び出す文脈によって構文が異なる。

#### ステートメント呼び出し

ワードをステートメントとして実行する場合は、**カッコなし**で記述する。

```tbx
DEF SETG()
  SET &G, "inside"
END

SETG
```

`SETG` のようにカッコなしで記述するのが正式な statement call の形式である。

#### 式内呼び出し

値を返すワードをオペランド式の中で使う場合は、`NAME(args...)` 形式の関数呼び出し構文を使う。

```tbx
LET X = ADD1(41)
PUTDEC STR_LEN("hello")
```

#### ステートメント文脈での `NAME()` 形式は非正式

ステートメント文脈で `SETG()` のようにカッコ付きで書ける場合があるが、これは正式な仕様ではない。偶然許容されている可能性があり、将来エラーとして扱う可能性を排除しない。ステートメント呼び出しには必ずカッコなし形式を使うこと。

### ステートメント境界のスタッククリア

> Issue #151「ステートメントの実行後にスタックをクリアする仕様」に基づく設計方針

各ステートメントの実行後、途中計算の残骸がデータスタックに残らないことを保証する。

このため、TBX は「文ごとにスタックを巻き戻す」という規律を持つ。これは次のトレードオフに基づく。

- 速度やメモリ効率より、意味論の単純さを優先する
- 文が値を返せる設計と、トップレベル実行の安全性を両立させる
- 「ある文の副作用が次の文のスタック状態を暗黙に汚す」事態を防ぐ

### 式内呼び出しの arity

> Issue #87「操車場アルゴリズムでの関数適用について」に基づく設計方針
> Issue #194「blueprint-language.md：arity確定方式の記述が矛盾している」に基づく設計方針
> Issue #121「IMMEDIATEワード実行が arity 計算を破壊する可能性」に基づく設計方針

式中のワード呼び出しの arity は、実行時のスタック差分ではなく、コンパイル時に確定する。

この設計の意図は以下。

- 呼び出し規約を式コンパイラの責務として閉じる
- 実行時の暗黙状態に依存しない
- 可読性よりも、規則の一貫性とエラー局所化を優先する

この前提のため、arity 追跡中の文脈では IMMEDIATE word を自由に差し込めない。

### 演算子

演算子の完全な一覧や優先順位は、実装とずれやすいためこの文書では列挙しない。ここで重要なのは次の規約だけである。

- カンマは複数値を積むための最低優先度の結合子として振る舞う
- `-` や `&` のように単項・二項の両用記号がある
- 単項・二項の解釈は、直前トークン種別に依存する

## 変数と代入

> Issue #117「ローカル変数のメモリレイアウトと仮引数構文が未定義」に基づく設計方針
> Issue #120「LET &A, expr 構文がBASIC慣習から乖離している」に基づく設計方針
> Issue #434「VAR 文でカンマ区切り複数変数の一括宣言を可能にする」に基づく設計方針
> Issue #636「VAR 宣言仕様を docs に反映する」に基づく設計方針

### グローバルとローカル

グローバル変数とローカル変数は見た目の名前空間を共有していても、意味論上は別物である。

- グローバル変数は長寿命の共有先
- ローカル変数は呼び出しフレームに束縛された一時領域

局所名はコンパイル後に消え、実行時にはフレーム内の位置だけが意味を持つ。

### `SET &A` を採る理由

`SET` は「変数専用代入」ではなく、**アドレスへ値を書き込む一般操作** として位置付ける。

そのため左辺は変数名ではなく、明示的なアドレス式 `&A` の形を採る。

この設計の意図は以下。

- 左辺が値なのか参照なのかを構文上で曖昧にしない
- 代入構文を変数専用にせず、他の書き込み先にも拡張可能にする
- BASIC 風の見た目の上に、Forth 的な低レベル操作を一貫した形で載せる

### 仮引数

仮引数は呼び出し側の関数呼び出し構文と対称な記法を採る。これは利用者の認知負荷を下げるためであり、内部表現の都合ではない。

### 可変長引数

> Issue #429「可変長引数（variadic arguments）のサポート」に基づく設計方針

可変長引数は「固定引数列 + 余剰引数列」という形で扱う。可変部分は独立したコレクションではなく、通常の呼び出し規約の延長として解釈する。

この方式は次を優先する。

- 固定長ワードとの呼び出し規約の近さ
- 実装の単純さ
- 可変部分を特別な値型にしないこと

### `VAR` 宣言

`VAR` はローカル変数またはグローバル変数を宣言するためのステートメントである。

#### 有効な形式

```tbx
VAR A          # Declare a single variable (top-level or inside DEF)
VAR A, B, C   # Declare multiple variables (top-level or inside DEF)
VAR A = expr   # Declare and initialize (inside DEF only)
```

#### 文脈による制約

**DEF 内（ローカル宣言）**では次の 2 形式が使える。

- `VAR name` / `VAR name, name, ...` — 宣言のみ。命令列を生成しない。
- `VAR name = expr` — 宣言と初期化。`LIT StackAddr(name) <expr> SET` に相当する命令列を emit する。

**トップレベル（グローバル宣言）**では宣言のみの形式だけが使える。

- `VAR name` / `VAR name, name, ...` — グローバル変数を登録する。
- `VAR name = expr` — **エラー**（`InvalidExpression`）。トップレベルでの初期化構文は許可しない。

この制約は、グローバル変数の初期化を明示的な `SET &G, expr` として書かせるためである。初期化タイミングが曖昧になる構文をコア言語へ取り込まない、という設計方針と対応している。

#### DEF 内の重複宣言

DEF 本体内で同じ名前のローカル変数を複数回宣言することはエラーとする。`VAR X` の後に再度 `VAR X` または `VAR X = expr` を書くと `InvalidExpression` を返す。

## 配列

> Issue #427「ローカル配列（ARRAY プリミティブ）の実装」に基づく設計方針
> Issue #454「トップレベルで作成した配列をグローバル変数経由で共有できるようにする」に基づく設計方針
> Issue #487「配列要素を数値型のみに制限する」に基づく設計方針
> Issue #684 decision に基づく設計方針
> Issue #720「array surface policy を docs / blueprint に反映する」に基づく設計方針

### 配列と tuple の役割分担

```text
Tuple = immutable aggregate value
Array = named mutable storage
```

**配列** は named mutable storage として位置付ける。複数の値を状態として記録・更新する用途に使う。

**タプル** は immutable aggregate value として位置付ける。複数の値をまとめて値として受け渡す用途に使う。`TUPLE(...)` で生成し、`T[i]` で要素を取り出す。

配列の生成手段は `DIM @A[n]` のみとする。`@A[i]` / `&@A[i]` / `LET @A[i] = expr` による操作は、`DIM @A[n]` で宣言した array binding に対して行う。

### 配列の surface policy

配列は surface language 上では first-class value ではない。`@A` は array value ではなく **array storage designator** である。

#### 正規の surface 操作

以下の操作のみをサポートする。

```tbx
DIM @A[n]        # Declare array binding of size n
@A[i]            # Read element i (1-based) of array A
&@A[i]           # Address of element i (for SET)
LET @A[i] = expr # Element assignment (sugar for SET &@A[i], expr)
SET &@A[i], expr # Element write via address
ARRAY_LEN(@A)    # Number of elements in array A
```

#### サポートしない操作

以下の操作はサポートしない。配列を first-class value として扱う操作はすべて対象外である。

```tbx
ARRAY_LEN(A)  # Unsupported: A without @ is not a valid array designator
LET B = A     # Unsupported: whole-array assignment
SET &B, A     # Unsupported: whole-array write
RETURN A      # Unsupported: whole-array return value
TUPLE(A)      # Unsupported: array as tuple element
PUTVAL A      # Unsupported: array display
A = B         # Unsupported: array equality as expression
EQ(A, B)      # Unsupported: array equality comparison
NEQ(A, B)     # Unsupported: array inequality comparison
SHUFFLE A     # Unsupported: aggregate-value transformation on array storage
SHUFFLE @A    # Unsupported: aggregate-value transformation on array storage
```

これらを許容すると `Array = first-class shared mutable reference value` の意味論に寄ってしまうため、明示的に禁止する。

`SHUFFLE` のような aggregate-value transformation は array storage operation ではないため、array surface API からは外す。必要なら tuple operation として別途設計する。

#### `Cell::Array` は内部表現である

`Cell::Array(usize)` / `Cell::Array(ArrayRef)` は VM 内部の実装表現であり、**surface language 上の first-class array value を意味しない**。

内部では配列ハンドルが `Cell` として stack / dictionary を流れることがあるが、これは実装の都合であり、surface 上の操作として露出することはない。surface コードが `Cell::Array` を直接生成・操作・受け渡しする経路は提供しない。

### 1-origin

配列添字は 1-origin とする。これは BASIC 系言語としての自然さを優先した選択である。

### ライフタイム

配列には次の 2 種類がある。

- 現在のワードに束縛された短寿命の配列
- トップレベル由来で共有可能な長寿命の配列

短寿命の配列をそのままグローバルへ逃がさないのは、安全性と解放規則を単純に保つためである。一方で、戻り値として所有権を移す経路は必要なので、返却は許可する。

### 要素型制限

配列要素をスカラー型中心に制限するのは、ネストした所有権や寿命の問題を増やさないためである。

TBX の配列は汎用オブジェクトグラフを作るための仕組みではなく、数値計算や単純な集約を扱うための軽量な容器として位置付ける。

#### `Str` の配列要素格納（Phase 5B-D4 以降）

> Issue #591（Phase 5B-D4: Rc-backed string liberation）に基づく設計方針

Phase 5B-D4 以降、`Cell::Str` は `Rc<str>` backed immutable string handle として実装されている（issue #588）。`Rc<str>` は参照カウントで寿命を管理するため、配列要素への `Str` 格納にコールフレームのライフタイム追跡は不要である。

- **`Cell::Str` はすべての配列（frame-local・global・caller-owned）の要素として格納できる**。`Rc` handle が配列要素として clone されるだけであり、dangling reference は発生しない。
- **nested Array**（`Cell::Array`）は引き続き無条件で拒否する。

拒否された場合のエラー：

- `Cell::Array` を配列要素に格納しようとすると `TbxError::InvalidArrayElement { got: "Array" }`

> Phase 5A での `PoolRefLifetime` ライフタイムマトリクス（`StringFrameEscape` / `FrameLocal` / `CallerOwned` / `Global` 区分）は Phase 5B-D4 で廃止した。`VM::strings` string pool・`saved_string_pool_len`・`global_string_pool_len` も同時に削除した（issue #590）。

### 記号の役割分担

> Issue #687「`()` / `[]` / `@` / `&` の役割分担を docs / blueprint に明文化する」に基づく設計方針

TBX では以下の記号を次の目的に割り当てる。

| 記号 | 役割 |
| --- | --- |
| `()` | grouping / function call |
| `[]` | element selection / projection |
| `@` | array binding / array storage sigil |
| `&` | address / lvalue |

それぞれの使い方の例を示す。

```tbx
F(1)        # function call
(X + 1)     # grouping
T[1]        # tuple projection
@A[1]       # array element value
&@A[1]      # array element address / lvalue
SET &@A[1], 10
LET @A[1] = 10
```

#### 設計意図

- `()` は grouping と function call に寄せる。
- `[]` は tuple projection / array indexing など、要素選択に寄せる。`[]` 自体は value / address を決めない。`@A[i]` は配列要素の value selection、`&@A[i]` は同じ selection に `&` を付けた address / lvalue access である。
- `@` は array binding / array storage を表す surface sigil とする。`@` なしの変数名はスカラーバインディングを表し、`@` 付きは配列バインディングを表す。
- `&` は address / lvalue を表す。`&A` はスカラー変数 `A` のアドレス、`&@A[i]` は配列要素のアドレスを表す。

旧 `A(i)` / `&A(i)` 配列アクセス構文は廃止した。配列要素アクセスには `@A[i]` / `&@A[i]` を使う。

現在のコンパイラは次のように扱う。

- `A(i)` — function call / call syntax として扱う。配列アクセス特別分岐は存在しない。
- `&A(i)` — 配列要素アドレス access としては認識しない。配列要素アドレスには `&@A[i]` を使う。

#### 将来拡張への判断基準

この記号分担は、今後の構文拡張時の判断基準として使う。

- 新しい要素選択構文は、原則として `[]` 系に寄せる。
- 新しい call / grouping 構文は、原則として `()` 系に寄せる。
- address / lvalue を要求する構文は、原則として `&` を明示する。
- array storage / binding を通常の scalar binding と区別する必要がある場合は、`@` sigil を使う。

ただし、過度に厳格な将来制約にはしない。現時点の設計原則として記録する。

### 配列バインディングの構文

> Issue #663「`DIM @A[n]` — array binding declaration」・Issue #665「`@A[i]` — array element read」・Issue #667「`&@A[i]` — array element address access」・Issue #669「`LET @A[i] = expr` — array element assignment sugar」に基づく設計方針

配列の宣言・読み書きには `@` シジルを使った構文を用いる。

#### `DIM @A[n]` — 配列バインディングの宣言

`DIM @A[n]` は名前 `A` でサイズ `n` の配列を宣言する。

- DEF 内では frame-local な配列として確保する
- トップレベルでは global な配列として確保する

#### `@A[i]` — 配列要素の読み込み

式中の `@A[i]` は配列 `A` の第 `i` 要素の値を返す。

コンパイラは `@A[i]` を次の命令列に展開する：

```
<array handle read>  <index expr>  ARRAY_GET
```

ここで `<array handle read>` はローカルなら `LIT StackAddr(n) FETCH`、グローバルなら `LIT DictAddr(n) FETCH` となる。

#### `&@A[i]` — 配列要素アドレスの取得

式中の `&@A[i]` は配列 `A` の第 `i` 要素のアドレス（`Cell::ArrayAddr`）を返す。`SET` と組み合わせて要素への書き込みに使う。

コンパイラは `&@A[i]` を次の命令列に展開する：

```
<array handle read>  <index expr>  ARRAY_ADDR
```

#### `LET @A[i] = expr` — 配列要素への代入（糖衣）

`LET @A[i] = expr` は、意味的には `SET &@A[i], expr` と同じ配列要素代入を行う糖衣構文である。内部的には `&@A[i]` と同じ `ARRAY_ADDR` 展開を経由した後、`SET` 命令が実行される。

#### `SET &@A[i], expr` — 配列要素への書き込み

`SET &@A[i], expr` は、`&@A[i]` が返す `Cell::ArrayAddr` を左辺に使い、`STORE` によって要素を上書きする。通常の配列要素代入には、糖衣構文 `LET @A[i] = expr` も使える。

### `TO_ARRAY` / `FROM_ARRAY` — 廃止方向（historical note）

`TO_ARRAY` / `FROM_ARRAY` は surface language spec から外す。これらは廃止方向であり、新規コードには使わない。

- value aggregate 用途は `TUPLE(...)` + `T[i]` へ移行する。
- 単一値取得は tuple projection `T[i]` で行う。
- 多値展開が必要な場合はワード多値返却セマンティクスとして別 issue で扱う。

## 文字列

> Issue #458「Cell::Str によるファーストクラス文字列値」に基づく設計方針
> Issue #588（Phase 5B-D2）・#591（Phase 5B-D4）で現行モデルに更新。

### Rc-backed immutable string

Phase 5B-D2（issue #588）以降、`Cell::Str` は `Rc<str>` backed immutable string handle として実装されている。

- `Cell::Str` は `Rc<str>` を内包する。文字列の実体は `Rc` が管理し、`Rc` の参照カウントが 0 になった時点で自動的に解放される。
- `Cell::Str` の clone は `Rc` handle の clone（参照カウントのインクリメント）だけであり、コストは O(1)。
- `Cell::Str` はデータスタック・変数スロット・配列要素間で安全に共有できる。フレームのライフタイム追跡は不要である。
- 文字列は immutable value として扱う。`STR_CONCAT` や `STR_SLICE` などの操作は常に新しい `Rc<str>` を生成して返す。

> Phase 2（issue #539 / #542）で導入した `VM::strings` string pool・`saved_string_pool_len`・`global_string_pool_len`・`PoolRefLifetime` によるライフタイムマトリクスは Phase 5B-D4（issue #590 / #591）で廃止した。

### 文字列リテラル

コンパイル時に出現する文字列リテラル（ソースコード中の `"..."` 構文）は、コンパイル時に `Cell::Str(Rc<str>)` として辞書本体（dictionary）に直接書き込まれる。`DEF ... END` 本体中に現れる文字列リテラルも同様であり、session の間は有効である。

文字列リテラルも実行時生成文字列（`STR_CONCAT` の結果など）も、いずれも同じ `Cell::Str(Rc<str>)` として表現される。両者を区別するランタイム上の特別な仕組みは存在しない。

```tbx
VAR G
DEF SETG()
  SET &G, "inside"
END
SETG
PUTSTR G   \ "inside" を出力する
```

このとき `"inside"` はコンパイル時に `Cell::Str(Rc<str>)` として辞書に埋め込まれ、`SET &G, ...` は `Rc` handle をそのままグローバル変数スロットにコピーする。`STR_CONCAT` などで実行時に生成した文字列も同様にグローバル変数へ格納できる。

回帰テストは `src/vm.rs` の `test_str_literal_inside_word_can_be_assigned_to_global_var` / `test_str_literal_assigned_to_global_var_at_top_level_succeeds` を参照。

### 等値比較

文字列ハンドルの同一性と、文字列内容の等しさは別概念として扱う。これは、実装上の共有最適化と、言語レベルの意味論を分離するためである。`Cell::Str` の `PartialEq` は `Rc<str>` を `str` に deref して内容比較を行う。

## 出力とテキスト

> Issue #2「コア言語における文字列の扱い」・Issue #4「数値を表示する基本命令」に基づく設計方針

### 出力命令

コアの出力命令は、改行付きの高水準 API ではなく、改行なしの小さな操作へ分解する。

この方針の意図は以下。

- 出力の最小単位を直交的に保つ
- 高水準な `PRINT` 系はライブラリで組み立てられるようにする
- コアを小さく保ちながら、文字・数値・文字列の出力を表現可能にする

### エスケープシーケンス

文字列リテラルのエスケープ規則はコア言語の字句仕様として扱う。ここはライブラリでは置き換えられないため、言語仕様として残す価値がある。

### 文字列の保存方式

Phase 5B-D2（issue #588）以降、文字列リテラルおよび動的文字列はすべて `Cell::Str(Rc<str>)` として辞書またはスタック上に直接保持される。別途管理するランタイム文字列プール（`VM::strings`）は廃止された。`Rc<str>` の参照カウントがスコープを超えた共有と自動解放を担保する。

## 他文書への委譲

この文書に列挙しない事項は、原則として以下へ委譲する。

- VM の構造、辞書層、実行モデル: `blueprint.md`
- コンパイルワード、制御構造の展開、低レベルなコンパイル操作: `blueprint-compiler.md`
- 現在有効な primitive 群、内部表現、境界条件、具体エラー: `src/`
