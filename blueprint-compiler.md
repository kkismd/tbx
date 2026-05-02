# TBX コンパイルワード設計・実装

コンパイルワード（コンパイル時に実行される IMMEDIATE ワード）の設計と実装に関する情報を集約するドキュメント。

ワード定義のコンパイル動作（DEF/END）、RETURN の挙動、コンパイルスタックプリミティブ、制御構造の実装パターンを記録する。

---

## DEF/END — ワード定義のコンパイル動作

> Issue #116「ワード本体の終端判定機構（EXIT規約）が仕様に明記されていない」に基づく設計方針

システムに「今は実行中か、それとも定義中か」を示すフラグ（is_compiling）を持たせて `DEF` を「即時実行ワード（IMMEDIATE）」として実装する。

DEF ワードの挙動
* フラグ is_compiling を true にする。
* 次のトークンを読み取り、それを「新しいワードの名前」として辞書に新規登録する。
* 名前に続く `(X, Y)` 形式の仮引数リストがあればパースし、局所変数テーブルをリセットしたうえで各引数名を `StackAddr(0)`, `StackAddr(1)`, ... として登録する（`arity = 引数の数`）。括弧がなければ `arity = 0`。
* 仮引数リストの最後のパラメータが `...` の場合は**可変引数ワード**として登録する（`is_variadic = true`）。`...` の前に固定引数があれば、それらは `StackAddr(0)`, ... として通常通り登録される。可変引数ワードでは、ローカル変数のインデックスが `VARIADIC_LOCAL_BASE` からのオフセット形式（`StackAddr(VARIADIC_LOCAL_BASE + n)`）で登録される。実行時の `VA_COUNT()` / `ARG_ADDR(I)` は `actual_arity`（リターンスタックフレームに保存）を参照する。
* 命令列の書き込み先は `dictionary[DP..]`（`DP_USER` 以降の空き領域）であり、DEF専用の別バッファは確保しない。コンパイルモード中、アウター・インタプリタはステートメント行を1行ずつ処理し、各ステートメントに対してフェーズ1と同じ命令列パターンを `dictionary[DP..]` に書き込む（DP を進めながら）。ステートメントの `EntryKind` に応じて2種類のパターンを使い分ける:

  **`EntryKind::Word` の場合:**
  ```
  Xt(LIT_MARKER)
  [引数式のコード]
  Xt(CALL), Xt(stmt), Int(arity), Int(0)
  Xt(DROP_TO_MARKER)
  ```

  **プリミティブ・変数・定数（それ以外の `EntryKind`）の場合:**
  ```
  Xt(LIT_MARKER)
  [引数式のコード]
  Xt(stmt)
  Xt(DROP_TO_MARKER)
  ```

VAR宣言はコンパイル時のローカル変数テーブルへの登録のみを行い、命令列は生成しない。`local_count` は VAR 宣言ごとに1増やす。RETURN / GOTO / BIF / BIT はそれぞれ対応する命令のXtを生成する。これを終わらせるのが END ワードです。

END ワードの挙動
* 命令列の末尾に EXIT のXtを書き込む（**EXIT規約**）。これがワード本体の終端判定の唯一のメカニズムであり、インナ・インタプリタは `EXIT` の実行によりワード本体の終端を検知する。
* `local_count = 局所変数テーブルのサイズ - arity` を確定し、この値をCALL命令のオペランドに埋め込む。
* 完成した命令列を辞書エントリに確定させる。
* フラグ is_compiling を false に戻す。
* END実行時にコンパイルスタックが空でない場合（対応するENDIFが欠落している等）は `CompileStackNotEmpty { count }` エラーを返し、`rollback_def()` を呼び出して定義を破棄する。

---

## RETURN ステートメントとコンパイラによる分岐

> Issue #150「ワードから値を返す仕組み」に基づく設計方針

ユーザーが使える構文は `RETURN` 1種類だが、コンパイラが内部で2種類のプリミティブにコンパイルする。

| 構文 | コンパイル結果 | 動作 |
| ---- | -------------- | ---- |
| `RETURN expr` | 式の命令列 + `Xt(RETURN_VAL)` | 式を評価して戻り値をスタックに積み、RETURN_VAL で返す |
| `RETURN`（引数なし） | `Xt(EXIT)` | void returnとして即時リターン |
| `END`（ワード本体終端） | `Xt(EXIT)` | void returnとして終端する |

