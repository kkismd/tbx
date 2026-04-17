# TBX  Tiny Basic eXtensible

## 開発環境セットアップ

### git worktree 用ディレクトリのVSCode信頼設定

このプロジェクトではエージェントが `../.tbx-worktrees/` 配下にgit worktreeを作成します。
VSCodeがワークスペース外のフォルダを開く際にセキュリティ確認ダイアログを表示するため、
事前に以下の設定を追加してください。

**VSCodeのユーザー設定 (`settings.json`) に追記:**

```json
"security.workspace.trust.trustedFolders": [
  "/path/to/your/src/Rust/.tbx-worktrees"
]
```

`/path/to/your/src/Rust/` は実際の配置場所に合わせて変更してください。
この設定により、worktree作業中のセキュリティ確認ダイアログが表示されなくなります。
