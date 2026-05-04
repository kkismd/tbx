---
name: improve-workflow
description: ワークフロー改善（エージェント定義・CI設定・命令ファイルなどのメタ設定変更）をブランチ・PRで管理する手順。skill/agent ファイルを変更した際の description-body 整合チェック（Iteration 0）を含む。issueに紐づかない改善作業に使用する。
---

## ワークフロー改善手順

issueの実装と同様、ワークフロー改善も **mainブランチへの直接コミットは禁止**。
必ずブランチを切り、Pull Request経由でマージする。

### 1. ブランチを作成する

main ブランチから新しいブランチを作成し、変更内容を表す名前でブランチを切る。

```bash
# mainが最新であることを確認
git checkout main && git pull

# ブランチを作成して切り替える
git checkout -b improve/short-description
```

- ブランチ名は `improve/内容の要約` の形式にする（例: `improve/agent-tmp-restriction`）
- 分岐元が `main` になっていることを確認する（`git log --oneline -1 main` で確認）

### 1.5. 変更ファイルの静的チェック（skill / agent ファイルを変更した場合のみ）

変更した SKILL.md / agent.md の frontmatter `description` と body を比較し、乖離がないことを確認する。

チェック観点:
- `description` が謳うトリガー・用途を body がカバーしているか
- body に追加した機能・手順が `description` に反映されているか

乖離がある場合はコミット前に修正する。

> **重要な skill への大幅な変更（新規作成を含む）の場合**は、PR 作成後に
> `empirical-prompt-tuning` skill を実施して実行精度を検証することを検討する。

### 2. 変更をステージしてコミットする

コミットメッセージは `.tmp/COMMIT_MSG` に書き出し、`-F` オプションで渡す。
プロジェクトルートの `.tmp/` はバージョン管理対象外（`.gitignore` 済み）のため、安全に利用できる。

```bash
git add <ファイル>

mkdir -p .tmp
cat > ".tmp/COMMIT_MSG" << 'EOF'
コミットメッセージ本文（日本語）
EOF

git commit -F ".tmp/COMMIT_MSG"
```

### 3. ブランチをpushする

```bash
git push -u origin <ブランチ名>
```

### 4. Pull Requestを作成する

PRのdescriptionは **必ず `--body-file` オプション** でファイルを渡す。
PR bodyは `.tmp/PR_BODY.md` に書き出す。

```bash
mkdir -p .tmp
cat > ".tmp/PR_BODY.md" << 'EOF'
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
  --body-file ".tmp/PR_BODY.md" \
  --base main
```

### 注意点

- `--body "..."` は使わない（`\n` がエスケープされず改行にならない）
- 一時ファイルは `.tmp/` 配下に書く（事前作成が必要な場合は `mkdir -p .tmp` を行う）
- issueと紐づかない場合は `Closes #N` は不要
