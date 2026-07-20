# TBX Next

TBX Next は、現行 `tbx` package とは独立した次世代実装用 crate です。
この crate は開発中であり、現時点では完成済み処理系でも、現行 `tbx` の互換実装でもありません。

## Crate Boundary

- package: `tbx-next`
- library: `tbx_next`
- binary: `tbx-next`
- source: [`src/`](./src/)
- tests: current unit tests in [`src/lib.rs`](./src/lib.rs)

現行実装はルート package `tbx` です。TBX Next と現行 `tbx` の間に crate dependency はありません。

## Source of Truth

実装事実と現在の挙動の正本はコードとテストです。
設計判断の入口は [ADR #1358](https://github.com/kkismd/tbx/issues/1358) を参照してください。
現在の作業と進捗は [Milestone #17 `TBX-Next`](https://github.com/kkismd/tbx/milestone/17) から追跡します。
非自明な why、不変条件、契約は、対応するソースコードコメントへ記録します。

## Commands

```sh
cargo build -p tbx-next
cargo test -p tbx-next
cargo run -p tbx-next --bin tbx-next
```

TBX Next 全体の案内は [`docs/next/README.md`](../../docs/next/README.md) を参照してください。
