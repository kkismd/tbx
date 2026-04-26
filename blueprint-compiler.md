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

コンパイルワードの実装に使用するプリミティブ群。このうち `CS_PUSH` / `CS_POP` / `CS_SWAP` / `CS_DROP` / `CS_DUP` / `CS_OVER` / `CS_ROT` / `PATCH_ADDR` / `COMPILE_EXPR` は **コンパイルモード（`is_compiling = true`）専用** であり、実行モード中に呼ばれた場合はエラーとする。`APPEND` / `HERE` / `JUMP_FALSE` / `JUMP_TRUE` / `JUMP_ALWAYS` は汎用プリミティブであり、コンパイルモード以外でも使用できる。

| プリミティブ | スタック効果 | 説明 |
| ------------ | ------------ | ---- |
| `APPEND`     | `( cell -- )` | スタックトップの Cell を `dictionary[DP]` に書き込み DP を +1 進める |
| `HERE`       | `( -- addr )` | 現在の辞書ポインタ（次の書き込み先の DictAddr）をデータスタックに積む |
| `CS_PUSH`    | `( val -- )` | データスタックのトップを compile_stack に移動する |
| `CS_POP`     | `( -- val )` | compile_stack のトップをデータスタックに移動する |
| `CS_SWAP`    | `( a b -- b a )` | compile_stack のトップ2要素を交換する |
| `CS_DROP`    | `( a -- )` | compile_stack のトップを捨てる |
| `CS_DUP`     | `( a -- a a )` | compile_stack のトップを複製する |
| `CS_OVER`    | `( a b -- a b a )` | compile_stack の2番目をトップにコピーする |
| `CS_ROT`     | `( a b c -- b c a )` | compile_stack の3番目をトップに移動する |
| `PATCH_ADDR` | `( addr -- )` | `DictAddr(addr)` をポップし、`dictionary[addr] = Cell::DictAddr(DP)` を書き込む（前方参照のバックパッチ） |
| `COMPILE_EXPR` | `( -- )` | ソースから式を1つ読み取ってコンパイルし、命令列を `dictionary[DP..]` に書き込む |
| `JUMP_FALSE` | `( -- xt )` | `BranchIfFalse`（BIF）のXt定数をデータスタックに積む |
| `JUMP_TRUE`  | `( -- xt )` | `BranchIfTrue`（BIT）のXt定数をデータスタックに積む |
| `JUMP_ALWAYS` | `( -- xt )` | `Goto` のXt定数をデータスタックに積む |

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

```
REM IF expr ... ENDIF
DEF IF
  COMPILE_EXPR
  APPEND JUMP_FALSE
  CS_PUSH HERE
  APPEND 0
END
IMMEDIATE IF

DEF ENDIF
  PATCH_ADDR CS_POP
END
IMMEDIATE ENDIF
```

**IF の動作**（コンパイル時）:
1. `COMPILE_EXPR` — 条件式をコンパイルし命令列に書き込む
2. `APPEND JUMP_FALSE` — BIF 命令のXtを書き込む
3. `CS_PUSH HERE` — 次に書き込む位置（ジャンプ先プレースホルダーのアドレス）をコンパイルスタックに積む
4. `APPEND 0` — ジャンプ先プレースホルダーとして 0 を書き込む

**ENDIF の動作**（コンパイル時）:
1. `CS_POP` — IF が積んだプレースホルダーのアドレスを取り出す（`PATCH_ADDR CS_POP` の引数として式評価される）
2. `PATCH_ADDR` — そのアドレスに現在の DP（ENDIF 直後の位置）を書き込む

> `PATCH_ADDR CS_POP` は1ステートメントであり、`CS_POP`（式）が先に評価されてアドレスをデータスタックに積み、次に `PATCH_ADDR`（ステートメント）がそのアドレスをポップしてパッチする。

実行時、条件が偽のとき BIF はパッチ済みのジャンプ先（= ENDIF 直後の位置）にジャンプする。

---

## ELSE / ELIF の設計方針

> 未実装。以下は IF...ENDIF の実装パターンを踏まえた設計方針を記録する。

### ELSE

ELSE は「IF ブロックの本体末尾に無条件ジャンプを挿入し、IF の BIF プレースホルダーを ELSE 直後に向けてパッチする」コンパイルワードとして実装する。

コンパイルスタックの遷移:
- IF 実行後: `CS = [A]`（A = BIF のジャンプ先プレースホルダーアドレス）
- ELSE 実行後: `CS = [B]`（B = JUMP_ALWAYS のジャンプ先プレースホルダーアドレス、A はパッチ済み）
- ENDIF 実行後: `CS = []`（B はパッチ済み）

