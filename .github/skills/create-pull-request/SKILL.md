---
name: create-pull-request
description: ブランチを切ってコミットし、GitHubのPull Requestを作成する手順。issueに対応した変更をPRとして提出するときに使用する。
---

## Pull Request 作成手順

### 1. ブランチを作成する

issue番号と内容を含む名前でブランチを作成し、チェックアウトする。

```bash
git checkout -b issue/N-short-description
```

- ブランチ名は `issue/番号-内容の要約` の形式にする（例: `issue/2-string-handling`）

### 2. 変更をステージしてコミットする

```bash
git add <ファイル>
git commit -m "コミットメッセージ"
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

```bash
# 一時ファイルにdescriptionを書き出す
cat > /tmp/pr_body.md << 'EOF'
## 概要

（変更の要約をここに書く）

## 変更内容

- 変更点1
- 変更点2

Closes #N
EOF

# --body-file でファイルを渡してPRを作成する
gh pr create \
  --title "PRタイトル" \
  --body-file /tmp/pr_body.md \
  --base main

# 一時ファイルを削除する
rm /tmp/pr_body.md
```

### 5. マージ後のクリーンアップ

PRがマージされたら、mainブランチに戻って最新の状態を取得する。

```bash
git checkout main && git pull
```

### 注意点

- `--body "..."` は使わない（`\n` がエスケープされず改行にならない）
- `Closes #N` をbodyに含めることでissueと自動リンクされる
- マージ後は必ず `git checkout main && git pull` でmainを最新化する
