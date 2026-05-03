---
name: create-pull-request
description: ブランチの作成、コミット、および GitHub Pull Request の作成を行うためのスキル。issue に対応した変更を PR として提出する際に使用する。
---

## Pull Request 作成手順

> **重要**: issue を確認したら、ファイルを編集する**前に**必ずブランチを作成してください。
> これは `main` ブランチへの直接コミットを避けるための必須手順です。

### 1. ブランチを作成する

`main` ブランチから最新の状態を取得し、issue 番号と内容を含む名前でブランチを作成します。

```bash
# main を最新にする
git checkout main && git pull

# ブランチを作成して切り替える
git checkout -b issue/N-short-description
```

- ブランチ名形式: `issue/番号-内容の要約`（例: `issue/329-fix-line-number`）
- `内容の要約` は英語の kebab-case にしてください。

### 2. 変更をコミットする

コミットメッセージは `.tmp/COMMIT_MSG` ファイルに書き出し、`git commit -F` で使用します。

```bash
git add <変更したファイル>
```

`write_file` ツールで `.tmp/COMMIT_MSG` を作成し、日本語でメッセージを記述します。

```bash
git commit -F ".tmp/COMMIT_MSG"
```

- **件名:** 日本語の動詞文（例：「〇〇を修正する」）
- **Prefix:** `fix:` や `feat:` は不要です。
- **Issue番号:** 件名には含めず、PR の本文で `Closes #N` を使用してリンクさせます。

### 3. ブランチを push する

```bash
git push -u origin <ブランチ名>
```

### 4. Pull Request を作成する

PR の本文（body）は必ず `--body-file` を使用してください。

1. `write_file` ツールで `.tmp/PR_BODY.md` を以下の形式で作成します。

```markdown
## 概要

（変更の目的や背景を簡潔に記述）

## 変更内容

- 変更点1
- 変更点2

Closes #N
```

2. `gh pr create` を実行します。

```bash
gh pr create \
  --title "（コミット件名と同じ）" \
  --body-file ".tmp/PR_BODY.md" \
  --base main
```

## 注意点

- `--body` フラグに直接文字列を渡すと改行が崩れるため、常に `--body-file` を使用してください。
- 一時ファイルは `.tmp/` 配下に作成してください（`.gitignore` で除外済みです）。
