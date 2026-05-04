---
name: impl
description: issue番号を引数に取り、orchestrate-issueエージェントを起動して実装・レビューループを実行するスキル。`/impl #N` の形式で使用する。
---

## 使用方法

```
/impl #N
```

`N` には実装対象のissue番号を指定する。

## 動作

Agent ツールで `subagent_type: "orchestrate-issue"` を指定し、以下のプロンプトでサブエージェントを起動する：

```
issue #<N> を実装してください
```

`orchestrate-issue` エージェントが以下のフローを自動的に実行する：

1. `implement-issue` エージェントによるPR作成
2. `review-implementation` エージェントによるレビュー
3. 🔴/🟡 の指摘があれば `implement-issue` に修正を依頼（最大3回）
4. 修正後に最終レビューを実施
5. 結果をユーザーに報告
