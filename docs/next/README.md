# TBX Next

TBX Next は、現行 `tbx` package と同じリポジトリに置かれた独立 package です。
根拠 ADR は #1358 です。

- 現行実装: ルート package `tbx`
- 次世代実装: `crates/tbx-next` の package `tbx-next`
- 実装事実の正本: コードとテスト
- 非自明な why: ソースコードコメントに記録

## Commands

```sh
cargo build -p tbx
cargo test -p tbx
cargo build -p tbx-next
cargo test -p tbx-next
cargo run -p tbx-next --bin tbx-next
cargo test --workspace
```
