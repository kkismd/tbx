# TBX Next

TBX Next は、現行 `tbx` package と同じリポジトリで開発している次世代実装です。
現時点では初期開発段階であり、完成済み処理系でも、現行 `tbx` の互換実装でもありません。

## Scope

- 現行実装はルート package `tbx` です。
- TBX Next の実装 crate は [`crates/tbx-next/`](../../crates/tbx-next/) です。
- TBX Next は現行 `tbx` に依存せず、現時点では現行版との互換性を保証しません。
- 現行の `blueprint.md`、`blueprint-language.md`、`blueprint-compiler.md` は現行 TBX の設計文書であり、TBX Next の仕様ではありません。

## Source of Truth

- 重要な設計判断の正本は ADR issue です。TBX Next の入口となる判断は [ADR #1358](https://github.com/kkismd/tbx/issues/1358) を参照してください。
- 実装事実と現在の挙動の正本はコードとテストです。
- 非自明な why、不変条件、契約は、対応するソースコードコメントへ記録します。
- 現在進行中または計画中の TBX Next 作業は [Milestone #17 `TBX-Next`](https://github.com/kkismd/tbx/milestone/17) から追跡します。

このディレクトリは TBX Next の入口です。包括的な言語仕様書、VM 設計書、compiler 設計書、関連 issue の固定一覧はここへ置きません。

## Commands

```sh
cargo build -p tbx-next
cargo test -p tbx-next
cargo run -p tbx-next --bin tbx-next
cargo test --workspace
```

crate 単位の詳細は [`crates/tbx-next/README.md`](../../crates/tbx-next/README.md) を参照してください。
