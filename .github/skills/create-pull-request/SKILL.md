---
name: create-pull-request
description: ブランチを切ってコミットし、GitHubのPull Requestを作成する手順。issueに対応した変更をPRとして提出するときに使用する。
---

## Pull Request 作成手順

### 1. ブランチを作成する

**必ず `main` ブランチから**ブランチを切る。issue番号と内容を含む名前でブランチを作成し、チェックアウトする。

```bash
git checkout main
git checkout -b issue/N-short-description
```

- ブランチ名は `issue/番号-内容の要約` の形式にする（例: `issue/2-string-handling`）
- **git worktree を使う場合**: `git worktree add ../tbx-worktree -b issue/N-short-description` で作成する。この場合も分岐元が `main` になっていることを確認すること（`git log --oneline -1 main` で確認）。

### 2. 変更をステージしてコミットする

コミットメッセージは `.git/COMMIT_MSG` に書き出し、`-F` オプションで渡す。
`.git/` 配下はGitが自動的に無視するため、プロジェクト外の `/tmp` を使う必要はない。

> **worktree使用時の注意**: worktree 内では `.git` はファイルになるため `cat > .git/COMMIT_MSG` は失敗する。
> その場合は `-m` オプションでメッセージをインラインで渡すか、`$(git rev-parse --git-dir)/COMMIT_MSG` を使う。

```bash
git add <ファイル>

# 通常のリポジトリの場合
cat > .git/COMMIT_MSG << 'EOF'
コミットメッセージ本文

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
EOF
git commit -F .git/COMMIT_MSG

# worktree 内の場合（.git がファイルのため上記が使えない）
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

一時ファイルは `.git/PR_BODY.md` に書き出す（プロジェクト外の `/tmp` は使わない）。

> **worktree使用時の注意**: `cat > .git/PR_BODY.md` は失敗する。`$(git rev-parse --git-dir)/PR_BODY.md` を使うこと。

```bash
# Write PR description to a temp file inside .git/
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

ユーザーからマージ完了の連絡を受けたら、mainブランチに戻って最新の状態を取得する。

```bash
git checkout main && git pull
```

### 注意点

- `--body "..."` は使わない（`\n` がエスケープされず改行にならない）
- 一時ファイルは `.git/` 配下に書く（`/tmp` などプロジェクト外は使わない）
- `.git/` 配下はGitが自動的に無視するため、コミットやgitignoreの心配が不要
- `Closes #N` をbodyに含めることでissueと自動リンクされる
- PR作成後すぐにmainへ戻らない。ユーザーのマージ完了報告を待ってから `git checkout main && git pull` を実行する
