# Agent Instructions

このファイルはすべてのAIエージェント（サブエージェント含む）に適用される共通ルールを定義します。

## 実装前に読むべきドキュメント

- **[`docs/agent-notes.md`](docs/agent-notes.md)** — 実装上の注意・落とし穴・レビュー由来の知見をまとめた日本語の共有ノート。構文の落とし穴・配列 convention・ブランチ運用など、実装前に確認すること。
- **[`docs/notes/star-trek-mayfield-1972.md`](docs/notes/star-trek-mayfield-1972.md)** — STTR1 実装時に参照する Mayfield 原典のルール抽出メモ。

## TBX Core Design Principles (最優先)

TBX は、実用的な言語機能を拡充しながらも、コアを小さく、直交的で、拡張可能に保つことを目指す。

新機能やリファクタを提案・実装するときは、既存の一般的な設計原則より先に、以下の TBX 固有のコア設計原則を適用すること。

1. **小さなコアを優先する**
   Rust 側に追加する機能は、複数の構文やライブラリ機能から再利用できる VM / compile-time VM 操作に限定する。特定の高水準構文だけを直接実装する専用プリミティブは避ける。

2. **直交的なプリミティブを優先する**
   大きな構文専用プリミティブより、小さく合成可能なプリミティブを優先する。新しいプリミティブは、特定の構文名を知らなくても説明できることを目安にする。

3. **構文は可能な限り TBX 側で定義する**
   新しい構文機能は、可能な限り TBX 側の IMMEDIATE ワードまたは標準ライブラリとして実装する。Rust 側は、それらを実現するための低レベルで再利用可能な語彙を提供する。

4. **実行時値・コンパイル時値・字句トークンを混ぜない**
   `Cell`、`CompileEntry`、`Token` / `SpannedToken` の責務を明確に分ける。便利さのために lexer token を実行時値として data stack に流すような設計は避ける。

5. **ad hoc な拡張より抽象化を優先する**
   `SKIP_EQ`、`SKIP_COMMA` のような個別処理を増やす前に、`EXPECT_TOKEN`、`EXPECT_OP`、`NEXT_TOKEN` のような再利用可能な抽象で表せないかを検討する。

6. **便利プリミティブには理由を求める**
   特定用途向けの便利プリミティブを追加する場合は、既存または新規の低レベル語彙の組み合わせでは十分に表せない理由を、issue / PR / コメントのいずれかに明記する。

7. **外側は BASIC、内側は小さな拡張可能コアに保つ**
   ユーザー向けの表面構文は BASIC らしく実用的でよい。ただし内部実装は、Forth 的な自己拡張性と小さな VM コアを保つ方向に整理する。

## Architecture

TBX is a Tiny BASIC interpreter with Forth-like self-extension capabilities. The design follows a bootstrapped VM with Indirect Threaded Code (ITC).

### Execution model

The VM (`src/vm.rs`) is the core. It holds:
- `dictionary: Vec<Cell>` — flat code/data array (the data layer)
- `headers: Vec<WordEntry>` — word name/flag/kind table (the header layer)
- `data_stack: Vec<Cell>` — argument passing and computation
- `return_stack: Vec<ReturnFrame>` — call/return with saved PC and BP

`Xt` (`src/cell.rs`) is a typed index into `headers`, not into `dictionary`. `EntryKind` in `src/dict.rs` determines how a word is executed: `Primitive(PrimFn)`, `Word(usize)` (dictionary offset), `Variable(usize)`, `Constant(Cell)`, or VM-internal instructions (`Call`, `Exit`, `ReturnVal`, `BranchIfFalse`, etc.).

The inner interpreter (`VM::exec_xt`) reads a sequence of `Xt` values, dispatches through `EntryKind`, and handles control flow. `ReturnFrame::TopLevel` is the sentinel used to terminate top-level execution cleanly.

### Entry point

`lib::init_vm()` creates a VM, calls `primitives::register_all()` to populate the system dictionary, then calls `vm.seal_sys()` to record the system boundary (`DP_SYS`).

### Layers

1. **`src/cell.rs`** — `Cell` (the value union: `Int`, `Float`, `DictAddr`, `StackAddr`, `Str`, `Marker`, `Xt`), `Xt`, `ReturnFrame`, `CompileEntry`
2. **`src/constants.rs`** — VM limits: `MAX_DICTIONARY_CELLS` (1M), `MAX_DATA_STACK_DEPTH` (65536), `MAX_RETURN_STACK_DEPTH` (4096)
3. **`src/dict.rs`** — `WordEntry`, `EntryKind`, dictionary flags (`FLAG_SYSTEM`, `FLAG_IMMEDIATE`)
4. **`src/error.rs`** — `TbxError` enum (all VM and compiler errors)
5. **`src/lexer.rs`** — tokenizer; produces `Token` / `SpannedToken`
6. **`src/expr.rs`** — expression compiler using the Shunting-Yard Algorithm; converts infix expressions to RPN `Vec<Cell>`
7. **`src/vm.rs`** — `VM` struct, `CompileState` (active DEF..END state), inner interpreter
8. **`src/primitives.rs` / `src/primitives/`** — built-in `PrimFn` implementations. `src/primitives.rs` remains the façade and registration entry point (`register_all`); low-dependency category modules may live under `src/primitives/` (e.g. `stack.rs`, `numeric.rs`, `logic.rs`).
9. **`src/interpreter.rs`** — outer interpreter (`Interpreter`); tokenizes source, drives `compile_program` / `exec_source` / `exec_line`

