---
name: create-pull-request
description: ブランチを切ってコミットし、GitHubのPull Requestを作成する手順。issueに対応した変更をPRとして提出するときに使用する。
---

## Pull Request 作成手順

> **重要**: issueを確認したら、ファイルの読み込み・編集を始める**前に**まずブランチを作成すること。
> この指示の目的は **main への直接コミットを防ぐこと**である。issue内容の把握のためにファイルを読んでしまっていた場合でも、コード編集を始める前にブランチを作成すれば継続してよい。

### 1. ブランチを作成する

main ブランチから新しいブランチを作成し、issue番号と内容を含む名前でブランチを切る。

```bash
# mainが最新であることを確認
git checkout main && git pull

# ブランチを作成して切り替える
git checkout -b issue/N-short-description
```

- ブランチ名は `issue/番号-内容の要約` の形式にする（例: `issue/2-string-handling`）
- `番号-内容の要約` の `内容の要約` 部分は英語のkebab-caseにする（例: `fix-parser-bug`, `extend-tokenizer`）
- 分岐元が `main` になっていることを確認する（`git log --oneline -1 main` で確認）

### 2. 変更をステージしてコミットする

コミットメッセージは `.tmp/COMMIT_MSG` に書き出し、`-F` オプションで渡す。
Gitが管理するパスのため、プロジェクト外の `/tmp` を使う必要はない。

```bash
git add <ファイル>

cat > ".tmp/COMMIT_MSG" << 'EOF'
〇〇を修正する

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
EOF

git commit -F ".tmp/COMMIT_MSG"
```

コミットメッセージの形式:
- 件名は日本語の動詞文にする（例：「〇〇を修正する」「〇〇を追加する」）
- `fix:` / `feat:` などの conventional commits prefix は付けない
- issue番号（`#N` や `issue #N:`）を件名に含めない（issue参照はPR bodyの `Closes #N` のみ）
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

PRのtitleは**コミット件名と同一**にする（日本語、prefixなし、issue番号なし）。

PR bodyは `.tmp/PR_BODY.md` に書き出す（プロジェクト外の `/tmp` は使わない）。

```bash
cat > ".tmp/PR_BODY.md" << 'EOF'
## 概要

（issueの内容を参考に変更の目的・背景を1〜2文で書く）

## 変更内容

- 変更点1
- 変更点2

Closes #N
EOF

# Pass the file with --body-file
gh pr create \
  --title "〇〇を修正する" \
  --body-file ".tmp/PR_BODY.md" \
  --base main
```

### 注意点

- `--body "..."` は使わない（`\n` がエスケープされず改行にならない）
- 一時ファイルは `.tmp/` 配下に書く（`/tmp` などプロジェクト外は使わない）
- `Closes #N` をbodyに含めることでissueと自動リンクされる
