# TBX-Next

TBX-Next は TBX の現在の主実装です。ルート package `tbx` の旧実装とは独立しており、旧TBXとの互換性を保証しません。

## Crate Boundary

- package: `tbx-next`
- library: `tbx_next`
- binary: `tbx-next`
- source: [`src/`](./src/)
- tests: current unit tests in [`src/lib.rs`](./src/lib.rs)

旧実装のルート package は `tbx` です。TBX-Next と旧TBXの間に crate dependency はありません。

## Source of Truth

実装事実と現在の挙動の正本はコードとテストです。
TBX-Nextを主実装とする設計判断は [ADR #2013](https://github.com/kkismd/tbx/issues/2013)、開発開始時の履歴は [#1358](https://github.com/kkismd/tbx/issues/1358) を参照してください。
非自明な why、不変条件、契約は、対応するソースコードコメントへ記録します。

## Commands

```sh
cargo build -p tbx-next
cargo test -p tbx-next
cargo run -p tbx-next --bin tbx-next
```

## 6502 assembly smoke checks

The explicit sim65 smoke check requires the cc65 toolchain (`ca65`, `ld65`,
`sim65`, and `sim6502.lib`). On Ubuntu, install it with `sudo apt-get install
cc65`. Run the dedicated checks with:

```sh
cargo test -p tbx-next --test sim65_smoke -- --ignored
```

The regular `cargo test -p tbx-next` and `cargo run -p tbx-next` commands do not
invoke cc65. The dedicated check fails with a tool-specific diagnostic if a
required executable is unavailable.

TBX Next 全体の案内は [`docs/next/README.md`](../../docs/next/README.md) を参照してください。

## ランタイムワード定義の局所参照名

ランタイムワード定義では、body から参照する call-base の値に名前を付けられます。

```text
DEF AREA width, height
  EVAL width * height
END
```

名前は call-base の下位から上位の順に記述し、大文字・小文字を区別しません。この列は
コンパイル時の参照情報であり、runtime arity の宣言や return 時の引数消費は行いません。
局所参照名を持たない `DEF FOO` も引き続き使用できます。
