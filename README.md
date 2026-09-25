# TBX — Tiny Basic eXtensible

TBX は Tiny BASIC の実用的な構文と、Forth 的な自己拡張性を備える処理系です。現在の主実装は [`crates/tbx-next/`](./crates/tbx-next/) の TBX-Next です。

## TBX-Next

TBX-Next のコードとテストが現在の実装事実の正本です。設計上の判断は採用済みADRを参照してください。位置づけ、実用的なクイックリファレンス、サンプル、開発コマンドへの入口は [`docs/next/README.md`](./docs/next/README.md) にあります。

```sh
cargo build -p tbx-next
cargo test -p tbx-next
cargo run -p tbx-next --bin tbx-next -- docs/next/examples/guess.tbx
```

## 旧TBX

ルート package `tbx` は旧実装です。通常の新機能開発や仕様変更の対象ではありません。旧実装のコードと利用方法はリポジトリ内に残っていますが、現在のTBXの仕様や主実装を示すものではありません。

旧TBXに関する設計資料は [`blueprint.md`](./blueprint.md)、[`blueprint-language.md`](./blueprint-language.md)、[`blueprint-compiler.md`](./blueprint-compiler.md)、[`docs/tbx-6502-profile.md`](./docs/tbx-6502-profile.md) を参照してください。