`RETURN expr` が式の途中（`f(x) + g(y)` など）で呼び出された場合も、RETURN_VAL の「退避 → truncate(bp) → 再push」によって引数・ローカル変数が正しく解放され、戻り値のみがスタックに残る。

複数の戻り値はサポートしない（単一値のみ）。

> Issue #128「コンパイル一時辞書のラベル・仮定義メカニズムが不明確」に基づく設計方針

**コンパイル進行中領域のライフサイクル**（`dictionary[DP_USER..DP]`）

DEF〜END 間の命令列は、辞書の独立した層ではなく、`DP_USER` 以降の空き領域（作業バッファ）に直接書き込む方式で管理する。

* **DEF 時**: ヘッダエントリを「未確定」状態で `headers` に追加し、命令列を `DP`（現在の書き込み先）以降に書き始める。`DP` は命令列の書き込みとともに進む。
* **END 時**: EXIT のXtを追記した後、`DP_USER = DP` として確定（マージ）する。ヘッダエントリの「未確定」状態を解除する。また、`RETURN expr` が出現した場合は対応する位置に `Xt(RETURN_VAL)` が書き込まれている。
* **エラー時**: `DP` を `DP_USER` に巻き戻し（書き込んだ命令列を破棄）、ヘッダエントリを削除する。

**行番号ラベルのコンパイラ内部管理**

行番号ラベル（`GOTO` / `BIF` / `BIT` のジャンプ先）はコンパイラ内部の Rust 構造体として管理する（辞書とは独立）。issue #117 のローカル変数テーブルと同じ設計パターン。

* `label_table: HashMap<i64, usize>` — 行番号 → `dictionary` インデックスのマッピング
* `patch_list: Vec<(i64, usize)>` — 未解決の行番号 → パッチすべき `dictionary` オフセットのリスト

DEF 開始時に両構造体を生成し、END 完了後に破棄する。行番号を辞書エントリとして登録しない（辞書の検索経路に行番号を混入させない）。

前方参照はバックパッチで解決する:

1. `GOTO N` コンパイル時にラベル `N` が未定義なら仮アドレス（`0`）を `dictionary` に書き込み、`patch_list` に `(N, offset)` を追記する。
2. ラベル `N` が出現したとき、現在の `DP` を `label_table[N]` として登録し、`patch_list` 内の `N` に対応するすべてのオフセットに正しいアドレスを書き戻す（バックパッチ）。
3. END 完了後に `patch_list` に未解決エントリが残っていた場合はコンパイルエラーとする。

---

## コンパイルスタックプリミティブ

> Issue #365「blueprint-compiler.md を compile_stack/CS_OPEN_TAG 新設計に更新する」に基づく設計方針

### compile_stack の型：`Vec<CompileEntry>`

`VM` はコンパイル時専用のスタック `compile_stack: Vec<CompileEntry>` を1本持つ。
PR #364 以前は `compile_stack: Vec<Cell>`（値スタック）と `control_stack: Vec<ControlKind>`（制御スタック）の2本構成だったが、これらを `Vec<CompileEntry>` として統合した。

`CompileEntry` は2つのバリアントを持つ:

```rust
pub enum CompileEntry {
    Cell(Cell),    // regular value: address, integer, Xt, …
    Tag(String),   // control-structure scope tag, e.g. "IF" or "WHILE"
}
```

**タグラスト方式（tag-last layout）**

制御構造を開く際、`CS_OPEN_TAG` による `Tag` エントリは常に **コンパイルスタックの最上部** に積む（タグラスト）。これにより以下の性質が成り立つ:

- `CS_CLOSE_TAG` はトップの1エントリを検査するだけで制御構造の対応関係を検証できる
- `CS_POP` がタグをデータスタックに誤って移動しようとした場合 `TypeError` を返す（防衛）
- `CS_SWAP` / `CS_DROP` 等のスタック操作は種別を問わず行えるが、タグの位置不変性の維持は tbx 側コードの責任とする

**エラー型**

