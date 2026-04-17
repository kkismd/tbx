---
name: improve-workflow
description: ワークフロー改善（エージェント定義・CI設定・命令ファイルなどのメタ設定変更）をブランチ・PRで管理する手順。issueに紐づかない改善作業に使用する。
---

## ワークフロー改善手順

issueの実装と同様、ワークフロー改善も **mainブランチへの直接コミットは禁止**。
必ずブランチを切り、Pull Request経由でマージする。

### 1. ブランチを作成する

**必ず `main` ブランチから**ブランチを切る。変更内容を表す名前でブランチを作成し、チェックアウトする。

```bash
git checkout main
git checkout -b improve/short-description
```

- ブランチ名は `improve/内容の要約` の形式にする（例: `improve/agent-tmp-restriction`）
- **git worktree を使う場合**: `git worktree add ../tbx-worktree -b improve/short-description` で作成する。この場合も分岐元が `main` になっていることを確認すること（`git log --oneline -1 main` で確認）。

### 2. 変更をステージしてコミットする

コミットメッセージは `.git/COMMIT_MSG` に書き出し、`-F` オプションで渡す。

> **worktree使用時の注意**: worktree 内では `.git` はファイルになるため `cat > .git/COMMIT_MSG` は失敗する。
> その場合は `$(git rev-parse --git-dir)/COMMIT_MSG` を使う。

```bash
git add <ファイル>

# 通常のリポジトリの場合
cat > .git/COMMIT_MSG << 'EOF'
コミットメッセージ本文（日本語）

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
EOF
git commit -F .git/COMMIT_MSG

# worktree 内の場合（.git がファイルのため上記が使えない）
cat > "$(git rev-parse --git-dir)/COMMIT_MSG" << 'EOF'
コミットメッセージ本文（日本語）

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
EOF
git commit -F "$(git rev-parse --git-dir)/COMMIT_MSG"
```

### 3. ブランチをpushする

```bash
git push -u origin <ブランチ名>
```

### 4. Pull Requestを作成する

PRのdescriptionは **必ず `--body-file` オプション** でファイルを渡す。
一時ファイルは `.git/PR_BODY.md` に書き出す（プロジェクト外の `/tmp` は使わない）。

> **worktree使用時の注意**: `cat > .git/PR_BODY.md` は失敗する。`$(git rev-parse --git-dir)/PR_BODY.md` を使うこと。

```bash
cat > "$(git rev-parse --git-dir)/PR_BODY.md" << 'EOF'
## 概要

（変更の背景・目的）

## 変更内容

- 変更点1
- 変更点2

## 解決策の根拠

（なぜこのアプローチを選んだか）
EOF

gh pr create \
  --title "改善内容のタイトル（日本語）" \
  --body-file "$(git rev-parse --git-dir)/PR_BODY.md" \
  --base main
```

### 5. PRのレビュー待機

PR作成後はトピックブランチにとどまり、ユーザーのマージ完了の報告を待つ。
mainへ戻るのはユーザーから「マージしました」と連絡があってからにする。

### 6. マージ後のクリーンアップ

ユーザーからマージ完了の連絡を受けたら、mainブランチに戻って最新の状態を取得する。

```bash
git checkout main && git pull
```

### 注意点

- `--body "..."` は使わない（`\n` がエスケープされず改行にならない）
- 一時ファイルは `.git/` 配下に書く（`/tmp` などプロジェクト外は使わない）
- issueと紐づかない場合は `Closes #N` は不要
- PR作成後すぐにmainへ戻らない。ユーザーのマージ完了報告を待ってから `git checkout main && git pull` を実行する