### Dictionary structure

Three logical layers share one flat `Vec<Cell>`:
- **System dictionary** — primitives registered by `register_all`; boundary at `DP_SYS`
- **Library dictionary** — standard library loaded via `USE`; boundary at `DP_LIB`
- **User dictionary** — user-defined words; boundary at `DP_USER`, with `DP` pointing to the next free cell

Headers use a linked-list (`prev: Option<usize>`) for shadowing and lookup order. Within a session, `headers` and `dictionary` entries grow monotonically, except for the internal rollback that undoes a partially-compiled definition when `DEF ... END` fails. Arrays are named mutable storage created by `DIM @A[n]`; they are not surface first-class values. Internally they are represented as `Cell::Array(ArrayRef)`, where `ArrayRef` is an `Rc<RefCell<Vec<Cell>>>` handle. Array lifetime is managed by `Rc` reference counting: an array is freed when all `Cell::Array` and `Cell::ArrayAddr` handles to it go out of scope. No pool-boundary-based lifetime management is needed. `Cell::ArrayAddr { array: ArrayRef, elem_idx }` holds an `ArrayRef` directly so that `FETCH` / `STORE` can access elements without indirection. Strings are represented as `Cell::Str(Rc<str>)` — reference-counted immutable handles — and also need no pool-based lifetime management; a `Cell::Str` may be safely shared across the data stack, variable slots, and array elements. A full reset is achieved by re-creating the VM or by recompacting from source. String literals are compiled as `Cell::Str(Rc<str>)` directly into the dictionary.

### Compilation

Word definitions use `DEF WORD(params) ... END`. `CompileState` in `src/vm.rs` tracks the in-progress compilation: parameter/local variable table, back-patch lists for GOTO labels and self-recursive `CALL` instructions, and rollback info for error recovery. Expression compilation is handled by `ExprCompiler` in `src/expr.rs`.

### Integration tests

`build.rs` generates one `#[test]` per `lib/tests/test_*.tbx` file and writes them to `$OUT_DIR/tbx_lib_tests_generated.rs`, which is `include!`-ed by `tests/tbx_lib_tests.rs`. To add a TBX-level test, add a `test_<name>.tbx` file to `lib/tests/`; no Rust code changes needed.

### Design documents

- `blueprint.md` — VM architecture, dictionary structure, memory layout
- `blueprint-language.md` — language syntax, statements, expressions, types
- `blueprint-compiler.md` — `DEF`/`END`, control structures, compile-time stack primitives
- `docs/tbx-quickref.ja.md` — TBX プログラムを書く人間およびエージェント向けの実用クイックリファレンス

`blueprint.md` records design decisions and specifications only; stable implementation details live in `src/`, not in blueprint docs.

When writing or modifying TBX programs, consult `docs/tbx-quickref.ja.md` first for common syntax, standard vocabulary, and agent-facing pitfalls. The implementation remains the source of truth; check `src/`, `lib/`, and tests for details, edge cases, and current behavior.


## Core Principles (必ず遵守)
このプロジェクトでは以下の7原則を最優先とする。これらを無視したコードは拒否・修正する。

1. **カプセル化**
   内部状態や実装詳細を外部に露出させない。公開APIは最小限とし、データは適切な境界で隠蔽する。

2. **関心の分離**
   ドメインロジック、技術的詳細、インフラ、プレゼンテーションを明確に分離する。一つの関数・クラス・モジュールは一つの役割を持つ。

3. **契約による設計 (Design by Contract)**
   関数・メソッドには事前条件 (Preconditions) と事後条件 (Postconditions) を明確に定義する。
   - 入力検証を厳格に行い、無効な状態を早期に拒否。
   - テストでは契約を検証する。

4. **副作用の隔離**
   副作用 (DBアクセス、外部API呼出、状態変更など) は純粋関数から分離し、専用レイヤー (Repository/Service/UseCase) に閉じ込める。純粋関数は常に同じ入力で同じ出力となる。

5. **ドメイン駆動設計 (Domain-Driven Design)**
   ビジネスドメインを中心にモデル化する。ユビキタス言語を徹底し、境界付けられたコンテキスト (Bounded Context) を明確にする。エンティティ・バリューオブジェクト・集約を適切に定義し、ドメイン知識をコードに反映する。