| エラー型 | フィールド | 説明 |
| -------- | ---------- | ---- |
| `MismatchedTag` | `expected: String`, `found: String` | `CS_CLOSE_TAG` が期待するタグと異なるタグをトップに発見した（例: `IF ... WHILE ... ENDIF`） |
| `NoOpenTag` | `expected: String` | `CS_CLOSE_TAG` 呼び出し時にコンパイルスタックが空、またはトップが `Cell` エントリだった（対応する `CS_OPEN_TAG` が存在しない） |

---

コンパイルワードの実装に使用するプリミティブ群。このうち `CS_PUSH` / `CS_POP` / `CS_SWAP` / `CS_DROP` / `CS_DUP` / `CS_OVER` / `CS_ROT` / `PATCH_ADDR` / `COMPILE_EXPR` / `COMPILE_LVALUE` / `COMPILE_LVALUE_SAVE` / `SKIP_EQ` / `SKIP_COMMA` / `CS_OPEN_TAG` / `CS_CLOSE_TAG` は **コンパイルモード（`is_compiling = true`）専用** であり、実行モード中に呼ばれた場合はエラーとする。`APPEND` / `HERE` / `JUMP_FALSE` / `JUMP_TRUE` / `JUMP_ALWAYS` / `LOOKUP` は汎用プリミティブであり、コンパイルモード以外でも使用できる。

| プリミティブ | スタック効果 | 説明 |
| ------------ | ------------ | ---- |
| `APPEND`     | `( cell -- )` | スタックトップの Cell を `dictionary[DP]` に書き込み DP を +1 進める |
| `HERE`       | `( -- addr )` | 現在の辞書ポインタ（次の書き込み先の DictAddr）をデータスタックに積む |
| `CS_PUSH`    | `( val -- )` | データスタックのトップを compile_stack に移動する |
| `CS_POP`     | `( -- val )` | compile_stack のトップ（`Cell` エントリ）をデータスタックに移動する。`Tag` エントリがトップにある場合は `TypeError` を返す |
| `CS_SWAP`    | `( a b -- b a )` | compile_stack のトップ2要素を交換する |
| `CS_DROP`    | `( a -- )` | compile_stack のトップを捨てる |
| `CS_DUP`     | `( a -- a a )` | compile_stack のトップを複製する |
| `CS_OVER`    | `( a b -- a b a )` | compile_stack の2番目をトップにコピーする |
| `CS_ROT`     | `( a b c -- b c a )` | compile_stack の3番目をトップに移動する |
| `PATCH_ADDR` | `( addr -- )` | `DictAddr(addr)` をポップし、`dictionary[addr] = Cell::DictAddr(DP)` を書き込む（前方参照のバックパッチ） |
| `COMPILE_EXPR` | `( -- )` | ソースから式を1つ読み取ってコンパイルし、命令列を `dictionary[DP..]` に書き込む |
| `COMPILE_LVALUE` | `( -- )` | トークンストリームから識別子を1つ読み、変数アドレスを `LIT addr` として `dictionary[DP..]` に書き込む。ローカル変数は `StackAddr`、グローバル変数は `DictAddr` に解決する |
| `COMPILE_LVALUE_SAVE` | `( -- )` | `COMPILE_LVALUE` と同様にアドレスを辞書に書き込むが、さらに同じアドレスをコンパイルスタック（`compile_stack`）にも積む。FOR ループなど「辞書への emit と後での再利用の両方」が必要な場面で使用する |
| `SKIP_EQ`    | `( -- )` | トークンストリームから次のトークンを読み、`=` であることを検証して破棄する。`=` 以外の場合は `InvalidExpression` |
| `SKIP_COMMA` | `( -- )` | トークンストリームから次のトークンを読み、`,` であることを検証して破棄する。`,` 以外の場合は `InvalidExpression`。FOR ループの `&var, end` 構文のセパレータ消費に使用する |
| `JUMP_FALSE` | `( -- xt )` | `BranchIfFalse`（BIF）のXt定数をデータスタックに積む |
| `JUMP_TRUE`  | `( -- xt )` | `BranchIfTrue`（BIT）のXt定数をデータスタックに積む |
| `JUMP_ALWAYS` | `( -- xt )` | `Goto` のXt定数をデータスタックに積む |
| `LOOKUP`     | `( str -- xt )` | 文字列で指定した名前のワードを辞書から検索し、その Xt をデータスタックに積む。`APPEND LOOKUP("SET")` で任意のワードの Xt を辞書に書き込むために使用する |
| `CS_OPEN_TAG` | `( str -- )` | データスタックから `StringDesc` をポップし、文字列を解決して `CompileEntry::Tag(string)` を compile_stack に積む。制御構造の開始を記録するために使用する |
| `CS_CLOSE_TAG` | `( str -- )` | データスタックから `StringDesc` をポップし、compile_stack のトップが一致する `Tag` であることを検証してポップする。不一致なら `MismatchedTag`、タグがなければ `NoOpenTag` を返す |

