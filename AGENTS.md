# Agent Instructions

このファイルはすべてのAIエージェント（サブエージェント含む）に適用される共通ルールを定義します。

## 動作確認・デバッグの方針

- **`/tmp` などプロジェクト外へのファイル書き込みは禁止**。
- 動作確認やエッジケースの検証のための一時的なコードは、プロジェクト内のテストモジュール（`#[cfg(test)]`）に書き、不要になったら削除すること。

## CIと同等のローカルチェック

コミット前に以下のコマンドを実行してCIと同じ条件でチェックすること。

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo fmt --check
```

**重要**: `--all-targets` を省略すると `#[cfg(test)]` ブロック内のコードが lint 対象から外れ、CIでのみ検出される警告が発生する。

## Git ワークフロー

- **mainブランチへの直接コミットは禁止**。変更は必ずブランチを切り、Pull Request経由でマージすること。
- **PRを自分でマージすることは禁止**。マージはユーザーが行う。
- **issueを自分でクローズすることは禁止**。クローズはユーザーが行う。

## エージェント一覧

| エージェント名 | 役割 |
| --- | --- |
| `plan-issue` | issueを調べて実装方針・変更ファイル・注意点をissueコメントに記録 |
| `spec-discussion` | 仕様の選択肢提示・ユーザーとの対話・issueへの決定記録 |
| `implement-issue` | issueを読んでRustコードを実装しPRを作成 |
| `review-implementation` | PRのコードをレビューし問題をコメント・issueに登録 |
| `blueprint-updater` | `blueprint.md` / `blueprint-language.md` / `blueprint-compiler.md` に設計方針を記録しPRを作成 |

## 言語ルール

- コードのコメントは**英語**で記述する。
- コミットメッセージは**日本語**で記述する。
