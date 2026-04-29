# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
- **After a PR is merged** — run `git checkout main && git pull --ff-only origin main` to update the local main branch, then `git branch -d <topic-branch>` to delete the topic branch.

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
8. **`src/primitives.rs`** — all built-in `PrimFn` implementations (`register_all`)
9. **`src/interpreter.rs`** — outer interpreter (`Interpreter`); tokenizes source, drives `compile_program` / `exec_source` / `exec_line`

### Dictionary structure

Three logical layers share one flat `Vec<Cell>`:
- **System dictionary** — primitives registered by `register_all`; boundary at `DP_SYS`
- **Library dictionary** — standard library loaded via `USE`; boundary at `DP_LIB`
- **User dictionary** — user-defined words; boundary at `DP_USER`, with `DP` pointing to the next free cell

Headers use a linked-list (`prev: Option<usize>`) for shadowing and `FORGET`-based rollback. The string pool is append-only; `FORGET` rolls back `headers` and `dictionary` but never shrinks the string pool.

### Compilation

Word definitions use `DEF WORD(params) ... END`. `CompileState` in `src/vm.rs` tracks the in-progress compilation: parameter/local variable table, back-patch lists for GOTO labels and self-recursive `CALL` instructions, and rollback info for error recovery. Expression compilation is handled by `ExprCompiler` in `src/expr.rs`.

### Integration tests

`build.rs` generates one `#[test]` per `lib/tests/test_*.tbx` file and writes them to `$OUT_DIR/tbx_lib_tests_generated.rs`, which is `include!`-ed by `tests/tbx_lib_tests.rs`. To add a TBX-level test, add a `test_<name>.tbx` file to `lib/tests/`; no Rust code changes needed.

### Design documents

- `blueprint.md` — VM architecture, dictionary structure, memory layout
- `blueprint-language.md` — language syntax, statements, expressions, types
- `blueprint-compiler.md` — `DEF`/`END`, control structures, compile-time stack primitives

`blueprint.md` records design decisions and specifications only; stable implementation details live in `src/`, not in blueprint docs.

## Agent roles

| Agent | Role |
|---|---|
| `plan-issue` | Reads issue, records implementation plan as issue comment |
| `spec-discussion` | Discusses spec options, records decisions to issue |
| `implement-issue` | Implements from issue, creates PR |
| `review-implementation` | Reviews PR, posts review comments / opens issues |
| `blueprint-updater` | Updates `blueprint*.md` files and creates PR |

## Communication

- ユーザーとの対話や思考内容の発信は日本語で出力してください。