`CS_SWAP` / `CS_DROP` / `CS_DUP` / `CS_OVER` / `CS_ROT` は `CompileEntry` の種別（`Cell` / `Tag`）を問わず操作する。タグの整合性チェックは `CS_OPEN_TAG` / `CS_CLOSE_TAG` を通じて tbx 側のコードが責任を持つ。

`PATCH_ADDR` はコンパイルスタックと組み合わせて分岐命令のアドレスを事後的に埋める用途で使用する。典型的なパターン:

```
APPEND JUMP_FALSE   -- emit BIF instruction
CS_PUSH HERE        -- save placeholder address on compile stack
APPEND 0            -- emit address placeholder (0)
...                 -- compile body
PATCH_ADDR CS_POP   -- back-patch the placeholder with current DP
```

---

## IF...ENDIF の実装記録

IF と ENDIF は `lib/basic.tbx` に TBX コードとして実装されたコンパイルワードである。
ELSIF/ELSE のカウント方式導入（issue #356）に伴い、IF は初期カウント `CS_PUSH 0` を追加し、
ENDIF は WHILE ループで複数の JUMP_ALWAYS プレースホルダーをパッチするよう拡張された。

```
DEF IF
  COMPILE_EXPR
  APPEND JUMP_FALSE
  CS_PUSH 0        REM initial count: number of ELSIF calls so far
  CS_PUSH HERE     REM BIF jump-target placeholder address (A)
  APPEND 0
  CS_OPEN_TAG "IF" REM push Tag("IF") last (tag-last layout)
END
IMMEDIATE IF

DEF ENDIF
  CS_CLOSE_TAG "IF"     REM validate and pop Tag("IF") first (fail-fast)
  PATCH_ADDR CS_POP     REM patch last BIF/JUMP_ALWAYS placeholder (C)
  VAR N
  SET &N, CS_POP        REM retrieve ELSIF call count
  VAR I
  SET &I, 0
  WHILE I < N
    PATCH_ADDR CS_POP   REM patch each accumulated JUMP_ALWAYS placeholder (Bi)
    SET &I, I + 1
  ENDWH
END
IMMEDIATE ENDIF
```

**IF の動作**（コンパイル時）:
1. `COMPILE_EXPR` — 条件式をコンパイルし命令列に書き込む
2. `APPEND JUMP_FALSE` — BIF 命令の Xt を書き込む
3. `CS_PUSH 0` — ELSIF 呼び出し回数の初期カウント（0）をコンパイルスタックに積む
4. `CS_PUSH HERE` — ジャンプ先プレースホルダーのアドレスをコンパイルスタックに積む
5. `APPEND 0` — ジャンプ先プレースホルダーとして 0 を書き込む

コンパイルスタック: `[0, A]`（0 が底、A がトップ）

**ENDIF の動作**（コンパイル時）:
1. `PATCH_ADDR CS_POP` — トップのプレースホルダー（C または ELSE の B）を現在の DP でパッチする
2. `SET &N, CS_POP` — ELSIF カウント N を取り出す
3. WHILE ループで N 回、蓄積された JUMP_ALWAYS プレースホルダー（B1…BN）を現在の DP でパッチする

> ELSIF/ELSE を使わない場合、CS は `[0, A]` のまま ENDIF に到達する。N=0 なのでループは 0 回実行され、旧実装と同じ動作になる（後方互換性が保たれる）。

---

## ELSE / ELSIF の実装記録

`IF ... ELSIF ... ELSE ... ENDIF` は `lib/basic.tbx` に TBX コードとして実装されている。
ENDIF の拡張方針は issue #338 で**カウント方式**に決定した。

### コンパイルスタック構造（カウント方式）

コンパイルスタックのフォーマットは `[B1...BN, N, C, Tag("IF")]`（`Tag("IF")` がトップ、タグラスト方式）。