6. **ステートマシンによる状態管理**
   重要な状態遷移は明示的なステートマシンで管理する。無効な状態遷移を防止し、状態の整合性を保証。状態は可能な限り宣言的に記述する。

7. **ハッピーパス優先**
   正常系 (ハッピーパス) を最初に明確に設計・実装し、エラーケース・エッジケースは後から追加する。メインのフローをシンプルに保ち、例外処理を分離して全体の可読性を高める。

## 必須運用ルール
- **人間レビュー必須**: すべての変更はdraft PRとして生成し、人間がレビューするまで本番ブランチにマージしない。
- **変更は最小単位で**: 大規模リファクタは複数ステップに分け、各ステップでテストを通す。
- **テスト優先**: 新機能・修正時は契約を反映したテストを最初に作成・実行する。
- **コスト・セキュリティ意識**: シークレットは環境変数のみ、不要な外部呼出を避け、トークン消費を意識した簡潔なコードを書く。

## Quality Checklist (コード生成後に必ず自己確認)
- [ ] 7原則 (カプセル化・関心の分離・契約・副作用隔離・ドメイン駆動・ステートマシン・ハッピーパス優先) をすべて満たしているか？
- [ ] 関数・クラスは単一責務か？境界コンテキストは明確か？
- [ ] 事前/事後条件の検証が適切に入っているか？
- [ ] 副作用は専用レイヤーに隔離されているか？
- [ ] 重要な状態はステートマシンで管理されているか？
- [ ] ハッピーパスが明確で優先的に実装され、エラーケースは分離されているか？
- [ ] テストで契約および状態遷移が検証可能か？
- [ ] コードは読みやすく、ボイラーコード・コメントが必要最小限か？

## プロジェクト固有の追加指示
(ここに個別プロジェクトごとの内容を記述)
- ディレクトリ構造の説明
- コーディング指針 (言語特有の規約)
- コマンド帳 (例：テスト実行コマンド、ビルドコマンド)
- その他 (使用フレームワークの制約、禁止事項など)

**注意**: 上記原則に違反する提案は、理由を明記して修正案を提示すること。

## Commands

```bash
# Build
cargo build

# Run tests (includes both Rust unit tests and .tbx integration tests)
cargo test

# Run a single test by name
cargo test test_name

# Lint (must pass with zero warnings — use --all-targets to catch #[cfg(test)] code)
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check

# Fix formatting
cargo fmt

# Run the interpreter on a file
cargo run -- path/to/file.tbx

# Run the interpreter in REPL mode (reads from stdin)
cargo run
```

## Git Workflow

- **Never commit directly to `main`** — all changes must go through a branch and Pull Request.
- **Never merge your own PRs** — leave that to the user.
- **Never close issues yourself** — leave that to the user.
- Commit messages must be in **Japanese**.
- Code comments must be in **English**.
- **After a PR is merged** — switch back to the PR base branch, update it, and safely delete the merged topic branch with `git branch -d`.

## 動作確認・デバッグの方針

- **`/tmp` などプロジェクト外へのファイル書き込みは禁止**。
- 動作確認やエッジケースの検証のための一時的なコードは、プロジェクト内の一時ディレクトリ（`.tmp/`）に書き、不要になったら削除すること。

## CIと同等のローカルチェック

コミット前に以下のコマンドを実行してCIと同じ条件でチェックすること。

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo fmt --check
```

**重要**: `--all-targets` を省略すると `#[cfg(test)]` ブロック内のコードが lint 対象から外れ、CIでのみ検出される警告が発生する。

## ユーザーへの確認ルール

- ユーザーにトレードオフを伴う選択を求めるときは、**先に選択肢の pros/cons を会話中または issue コメントとして提示**してから選択 UI を表示すること（理由: ユーザーが根拠なしに選択を迫られる）

## ユーザーとのコミュニケーション

- 「鋭い指摘ですね！」など反射的にユーザーを肯定するような表現は避けてください。
- ユーザーの質問に対して、わからない場合は「わかりません」と正直に答えてください。
- 「劇的に改善されます」「完璧に動作します」などの過剰な表現は避けてください。

## PRレビュー運用

- サブエージェントやローカルレビューで **具体的な指摘**（バグ、回帰、テスト不足、仕様不一致など）が見つかった場合、最終的な承認相当コメントだけで済ませず、**問題内容と修正方針がユーザーに追える形でPRコメントへ残すこと**。
- 少なくとも次をPRコメントに含めること。
  - 何が問題だったか
  - どういう条件で再現・影響するか
  - どういう方針で修正したか
  - どの確認を再実行したか
- 内部のエージェント間メッセージだけで重要なレビュー論点を完結させないこと。ユーザーが PR 上の記録だけで判断できる状態を優先すること。
