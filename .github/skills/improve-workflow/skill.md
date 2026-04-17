---
name: improve-workflow
description: ワークフロー改善（エージェント定義・CI設定・命令ファイルなどのメタ設定変更）をブランチ・PRで管理する手順。issueに紐づかない改善作業に使用する。
---

## ワークフロー改善手順

issueの実装と同様、ワークフロー改善も **mainブランチへの直接コミットは禁止**。
必ずブランチを切り、Pull Request経由でマージする。

### 1. ブランチを作成する

変更内容を表す名前でブランチを作成し、チェックアウトする。

```bash
git checkout -b improve/short-description
```

- ブランチ名は `improve/内容の要約` の形式にする（例: `improve/agent-tmp-restriction`）

### 2. 変更をステージしてコミットする

コミットメッセージは `.git/COMMIT_MSG` に書き出し、`-F` オプションで渡す。

```bash
git add <ファイル>

cat > .git/COMMIT_MSG << 'EOF'
コミットメッセージ本文（日本語）

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
EOF

git commit -F .git/COMMIT_MSG
```

### 3. ブランチをpushする

```bash
git push -u origin <ブランチ名>
```

### 4. Pull Requestを作成する

PRのdescriptionは **必ず `--body-file` オプション** でファイルを渡す。
一時ファイルは `.git/PR_BODY.md` に書き出す（プロジェクト外の `/tmp` は使わない）。

```bash
cat > .git/PR_BODY.md << 'EOF'
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
  --body-file .git/PR_BODY.md \
  --base main
```

### 5. マージ後のクリーンアップ

PRがマージされたら、mainブランチに戻って最新の状態を取得する。

```bash
git checkout main && git pull
```

### 注意点

- `--body "..."` は使わない（`\n` がエスケープされず改行にならない）
- 一時ファイルは `.git/` 配下に書く（`/tmp` などプロジェクト外は使わない）
- issueと紐づかない場合は `Closes #N` は不要
- マージ後は必ず `git checkout main && git pull` でmainを最新化する
