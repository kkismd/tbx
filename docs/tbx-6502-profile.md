# tbx-6502 初期ターゲットプロファイル

この文書は、最初の `tbx-6502` 実装が受け付ける TBX サブセットと、6502 上の実行モデルを定義する。

対象は `generic-6502` プロファイルである。C64、Apple II などの実機向け差分は、後述する platform profile で上書きする。

このプロファイルはホスト VM の完全な移植ではない。ホスト側で TBX ソースを解析・検査・コンパイルし、ターゲット側では生成済みコードを小さな実行専用ランタイムで動かす。

## 適合レベル

機能を次の三段階に分ける。

- **初期版でサポート**: M2 の `tbx16` VM と M3 のスレッディング IR が実装すべき範囲。
- **延期**: モデルと矛盾しないが、初期版には含めない。追加時はこの文書を更新する。
- **非対応**: 初期 6502 ターゲットの設計目標に含めない。ホストコンパイル時に明示的に拒否する。

非対応機能を暗黙にホスト VM の挙動へフォールバックしてはならない。

## 実行モデル

### ホストコンパイル、ターゲット実行

初期版は次の構成とする。

1. ホスト上で TBX ソースを読み、名前解決、型・範囲・プロファイル適合性を検査する。
2. ホスト上で `tbx-6502` 用スレッディング IR と静的データを生成する。
3. IR を Rust 上の `tbx16` VM、または 6502 ランタイム向けコードへ変換する。
4. 6502 側は生成済みコードを実行するだけで、TBX の lexer、parser、compiler、REPL は持たない。

`USE` や compile-time word を利用する場合も、その処理はホスト側で完結させる。ターゲットイメージには実行時に必要なワードとデータだけを含める。

### スレッディング方式

初期版は、16bit の実行トークンまたはコードアドレスを並べるスレッディング表現を前提とする。正確な direct/indirect threading の選択は M2/M3 で決めてよいが、次の性質を守る。

- 実行トークンは 1 セルで表現できる。
- リテラル、分岐先、データアドレス、文字列参照も 1 セル値または連続する静的データで表現できる。
- ホスト VM の `Cell` enum や Rust オブジェクトへの参照をターゲット値として持ち込まない。

## Cell モデル

### 16bit タグなしセル

ターゲットの基本値は 16bit のタグなしセルである。実装上は `u16` 相当のビット列として保持する。

同じ 16bit 値を、命令の文脈に応じて次のいずれかとして解釈する。

- 整数
- 真偽値
- メモリアドレス
- 実行トークン

実行時型タグは持たない。ホストコンパイラが不正な組み合わせを可能な限り拒否し、ターゲットランタイムはホスト VM 相当の詳細な型検査を行わない。

### メモリ上の表現

1 セルは 2 バイトで、6502 の慣例に合わせて little-endian とする。

```text
address + 0: low byte
address + 1: high byte
```

コンパイラが配置するセル領域は原則として偶数アドレスから開始する。ただし、ランタイムの `FETCH` / `STORE` は 6502 自体にアラインメント制約がないことを前提とし、実装上必須の制約にはしない。

## 整数セマンティクス

### 基本解釈

整数演算ではセルを 16bit 2 の補数として扱う。

- 符号付き範囲: `-32768..32767`
- 符号なし範囲: `0..65535`
- 通常の算術比較、除算、十進出力は符号付き解釈を使う。
- アドレス、実行トークン、ビット演算は符号なし 16bit 値として扱う。

初期版の整数リテラルは、ホストコンパイル時に 16bit セルへ表現可能でなければならない。通常の十進整数としては `-32768..32767` を受け付け、それ以外は明示的なコンパイルエラーとする。アドレスや実行トークンはコンパイラがシンボルから生成するため、ソース上の大きな整数で代用しない。

### オーバーフロー

`+`, `-`, `*`, 単項 `-` は 16bit でラップする。

```text
32767 + 1  -> $8000 -> -32768
-32768 - 1 -> $7fff -> 32767
```

これは 6502 上で単純かつ決定的に実装できる規約として、ホスト VM の checked 64bit 整数演算とは意図的に異なる。

### 除算と剰余

`/` と `MOD` は符号付き 16bit 整数に対して動作する。

- 商は 0 方向へ切り捨てる。
- 剰余は `a = (a / b) * b + (a MOD b)` を満たし、0 でない場合は被除数と同じ符号を持つ。
- 0 除算はターゲット実行エラーとする。
- `-32768 / -1` は 16bit にラップして `-32768`、剰余は `0` とする。