- `C` : 直前の条件分岐（BIF）のジャンプ先プレースホルダーアドレス
- `N` : ELSIF の呼び出し回数（= 蓄積された JUMP_ALWAYS プレースホルダーの数）
- `B1...BN` : 各 if/elsif ブロック末尾の JUMP_ALWAYS プレースホルダーアドレス
- `Tag("IF")` : 制御構造の種別タグ（常にスタックの最上位に位置する）

### IF

`CS_OPEN_TAG "IF"` を末尾に追加し、タグラスト方式でコンパイルスタックを `[0, A, Tag("IF")]` 形式にする。

```
DEF IF
  COMPILE_EXPR
  APPEND JUMP_FALSE
  CS_PUSH 0        REM initial count: number of ELSIF calls so far
  CS_PUSH HERE     REM BIF jump-target placeholder address (A)
  APPEND 0
  CS_OPEN_TAG "IF" REM push Tag("IF") last (tag-last layout)
END
IMMEDIATE IF
```

### ENDIF

`CS_CLOSE_TAG "IF"` を先頭に追加し、compile_stack を触る前にフェイルファストでタグを検証する。`WHILE`/`ENDWH` ループで蓄積された `JUMP_ALWAYS` プレースホルダーを一括パッチする。
`WHILE`/`ENDWH` は `lib/basic.tbx` で ENDIF より前に定義されていること。

```
DEF ENDIF
  CS_CLOSE_TAG "IF"     REM validate and pop Tag("IF") first (fail-fast)
  PATCH_ADDR CS_POP     REM patch last BIF/JUMP_ALWAYS placeholder (C)
  VAR N
  SET &N, CS_POP        REM retrieve ELSIF call count
  VAR I
  SET &I, 0
  WHILE I < N
    PATCH_ADDR CS_POP   REM patch each accumulated JUMP_ALWAYS placeholder (Bi)
    SET &I, I + 1
  ENDWH
END
IMMEDIATE ENDIF
```

### ELSIF

`CS_CLOSE_TAG "IF"` を先頭に、`CS_OPEN_TAG "IF"` を末尾に追加し、タグを取り外して操作後に積み直す。

コンパイルスタックの遷移:
- 入力: `CS = [..., N, C, Tag("IF")]`（Tag がトップ）
- 出力: `CS = [..., B, N+1, C_new, Tag("IF")]`（C パッチ済み、B は JUMP_ALWAYS プレースホルダー）

```
DEF ELSIF
  CS_CLOSE_TAG "IF"        REM pop Tag("IF") first
  APPEND JUMP_ALWAYS       REM emit unconditional jump at end of current branch body
  VAR B
  SET &B, HERE             REM save JUMP_ALWAYS placeholder address (B)
  APPEND 0                 REM emit placeholder
  PATCH_ADDR CS_POP        REM patch previous BIF placeholder (C); DP now = elsif condition start
  VAR N
  SET &N, CS_POP           REM pop the ELSIF count
  CS_PUSH B                REM push B (JUMP_ALWAYS placeholder)
  CS_PUSH N + 1            REM push incremented count
  COMPILE_EXPR             REM compile the elsif condition expression
  APPEND JUMP_FALSE        REM emit new BIF instruction
  CS_PUSH HERE             REM push new BIF placeholder address (C_new)
  APPEND 0
  CS_OPEN_TAG "IF"         REM push Tag("IF") back
END
IMMEDIATE ELSIF
```

### ELSE

`CS_CLOSE_TAG "IF"` を先頭に、`CS_OPEN_TAG "IF"` を末尾に追加し、タグを取り外して操作後に積み直す。

コンパイルスタックの遷移:
- 入力: `CS = [..., N, C, Tag("IF")]`（Tag がトップ）
- 出力: `CS = [..., N, B, Tag("IF")]`（C パッチ済み、B は JUMP_ALWAYS プレースホルダー、N は変化なし）

```
DEF ELSE
  CS_CLOSE_TAG "IF"        REM pop Tag("IF") first
  APPEND JUMP_ALWAYS       REM emit unconditional jump at end of if/elsif branch
  VAR B
  SET &B, HERE             REM save JUMP_ALWAYS placeholder address (B)
  APPEND 0                 REM emit placeholder
  PATCH_ADDR CS_POP        REM patch previous BIF placeholder (C); DP now = else body start
  CS_PUSH B                REM push B on top of N
  CS_OPEN_TAG "IF"         REM push Tag("IF") back
END
IMMEDIATE ELSE
```

