---
name: after-merge
description: PRマージ後のローカルワークスペース後片付け。mainを最新化してトピックブランチを削除する。ユーザーがPRのマージ完了を伝えたときに使用する。
---

## PRマージ後の後片付け手順

### 1. 事前チェック

以下の条件をすべて確認する。ひとつでも満たさない場合は**作業をストップしてユーザーに報告する**。

#### 1-1. カレントブランチがトピックブランチであること

```bash
git branch --show-current
```

- `main` が返ってきた場合はストップ（すでにmainにいる）
- トピックブランチ名を `TOPIC_BRANCH` として記録する

#### 1-2. 未コミットの変更がないこと

```bash
git status --porcelain
```

- 出力が空でない場合はストップ（未コミットの変更あり）

#### 1-3. トピックブランチがoriginにプッシュ済みで、未プッシュのコミットがないこと

```bash
git status --short --branch
```

- `ahead` が含まれる場合はストップ（未プッシュのコミットあり）
- `origin/...` のリモート追跡ブランチが存在しない場合はストップ（originにプッシュされていない）

### 2. mainを最新化する

```bash
git switch main
git pull
```

### 3. トピックブランチを削除する

```bash
git branch -d <TOPIC_BRANCH>
```
