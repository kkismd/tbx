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

## Quick Reference

現在実装済みの構文、標準語彙、入出力、サンプルへの実用入口は
[`tbx-quickref.ja.md`](tbx-quickref.ja.md) を参照してください。

## Commands

```sh
cargo build -p tbx-next
cargo test -p tbx-next
cargo run -p tbx-next --bin tbx-next
cargo test --workspace
```

crate 単位の詳細は [`crates/tbx-next/README.md`](../../crates/tbx-next/README.md) を参照してください。

## Examples

数当てゲームは、乱数と対話入力を組み合わせたTBX-Nextの縦断サンプルです。

```sh
cargo run -p tbx-next -- docs/next/examples/guess.tbx
```

整数マンデルブロ集合は、整数演算、反復、条件分岐、`ABS`、`PUTCHR` を
組み合わせた79×25文字のサンプルです。

```sh
cargo run -p tbx-next -- docs/next/examples/mandelbrot.tbx
```

成績区分サンプルは、固定点数列を `FOR` で走査し、各点数を `SELECT` で
A/B/C/D/F に分類して合格件数を集計します。

```sh
cargo run -p tbx-next -- docs/next/examples/grades.tbx
```

[MSBASIC実数版とTinyBASIC整数版](https://kyo-ta04.github.io/memo/)を参照し、
実数版と同じ座標点（`X=-39..39`, `Y=-12..12`）をscale 125の単セル固定小数点で
近似しています。TinyBASIC版のscale 50より細かい量子化を使い、固定小数点乗算では
両オペランドをscaleによる商と余りへ分解します。固定小数点乗算へ渡す各成分を
絶対値250以内に制限するため、余り同士の最大中間積は`124 * 124 = 15376`、
更新値は最大でも1125で、すべて`i16` checked arithmeticの範囲内です。
これは多セル演算による実数版との
完全一致を目的とせず、整数近似版自身の出力を回帰テストで固定しています。