### コンパイルスタックのトレース

**IF...ELSE...ENDIF**:

| 時点 | CS |
|---|---|
| IF 後 | `[0, A, Tag("IF")]` |
| ELSE 後 | `[0, B, Tag("IF")]`（A パッチ済み）|
| ENDIF 後 | `[]`（B パッチ、N=0、ループ 0 回）|

**IF...ELSIF...ENDIF**:

| 時点 | CS |
|---|---|
| IF 後 | `[0, A, Tag("IF")]` |
| ELSIF 後 | `[B1, 1, C, Tag("IF")]`（A パッチ済み）|
| ENDIF 後 | `[]`（C パッチ、N=1、B1 パッチ）|

**IF...ELSIF...ELSE...ENDIF**:

| 時点 | CS |
|---|---|
| IF 後 | `[0, A, Tag("IF")]` |
| ELSIF 後 | `[B1, 1, C, Tag("IF")]`（A パッチ済み）|
| ELSE 後 | `[B1, 1, B_new, Tag("IF")]`（C パッチ済み）|
| ENDIF 後 | `[]`（B_new パッチ、N=1、B1 パッチ）|

### 既知の制限事項

- **ELSE の二重使用**: `IF ... ELSE ... ELSE ... ENDIF` を書いてもコンパイルエラーにならない。
  2番目の `ELSE` は最初の ELSE が積んだ JUMP_ALWAYS プレースホルダーを BIF プレースホルダーとして扱い、
  誤ったアドレスにパッチしてしまう。現時点では二重 ELSE の検出は行わず、未定義動作とする。

- **ELSIF/ELSE を IF なしで使用**: `CS_CLOSE_TAG "IF"` が空スタックを検出し `NoOpenTag { expected: "IF" }` を返す。


---

## WHILE...ENDWH の実装記録

WHILE と ENDWH は `lib/basic.tbx` に TBX コードとして実装されたコンパイルワードである。

```
REM WHILE expr ... ENDWH
DEF WHILE
  CS_PUSH HERE
  COMPILE_EXPR
  APPEND JUMP_FALSE
  CS_PUSH HERE
  APPEND 0
  CS_OPEN_TAG "WHILE" REM push Tag("WHILE") last (tag-last layout)
END
IMMEDIATE WHILE

DEF ENDWH
  CS_CLOSE_TAG "WHILE" REM validate and pop Tag("WHILE") first (fail-fast)
  CS_SWAP
  APPEND JUMP_ALWAYS
  APPEND CS_POP
  PATCH_ADDR CS_POP
END
IMMEDIATE ENDWH
```

### 生成される命令列（実行時）

```
A:  [条件式のコード]
    BIF  D            ← 条件が偽なら D にジャンプ
    [本体のコード]
    JUMP_ALWAYS  A    ← ループ先頭に戻る（DictAddr ターゲット）
D:  ...（ENDWH 直後）
```

### コンパイルスタックの遷移

| 時点 | コンパイルスタック |
|---|---|
| WHILE 実行直前 | `[]` |
| WHILE 実行後 | `[A, Caddr, Tag("WHILE")]`（Tag がトップ、タグラスト方式） |
| ENDWH 実行後 | `[]` |

- `A` = ループ先頭の DictAddr（WHILE が `CS_PUSH HERE` で積む）
- `Caddr` = BIF のジャンプ先プレースホルダーの DictAddr（WHILE が `APPEND 0` の直前に `CS_PUSH HERE` で積む）
- `Tag("WHILE")` = 制御構造の種別タグ（`CS_OPEN_TAG "WHILE"` で最後に積む）

### ENDWH の動作トレース

1. `CS_CLOSE_TAG "WHILE"` — compile_stack のトップが `Tag("WHILE")` か検証してポップする（フェイルファスト）。空スタックなら `NoOpenTag { expected: "WHILE" }`、別タグなら `MismatchedTag` を返す
2. `CS_SWAP` — CS を `[A, Caddr]` → `[Caddr, A]` に並び替える（A がトップ）
3. `APPEND JUMP_ALWAYS` — 無条件ジャンプ命令の Xt を辞書に書き込む
4. `APPEND CS_POP` — CS から A（DictAddr）をポップして辞書に書き込む（JUMP_ALWAYS のジャンプ先ターゲット）
5. `PATCH_ADDR CS_POP` — CS から Caddr をポップし、`dictionary[Caddr]` に現在の DP（= ENDWH 直後）を書き込む（BIF のジャンプ先を確定）