ターゲット実行エラーの具体的な停止方法は platform profile またはランタイム実装で定めるが、成功値として処理を継続してはならない。

### 比較

- `=` と `<>` は 16bit ビット列の一致・不一致を比較する。
- `<`, `<=`, `>`, `>=` は符号付き整数比較とする。
- 初期版では unsigned comparison 専用の表面演算子を提供しない。

## 真偽値

真偽値は次のように表現する。

- `FALSE = $0000`
- `TRUE = $ffff`

条件分岐は `0` を false、0 以外を true と解釈する。比較演算、論理演算、`TO_BOOL` 相当の正規化結果は必ず `$0000` または `$ffff` にする。

この規約により、正規化済み真偽値に対する bitwise AND/OR も論理 AND/OR と整合する。ただし、`&&` / `||` は TBX の短絡評価を維持し、単なる eager な bitwise 演算へ置き換えてはならない。

## スタックとメモリ配置

### generic-6502 の zero page

初期 `generic-6502` プロファイルでは、zero page を次のように使う。

```text
$0000-$001F  VMレジスタ / 作業ポインタ
$0020-$003F  I/O・一時領域・乗除算ワーク
$0040-$007F  予備 / platform profile 用
$0080-$00FF  データスタック
```

データスタックは 1 セル 2 バイト、`$0080` から上方向へ伸びる。最大 64 セルとする。DSP は「次に書き込む空きセル」の byte address を保持する。

初期版では TOS キャッシュを行わない。M2 の Rust VM も、最適化前の観察可能な挙動として 64 セル上限を再現する。

### VM レジスタ

`$0000-$001F` には、少なくとも次の論理レジスタまたはポインタを置ける領域を確保する。

- instruction pointer
- data stack pointer
- current frame/base pointer
- return stack pointer
- current word / execution token 用ポインタ
- 汎用一時ポインタ

正確な byte offset はランタイム実装で確定する。platform profile は予約済み zero page と衝突する場合に、この配置を別領域へ差し替えられる。

### return stack

return stack は 6502 の page 1 ハードウェアスタックと混在させず、通常 RAM 上の VM 専用領域に置く。

return stack には、少なくともワード呼び出しから復帰するための情報と、必要なら呼び出し元フレーム情報を保持する。ハードウェアスタックは 6502 サブルーチン呼び出しやランタイム内部処理のために残す。

return stack の開始・終了アドレスと最大深さは platform profile またはリンク設定が供給する。`generic-6502` は特定の通常 RAM アドレスを固定しない。

### platform profile

`platform-c64`, `platform-apple2` などは、少なくとも次を上書きできる。

- zero page の予約領域と VM レジスタ位置
- データスタック位置と上限
- return stack の RAM 範囲
- コード・静的データ・グローバル領域
- 文字コードと I/O hook
- 起動・終了・実行エラー処理
- 乱数 seed の取得方法

言語レベルの 16bit セル、整数、真偽値の規約は platform profile で変更しない。

## ワードと呼び出し規約

### 初期版でサポート

- `DEF ... END` によるユーザー定義ワード
- 固定 arity の仮引数
- 引数なしワード
- statement call と式内 call
- void return と 1 セルの値を返す `RETURN`
- 自己再帰を含む通常のワード呼び出し。ただしスタック上限を超えれば実行エラー

呼び出し側は引数を左から右へ評価してデータスタックへ積む。callee は固定個数の引数と、コンパイル時に個数が確定したローカルセルからフレームを構成する。

1 セルを返すワードは、復帰時に引数・ローカルを破棄し、戻り値 1 セルだけを呼び出し側データスタックへ残す。void word は値を残さない。

### ローカル変数

`DEF` 内のスカラー `VAR` と仮引数をサポートする。ローカルスロット数はコンパイル時に確定し、callee のデータスタックフレーム内に確保する。

`&local` は現在のフレーム内セルを指す 16bit byte address として扱えるが、そのアドレスをワードから返す、グローバルへ保存する、呼び出し終了後に利用する操作は非対応とする。ホストコンパイラは検出可能な escape を拒否する。

### 初期版で非対応

- 可変長ワードと `...`
- 裸の多値返却
- closure、first-class word、動的 call
- ターゲット上での新規ワード定義、`HEADER`, `IMMEDIATE`, `LITERAL` などの compile-time 辞書操作

## サポートする文と制御構造

初期版で次をサポートする。

- `DEF`, `END`, `RETURN`
- スカラー `VAR`
- `SET &target, expr`
- `HALT`
- `IF`, `ELSE`, `ENDIF`
- `WHILE`, `ENDWH`
- ワード内ローカルラベルと `GOTO`, `BIF`, `BIT`
- 改行またはセミコロンによる文終端
- `#` と `REM` コメント
- ホストコンパイル時に完結する `USE`

