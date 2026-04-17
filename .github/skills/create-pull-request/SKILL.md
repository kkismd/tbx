---
name: create-pull-request
description: ブランチを切ってコミットし、GitHubのPull Requestを作成する手順。issueに対応した変更をPRとして提出するときに使用する。
---

## Pull Request 作成手順

### 1. ブランチを作成する

作業は**必ず git worktree を使って行う**。main ブランチから worktree を作成し、issue番号と内容を含む名前でブランチを切る。

```bash
# mainが最新であることを確認
git pull

# 初回のみ: worktree専用ディレクトリを作成する
mkdir -p ../.tbx-worktrees

# worktreeを作成してブランチを切る
git worktree add ../.tbx-worktrees/issue-N-short-description -b issue/N-short-description
cd ../.tbx-worktrees/issue-N-short-description
```

- ブランチ名は `issue/番号-内容の要約` の形式にする（例: `issue/2-string-handling`）
- 分岐元が `main` になっていることを確認する（`git log --oneline -1 main` で確認）

### 2. 変更をステージしてコミットする

コミットメッセージは `$(git rev-parse --git-dir)/COMMIT_MSG` に書き出し、`-F` オプションで渡す。
Gitが管理するパスのため、プロジェクト外の `/tmp` を使う必要はない。

```bash
git add <ファイル>

cat > "$(git rev-parse --git-dir)/COMMIT_MSG" << 'EOF'
コミットメッセージ本文

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
EOF

git commit -F "$(git rev-parse --git-dir)/COMMIT_MSG"
```

コミットメッセージの形式:
- 日本語で記述する
- 末尾に必ず以下のtrailerを含める:

```
Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
```

### 3. ブランチをpushする

```bash
git push -u origin <ブランチ名>
```

### 4. Pull Requestを作成する

PRのdescriptionは **必ず `--body-file` オプション** でファイルを渡す。
`--body` に文字列を直接渡すと `\n` がリテラルのまま送信されてしまい、改行が反映されない。

PR bodyは `$(git rev-parse --git-dir)/PR_BODY.md` に書き出す（プロジェクト外の `/tmp` は使わない）。

```bash
cat > "$(git rev-parse --git-dir)/PR_BODY.md" << 'EOF'
## 概要

（変更の要約をここに書く）

## 変更内容

- 変更点1
- 変更点2

Closes #N
EOF

# Pass the file with --body-file
gh pr create \
  --title "PRタイトル" \
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
WORKTREE=../.tbx-worktrees/issue-N-short-description
if [ -n "$(git -C "$WORKTREE" status --short)" ]; then
  echo "未コミットの変更があります。確認してください:"
  git -C "$WORKTREE" status --short
  echo "問題なければ手動で: git worktree remove --force $WORKTREE"
else
  git worktree remove "$WORKTREE"
fi
git checkout main && git pull
```

### 注意点

- `--body "..."` は使わない（`\n` がエスケープされず改行にならない）
- 一時ファイルは `$(git rev-parse --git-dir)/` 配下に書く（`/tmp` などプロジェクト外は使わない）
- `Closes #N` をbodyに含めることでissueと自動リンクされる
- PR作成後すぐにmainへ戻らない。ユーザーのマージ完了報告を待ってから worktree を削除する
