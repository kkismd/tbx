# TBX — Tiny Basic eXtensible

TBX は Tiny BASIC の構文と Forth 的な自己拡張性を組み合わせた処理系です。
Rust で実装された最小限のプリミティブセットを土台として、言語自身を用いて段階的にブートストラップします。

## アーキテクチャの概要

| 要素 | 説明 |
|------|------|
| VM | 間接スレッデッドコード（ITC）方式のスタックマシン |
| 辞書 | ヘッダ層（`Vec<WordEntry>`）とデータ層（`Vec<Cell>`）の二層構造。ユーザー辞書 → 標準ライブラリ辞書 → システム辞書の順に検索（最後に登録されたワードが優先される） |
| スタック | データスタック（SP / BP によるスタックフレーム管理）とリターンスタック |
| コンパイラ | 優先度付き演算子を操車場アルゴリズムで RPN 命令列に変換。制御構造の入れ子をコンパイル時専用スタックで管理 |
| Cell | スタック・辞書の基本単位。`Int` / `Float` / `Bool` / `DictAddr` / `StackAddr` / `Xt` / `StringDesc` 等を保持する Rust enum |

プリミティブはすべて Rust で実装され、システム辞書にワードとして登録されます。
より高レベルな言語機能は TBX 自身で記述されます。

## 言語の特徴

- `命令 オペランド` の統一構文（終端は改行またはセミコロン）
- オペランドのカンマは最低優先度の二項演算子として機能し、複数引数をスタックに積む
- `DEF` / `END` によるワード定義（GOSUB は廃止）
- GOTOや条件分岐命令(BIF/BIT) のジャンプ先はワード本体内のローカル行番号に限定

## ビルドと実行

```sh
cargo build --release
```

```sh
# ファイルを実行
tbx source.tbx

# 標準入力から1行ずつ実行
tbx
```

### サンプル

```basic
DEF HELLO
  PUTSTR "hello\n"
END

HELLO
HALT
```

## ドキュメント

- [`blueprint.md`](./blueprint.md) — VM・辞書のアーキテクチャ設計
- [`blueprint-language.md`](./blueprint-language.md) — コア言語仕様
- [`blueprint-compiler.md`](./blueprint-compiler.md) — コンパイラ設計