構造化制御構文がホスト側 compile-time word として実装されている場合は、ターゲット IR へ分岐として完全に展開してから出力する。ターゲットランタイムに compile-time VM を搭載しない。

初期版では、ターゲットでの対話実行、`exec_line` 相当の逐次コンパイル、動的 source include は非対応とする。

## 式と演算子

### 初期版でサポート

- 16bit 整数リテラル
- `TRUE`, `FALSE`
- スカラー変数、仮引数、配列要素の読み出し
- 固定 arity ワード呼び出し
- 算術: `+`, `-`, `*`, `/`, `%`
- 比較: `=`, `<>`, `<`, `<=`, `>`, `>=`
- 論理: `!`, `&&`, `||`
- bitwise: `&`, `|`
- 括弧
- 引数区切りとしてのカンマ

`&&` と `||` は短絡評価する。結果は canonical boolean に正規化する。

### 延期

- shift 演算
- unsigned comparison と unsigned division の表面 API
- 定数畳み込み以外のターゲット固有算術最適化

### 非対応

- `Float` リテラルと浮動小数点演算
- 文字列値を返す式
- tuple の生成・projection
- 配列全体を値として扱う式

## グローバル変数とアドレス

トップレベルのスカラー `VAR` をサポートする。各グローバル変数は静的 RAM に 1 セル確保し、プログラム開始時の値は `0` とする。

- `G` はグローバルセルの値を読み出す。
- `&G` はそのセルの 16bit byte address を返す。
- `FETCH(addr)` は `addr` から little-endian の 1 セルを読む。
- `STORE(addr, value)` または `SET &G, value` は 1 セルを書き込む。

コード、読み取り専用文字列、書き込み可能 RAM の区別はリンク時に既知である。ホストコンパイラは、文字列領域やコード領域への `STORE` のように静的に判定できる不正書き込みを拒否する。

初期版では任意の整数からアドレスを捏造する cast 構文を提供しない。アドレスは変数、配列要素、コンパイラ生成シンボルから得る。

## 静的配列

初期版では、トップレベルで宣言する 1 次元の静的配列だけをサポートする。

```tbx
DIM @A[10]
SET &@A[1], 42
PUTDEC @A[1]
```

規約は次の通り。

- 要素は 16bit セル。
- 添字は既存 TBX と同じ 1-origin。
- 要素数は正のコンパイル時定数。
- ストレージは静的 RAM に連続配置し、開始時に 0 初期化する。
- `@A[i]`, `&@A[i]`, `SET &@A[i], value`, `ARRAY_LEN(@A)` をサポートする。
- 実行時の範囲外添字はターゲット実行エラーとする。

初期版では次を延期する。

- `DEF` 内のローカル配列
- 2 次元配列
- platform 固有の packed byte array

動的ヒープ配列、nested array、配列全体の代入・返却・比較は非対応とする。

## 文字列リテラル

初期版の文字列は **出力専用の静的 byte string** とする。ファーストクラス値ではない。

文字列リテラルはホストコンパイル時にエスケープを展開し、ターゲットイメージの読み取り専用データへ次の形式で配置する。

```text
word length in bytes, little-endian
length bytes of encoded data
```

IR は文字列そのものではなく `StringRef` 相当のシンボル参照を保持する。`PUTSTR "literal"` は、文字列アドレスを出力 primitive/hook へ渡す形に lowering する。

- generic profile の文字列は 8bit byte sequence とする。
- platform profile が ASCII、PETSCII などの文字エンコーディングを定める。
- ソース文字を選択した platform encoding に変換できない場合はコンパイルエラーとする。
- NUL 終端には依存しないため、データ中の `0` byte を許容する。
- 同一文字列の deduplication は許可するが必須ではない。

初期版では、文字列を変数・配列へ保存する、返す、連結する、slice する、比較する、入力値として生成する操作は非対応とする。

## I/O プリミティブ

`generic-6502` ランタイムは、platform profile から次の hook を受け取る。

### 必須

- `PUTCHR(value)`: `value` の low byte を 1 文字として出力する。
- `PUTDEC(value)`: value を符号付き 16bit 十進数として出力する。改行は付けない。
- `PUTSTR(ref)`: length-prefixed な静的 byte string をそのまま出力する。
- `HALT`: 正常終了する。
- `TRAP(code)`: 0 除算、stack overflow、配列範囲外など、継続不能なターゲット実行エラーを通知して停止する。