### `read_jump_target` の簡略化

全ジャンプターゲットは `Cell::DictAddr` に統一されている。そのため `vm.rs` の `read_jump_target` は `Cell::DictAddr(a)` のみを受け付け、それ以外は `TypeError` を返す。

```rust
fn read_jump_target(&self, offset: usize) -> Result<usize, TbxError> {
    let cell = self.dict_read(offset)?;
    match cell {
        Cell::DictAddr(a) => Ok(a),
        _ => Err(TbxError::TypeError {
            expected: "DictAddr (jump target)",
            got: cell.type_name(),
        }),
    }
}
```

フォワードジャンプ（BIF → ENDWH 直後 D）は `PATCH_ADDR` が `Cell::DictAddr(dp)` を書き込む。バックジャンプ（ENDWH → ループ先頭 A）のターゲットも `Cell::DictAddr` として辞書に書き込まれる。すべてのジャンプターゲットが `Cell::DictAddr` に統一されているため、算術整数と実行アドレスが型レベルで区別される。


---

## DO...UNTIL の実装記録

> Issue #381「DO ... UNTIL expr の実装」に基づく設計方針

DO と UNTIL は `lib/basic.tbx` に TBX コードとして実装されたコンパイルワードである。
`WHILE...ENDWH` が前判定ループであるのに対し、`DO...UNTIL` は後判定ループであり、
ループ本体を少なくとも1回実行する。条件が**真**のときループを脱出し、**偽**のときに先頭へ戻る。

```
REM DO ... UNTIL expr
DEF DO
  CS_PUSH HERE
  CS_OPEN_TAG "DO"
END
IMMEDIATE DO

DEF UNTIL
  CS_CLOSE_TAG "DO"
  COMPILE_EXPR
  APPEND JUMP_FALSE
  APPEND CS_POP
END
IMMEDIATE UNTIL
```

### 生成される命令列（実行時）

```
A:  [ループ本体のコード]
    [条件式のコード]
    BIF  A            ← 条件が偽なら A に戻る（ループ継続）
D:  ...（UNTIL 直後）
```

### コンパイルスタックの遷移

| 時点 | コンパイルスタック |
|---|---|
| DO 実行直前 | `[]` |
| DO 実行後 | `[A, Tag("DO")]`（Tag がトップ、タグラスト方式） |
| UNTIL 実行後 | `[]` |

- `A` = ループ先頭の DictAddr（DO が `CS_PUSH HERE` で積む）

### UNTIL の動作トレース

1. `CS_CLOSE_TAG "DO"` — compile_stack のトップが `Tag("DO")` か検証してポップする（フェイルファスト）
2. `COMPILE_EXPR` — 条件式をコンパイルして命令列に書き込む
3. `APPEND JUMP_FALSE` — BIF 命令の Xt を辞書に書き込む
4. `APPEND CS_POP` — CS から A（DictAddr）をポップして辞書に書き込む（BIF のジャンプ先）

---

## FOR...NEXT の実装記録

> Issue #420「FOR/NEXT ループを簡略化する（start=1・step=1 固定）」に基づく設計方針

FOR と NEXT は `lib/basic.tbx` に TBX コードとして実装されたコンパイルワードである。
`FOR &var, end` の形式で、ループ変数を 1 に初期化し、`var <= end` の間ループ本体を繰り返す（start=1・step=1 固定）。

