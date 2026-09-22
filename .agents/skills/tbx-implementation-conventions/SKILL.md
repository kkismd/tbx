---
name: tbx-implementation-conventions
description: TBX リポジトリのコード変更時に、TBX 固有の実装制約と検証手順を適用する。
---

# TBX 実装規約

このskillは、一般的な実装workflowと併用してTBXリポジトリのコードを変更するときに使用する。branch作成、issue対応、commit、push、PR作成の手順は一般workflowに属し、このskillではTBX固有の規約だけを扱う。

## 実装前の確認

- 適用される `AGENTS.md` の指示を読む。
- `crates/tbx-next` 配下を変更する場合は、最初に `docs/next/README.md` を読み、必要に応じて `crates/tbx-next/README.md` を確認する。
- 現行 `tbx` crateを変更する場合は、`docs/agent-notes.md` を読む。
- 実装issueの作成、再評価、着手前レビューでは、`docs/implementation-issue-guidelines.md` を読む。
- Star Trekサンプルまたは関連テストを変更する場合は、`docs/notes/star-trek-mayfield-1972.md` を読む。
- TBX Core Design Principlesを守る。特に、コアを小さく保つこと、直交的なプリミティブを優先すること、可能な限りTBX側で構文を定義すること、実行時値・コンパイル時値・字句トークンを分離すること、ad hocな拡張より再利用可能な抽象化を優先することを確認する。

## リポジトリ固有の制約

- `main` に直接commitせず、人間がレビューするPRを経由する。
- issueごとにPRを分ける。ただしissueが連携した変更を明示的に要求する場合を除く。
- commit messageは日本語で記述する。
- コードコメントは英語で記述する。
- 変更範囲を最小限に保ち、独立したリファクタリングは別の段階へ分ける。
- PRを自分でmergeせず、issueを自分でcloseしない。
- secretは環境変数で管理し、不要な外部呼び出しを避ける。

## 検証

PR作成前に、リポジトリのCIと同等のチェックを実行する。

```sh
cargo ci-clippy
cargo ci-test
cargo ci-fmt
```

実装中は対象crateや対象testに絞ったコマンドを使ってよいが、必須チェックを実行できなかった場合は理由を報告する。

TBXまたはTBX Nextのmilestoneを完了するときは、実装したユーザー向け機能と対象quick referenceの整合性を確認し、必要なら更新する。milestoneの手順は `docs/milestone-process.md` に従う。
