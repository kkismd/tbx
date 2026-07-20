# TBX Next

TBX Next は、現行 `tbx` package とは独立した次世代実装用 crate です。
この crate は ADR #1358 に基づくリポジトリ境界を置くための最小コードベースであり、現時点では VM、lexer、parser、compiler、辞書、値型、スタック、変数、ワード定義などの言語機能を実装していません。

現行実装はルート package `tbx` です。TBX Next は package `tbx-next`、library `tbx_next`、binary `tbx-next` として分離されています。新旧 crate 間の dependency はありません。

## Source of Truth

実装事実の正本はコードとテストです。設計判断の根拠は ADR #1358 を参照し、非自明な why は実装時にソースコードコメントへ記録します。

## Commands

```sh
cargo build -p tbx-next
cargo test -p tbx-next
cargo run -p tbx-next --bin tbx-next
```