```
# FOR &varref, end ... NEXT
DEF FOR
  # Read &var, emit LIT addr, and save var_addr for reuse in NEXT.
  VAR VAR_ADDR
  COMPILE_LVALUE_SAVE
  SET &VAR_ADDR, CS_POP
  # Consume the comma between &var and the end expression.
  SKIP_COMMA
  # Emit: LIT 1, SET  ->  var = 1 (fixed start)
  LITERAL 1
  APPEND LOOKUP("SET")
  # Record loop-condition start address A on the compile stack.
  CS_PUSH HERE
  # Emit condition: LIT addr, FETCH, <end_expr>, LE
  LITERAL VAR_ADDR
  APPEND LOOKUP("FETCH")
  COMPILE_EXPR
  APPEND LOOKUP("LE")
  # Emit JUMP_FALSE placeholder (patched by NEXT to D).
  APPEND JUMP_FALSE
  CS_PUSH HERE
  APPEND 0
  # Save var_addr for NEXT (needed to emit increment code).
  CS_PUSH VAR_ADDR
  # Push scope tag last (tag-last layout).
  CS_OPEN_TAG "FOR"
END
IMMEDIATE FOR

DEF NEXT
  # Validate and pop the "FOR" scope tag.
  CS_CLOSE_TAG "FOR"
  # Restore saved values from the compile stack.
  VAR VAR_ADDR
  SET &VAR_ADDR, CS_POP
  VAR BIF_ADDR
  SET &BIF_ADDR, CS_POP
  # Emit increment code: LIT addr, LIT addr, FETCH, LIT 1, ADD, SET  ->  var += 1
  LITERAL VAR_ADDR
  LITERAL VAR_ADDR
  APPEND LOOKUP("FETCH")
  LITERAL 1
  APPEND LOOKUP("ADD")
  APPEND LOOKUP("SET")
  # Emit JUMP_ALWAYS back to A (loop-condition start).
  APPEND JUMP_ALWAYS
  APPEND CS_POP
  # Patch the JUMP_FALSE placeholder to here (= D, loop exit).
  PATCH_ADDR BIF_ADDR
END
IMMEDIATE NEXT
```

### 生成される命令列（実行時）

```
    LIT addr_of_var
    LIT 1
    SET                      -- var = 1 (fixed start)
A:
    LIT addr_of_var
    FETCH                    -- push current var value
    [end expression]
    LE                       -- var <= end ?
    BIF D                    -- exit loop if false
    [loop body]
    LIT addr_of_var
    LIT addr_of_var
    FETCH
    LIT 1
    ADD
    SET                      -- var += 1
    JUMP_ALWAYS A
D:  ...（NEXT 直後）
```

### コンパイルスタックの遷移（タグラスト方式）

| 時点 | コンパイルスタック |
|---|---|
| FOR 実行直前 | `[]` |
| FOR 実行後 | `[A, BIF_placeholder, VAR_ADDR, Tag("FOR")]`（Tag がトップ） |
| NEXT 実行後 | `[]` |

---

## LET — BASIC スタイル代入文の実装記録

> Issue #391「LET文の実装」に基づく設計方針

`LET I = 10` は `SET &I, 10` の糖衣構文として `lib/basic.tbx` に TBX コンパイルワードとして実装する。

### 必要な新プリミティブ

`COMPILE_LVALUE` と `SKIP_EQ` を新たに追加する（上記プリミティブ一覧を参照）。
ワードの Xt を辞書に書き込むには `APPEND LOOKUP("SET")` パターンを使用する（`LOOKUP` プリミティブ参照）。

### LET の実装（`lib/basic.tbx`）

```
DEF LET
  COMPILE_LVALUE        REM read variable name, emit LIT addr
  SKIP_EQ               REM consume '='
  COMPILE_EXPR          REM compile right-hand side expression
  APPEND LOOKUP("SET")  REM emit SET instruction
END
IMMEDIATE LET
```

**LET の動作**（コンパイル時）:
1. `COMPILE_LVALUE` — トークンストリームから識別子を読み、`LIT StackAddr(idx)` または `LIT DictAddr(addr)` を辞書に書き込む
2. `SKIP_EQ` — `=` トークンを読み捨てる
3. `COMPILE_EXPR` — 残りのトークンを式としてコンパイルし命令列に書き込む
4. `APPEND LOOKUP("SET")` — SET 命令の Xt を辞書に書き込む

生成されるコード（`LET I = X + 1` の場合、I はローカル変数）:

```
LIT StackAddr(idx_of_I)
[X + 1 の命令列]
SET
```

これは `SET &I, X + 1` が生成するコードと同等。

**制約**:
- DEF ボディ内専用（コンパイルモード）。トップレベルでは `SET &I, expr` を使うこと
- 配列要素への代入（`LET A(I) = ...`）は非対応
