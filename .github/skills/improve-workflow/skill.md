---
name: improve-workflow
description: ワークフロー改善（エージェント定義・CI設定・命令ファイルなどのメタ設定変更）をブランチ・PRで管理する手順。issueに紐づかない改善作業に使用する。
---

## ワークフロー改善手順

issueの実装と同様、ワークフロー改善も **mainブランチへの直接コミットは禁止**。
必ずブランチを切り、Pull Request経由でマージする。

### 1. ブランチを作成する

作業は**必ず git worktree を使って行う**。main ブランチから worktree を作成し、変更内容を表す名前でブランチを切る。

```bash
# mainが最新であることを確認
git pull

# worktreeを作成してブランチを切る
git worktree add ../tbx-improve-short-description -b improve/short-description
cd ../tbx-improve-short-description
```

- ブランチ名は `improve/内容の要約` の形式にする（例: `improve/agent-tmp-restriction`）
- 分岐元が `main` になっていることを確認する（`git log --oneline -1 main` で確認）

### 2. 変更をステージしてコミットする

コミットメッセージは `$(git rev-parse --git-dir)/COMMIT_MSG` に書き出し、`-F` オプションで渡す。
Gitが管理するパスのため、プロジェクト外の `/tmp` を使う必要はない。

```bash
git add <ファイル>

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
PR bodyは `$(git rev-parse --git-dir)/PR_BODY.md` に書き出す（プロジェクト外の `/tmp` は使わない）。

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

ユーザーからマージ完了の連絡を受けたら、worktreeを削除してmainブランチを最新化する。

```bash
# worktreeの外（メインのリポジトリ）で実行する
git worktree remove ../tbx-improve-short-description
git checkout main && git pull
```

### 注意点

- `--body "..."` は使わない（`\n` がエスケープされず改行にならない）
- 一時ファイルは `$(git rev-parse --git-dir)/` 配下に書く（`/tmp` などプロジェクト外は使わない）
- issueと紐づかない場合は `Closes #N` は不要
- PR作成後すぐにmainへ戻らない。ユーザーのマージ完了報告を待ってから worktree を削除する