```
DEF ELSE
  APPEND JUMP_ALWAYS       REM emit unconditional jump to skip else-body
  VAR JUMP_PLACEHOLDER
  JUMP_PLACEHOLDER = HERE  REM save JUMP_ALWAYS placeholder address
  APPEND 0                 REM emit placeholder; DP now = else-body start
  PATCH_ADDR CS_POP        REM patch IF's BIF placeholder (A) with current DP
  CS_PUSH JUMP_PLACEHOLDER REM push JUMP_ALWAYS placeholder (B) for ENDIF
END
IMMEDIATE ELSE
```

ENDIF はパッチ対象が JUMP_ALWAYS プレースホルダー（B）に変わるだけで変更不要。

### ELIF

ELIF（elseif）は ELSE と IF を組み合わせた動作を単一ワードで実現する。直前の IF/ELIF のプレースホルダーをパッチしたうえで、新たな条件コードと BIF プレースホルダーをコンパイルスタックに積む。

ELIF は2つのパッチ対象（B: JUMP_ALWAYS プレースホルダー、C: 新しい BIF プレースホルダー）を生成する。どちらも ENDIF 実行時点の DP（= ENDIF 直後の位置）にジャンプする必要がある。

コンパイルスタックの遷移:
- 入力: `CS = [A]`（直前の IF/ELIF の BIF プレースホルダーアドレス）
- 出力: `CS = [B, C]`（B = JUMP_ALWAYS プレースホルダーアドレス、C = 新しい BIF プレースホルダーアドレス）

```
DEF ELIF
  APPEND JUMP_ALWAYS       REM emit unconditional jump to skip this elif-body
  VAR JUMP_PLACEHOLDER
  JUMP_PLACEHOLDER = HERE  REM save JUMP_ALWAYS placeholder address (B)
  APPEND 0                 REM emit placeholder; DP = elif condition check start
  PATCH_ADDR CS_POP        REM patch previous IF/ELIF's BIF placeholder (A) with current DP
  CS_PUSH JUMP_PLACEHOLDER REM push B onto CS (deeper entry)
  COMPILE_EXPR             REM compile new condition expression
  APPEND JUMP_FALSE        REM emit new BIF instruction
  CS_PUSH HERE             REM push new BIF placeholder address (C) on CS (top)
  APPEND 0                 REM emit placeholder
END
IMMEDIATE ELIF
```

ELIF 後の ENDIF は CS から B と C の両方をポップしてパッチする必要がある。このため ENDIF は ELIF が使われる場合に **複数エントリをパッチする拡張** が必要となる。拡張方針として以下の2案がある。

- **カウント方式**: ELIF が JUMP_ALWAYS エントリ数をコンパイルスタックに積んでおき、ENDIF がその数だけ追加でポップしてパッチする。
- **フォワード参照チェーン方式**: 各 JUMP_ALWAYS プレースホルダーが次のプレースホルダーのアドレスを持つリンクリストを形成し、ENDIF がチェーンを辿ってすべてパッチする。

拡張方針の選択については issue #338 でトラッキングする。

> **注意**: 上記のコード例はアルゴリズムの概要を示すものであり、ENDIF の拡張設計（カウント方式 vs チェーン方式）の選択を含め、最終的な TBX 実装は別途決定する必要がある。

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
END
IMMEDIATE WHILE

DEF ENDWH
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
| WHILE 実行後 | `[A, Caddr]`（A が底、Caddr がトップ） |
| ENDWH 実行後 | `[]` |

- `A` = ループ先頭の DictAddr（WHILE が `CS_PUSH HERE` で積む）
- `Caddr` = BIF のジャンプ先プレースホルダーの DictAddr（WHILE が `APPEND 0` の直前に `CS_PUSH HERE` で積む）

### ENDWH の動作トレース

1. `CS_SWAP` — CS を `[A, Caddr]` → `[Caddr, A]` に並び替える（A がトップ）
2. `APPEND JUMP_ALWAYS` — 無条件ジャンプ命令の Xt を辞書に書き込む
3. `APPEND CS_POP` — CS から A（DictAddr）をポップして辞書に書き込む（JUMP_ALWAYS のジャンプ先ターゲット）
4. `PATCH_ADDR CS_POP` — CS から Caddr をポップし、`dictionary[Caddr]` に現在の DP（= ENDWH 直後）を書き込む（BIF のジャンプ先を確定）

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

