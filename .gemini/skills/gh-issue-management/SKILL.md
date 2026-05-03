---
name: gh-issue-management
description: GitHub Issue の閲覧、検索、作成、およびコメント投稿を行うためのスキル。issue番号を指定して内容を確認したり、特定キーワードで検索したり、実装計画をコメントしたりする際に使用する。
---

## GitHub Issue 操作手順

GitHub CLI (`gh`) を使用して、Issue の管理を行います。

### 1. Issue を一覧表示・検索する

プロジェクト内の Issue を確認したり、特定のラベルやキーワードで検索したりします。

```bash
# 最近のオープンな Issue を 10 件表示
gh issue list --limit 10

# 特定のラベル（例: bug）が付いた Issue を検索
gh issue list --label "bug"

# キーワードで検索
gh issue list --search "keyword"
```

### 2. Issue の詳細を表示する

Issue の本文やコメントを確認します。

```bash
# Issue #N の本文を表示
gh issue view <N>

# Issue #N の本文とすべてのコメントを表示
gh issue view <N> --comments
```

### 3. Issue にコメントを投稿する

実装計画や進捗報告などをコメントとして残します。

```bash
# メッセージを直接指定してコメント
gh issue comment <N> --body "コメント内容"

# ファイルの内容をコメントとして投稿（長い計画などに推奨）
gh issue comment <N> --body-file ".tmp/COMMENT_BODY.md"
```

### 4. Issue を作成する

新しい課題やバグ報告を Issue として登録します。

```bash
# タイトルと本文を指定して作成
gh issue create --title "タイトル" --body "本文"

# ラベルを指定して作成
gh issue create --title "タイトル" --body "本文" --label "bug,enhancement"
```

## Tips

- 一時的なコメント本文などは `.tmp/` 配下にファイルとして書き出し、`--body-file` で渡すと改行などが正確に反映されます。
- 一時ファイルはWriteFileツールで書き出します。
- `gh issue view <N> --json title,body,comments` のように `--json` フラグを使うと、プログラムで扱いやすい形式で取得できます。