改行は `PUTCHR` で platform profile が定める文字コードを出力するか、ホスト側で展開したライブラリワードで構成する。高水準な `PRINT` は必須ランタイム API にしない。

### 延期

- 文字入力
- 十進入力
- ファイル I/O
- 端末制御

入力を追加する場合は、blocking/non-blocking、EOF、parse failure を platform profile で明示し、ホスト VM の tuple ベース recoverable API をそのまま持ち込まない。

## 乱数

初期版は `RND(n)` と seed 設定用のランタイム入口をサポートする。

- `n` は `1..32767`。範囲外はターゲット実行エラー。
- 結果は既存 TBX と同じ `1..n` の一様な整数。
- modulo bias を避けるため、実装は rejection sampling を使う。
- PRNG の内部状態と algorithm は platform 間で共通にし、同じ seed と同じ呼び出し列から同じ結果を得る。
- 初期 algorithm は 16bit xorshift など、小さく非暗号学的なものを採用してよい。algorithm を確定した時点でテストベクトルとともに文書化する。
- seed `0` を内部の禁止状態にする algorithm では、決められた非 0 値へ正規化する。
- `RANDOMIZE` は platform profile の entropy/time hook があればそれを seed に使い、なければ決定的な既定 seed を使う。

暗号学的安全性は目標としない。`SHUFFLE` は初期ランタイム primitive ではなく、静的配列サポートの上にライブラリとして実装する候補とする。

## 初期命令・primitive の最小集合

M2 の `tbx16` VM は少なくとも次を直接表現または同等に実行できること。

- 実行: `LIT`, `EXIT`, user word call, `HALT`
- 分岐: unconditional branch, branch-if-false / zero-branch
- スタック: `DUP`, `DROP`, `SWAP`, `OVER`
- 算術: `+`, `-`, `*`, `/`, `MOD`, negate
- 比較: `=`, `<>`, `<`, `<=`, `>`, `>=`
- 論理・bitwise: canonical boolean normalization, `NOT`, AND/OR, bitwise AND/OR
- メモリ: `FETCH`, `STORE`
- フレーム: fixed-arity call、引数/ローカルセル参照、0/1 セル return
- 配列: address calculation と bounds check、または同等の lowering
- 出力: `PUTDEC`, `PUTCHR`, `PUTSTR`
- 乱数: seed、bounded `RND`

primitive 名や opcode の正確な綴りは M2/M3 で決めてよい。上記の観察可能な意味論を変えてはならない。

## 明示的な非対応機能

次は初期 `tbx-6502` ターゲットの非目標とする。

- `Float`
- ファーストクラスな動的文字列
- 実行時値としての文字列連結・比較・slice
- 動的ヒープ配列
- ファーストクラスな配列
- tuple
- 可変長ワード
- 裸の多値返却
- ホスト VM 相当の詳細な実行時型チェック
- 例外捕捉または実行時エラーからの回復
- 6502 ターゲット上での TBX parser/compiler 実行
- ターゲット上の REPL、セルフホスト、動的 `USE`
- ターゲット実行中の辞書拡張と compile-time word 実行
- garbage collector と一般用途 heap

これらがソースに現れた場合、M3 のホストコンパイラは対象箇所を示す profile error として拒否する。

## M2 / M3 への実装契約

### M2: Rust 上の tbx16 VM

- セルは `u16` 等の 16bit 値であり、ホスト VM の `Cell` enum に依存しない。
- 64 セルのデータスタック上限を既定値として再現する。
- arithmetic wrap、signed division、canonical boolean、little-endian memory をユニットテストする。
- 手書きのスレッディングプログラムだけで call、branch、memory、output を検証できる。
- target trap は決定的なエラー値として観察できる。

### M3: スレッディング IR

IR は少なくとも次を区別して表現する。

- primitive / user word reference
- 16bit literal
- label、unconditional branch、conditional branch
- global/data address
- local/argument slot reference
- output-only string reference
- static scalar / array data block

lowering 前または lowering 中に、このプロファイルに含まれない値型、構文、word、配列形状を拒否する。Rust ホスト VM で偶然実行できることを、6502 対応の根拠にしてはならない。

## 変更方針

この文書は初期ターゲットの契約である。実装上の都合で意味論を暗黙に変更せず、次のどちらかを行う。

1. 実装をこのプロファイルへ合わせる。
2. issue / PR で設計差分を明示し、この文書を先に更新する。

platform 固有差分は language profile を分岐させず、可能な限り zero page、memory map、encoding、I/O、起動処理の差し替えとして表現する。
