---
name: after-merge
description: PRマージ後のローカルワークスペース後片付け。トピックブランチからベースブランチに戻り、トピックブランチを削除する。`/after-merge` でマージ済みPRのbase refを自動検出。`/after-merge <ベースブランチ>` で手動指定。ユーザーがPRのマージ完了を伝えたときに使用する。
---

## 使用方法

```
/after-merge                  # マージ済みPRの base ref を自動検出
/after-merge <ベースブランチ> # 戻り先ブランチを明示的に指定
```

中間トランクへのマージ（例: `phase5b-rc-str` ← topic）にも対応するため、戻り先は `main` 決め打ちではなく PR の base ref から決める。

## PRマージ後の後片付け手順

### 1. トピックブランチを記録する

```bash
TOPIC_BRANCH=$(git branch --show-current)
```

**detached HEAD チェック**: `TOPIC_BRANCH` が空の場合は即座に停止する。

```bash
if [ -z "$TOPIC_BRANCH" ]; then
  echo "カレントがブランチ上ではありません（detached HEAD）。処理を停止します。"
  # stop here
fi
```

### 2. ベースブランチを決定する

優先順位:

1. **ユーザー引数があればそれを使う**
   - `/after-merge <ベースブランチ>` の引数を `BASE_BRANCH` として採用する。
2. **無ければマージ済みPRから自動検出する**
   ```bash
   BASE_BRANCH=$(gh pr list --head "$TOPIC_BRANCH" --state merged \
     --json baseRefName --limit 1 --jq '.[0].baseRefName // ""')
   ```
   - `jq` フィルタに `// ""` を付けて `null` を空文字列に変換する。
   - 取得結果が空または `null` 文字列の場合はステップ 3 へ進む。
3. **どちらでも決まらない場合は `main` にフォールバックする**
   - フォールバックを使ったことをユーザーに必ず一言伝える（誤爆防止）。
   ```bash
   if [ -z "$BASE_BRANCH" ] || [ "$BASE_BRANCH" = "null" ]; then
     BASE_BRANCH=main
     echo "PR の base ref を自動検出できなかったため、main にフォールバックします。"
   fi
   ```

### 3. 事前チェック

以下の条件をすべて確認する。ひとつでも満たさない場合は**作業をストップしてユーザーに報告する**。

#### 3-1. カレントブランチがトピックブランチであること（= ベースブランチでないこと）

- `TOPIC_BRANCH` が `BASE_BRANCH` と等しい場合はストップ（すでにベースブランチにいる）。

#### 3-2. 未コミットの変更がないこと

```bash
git status --porcelain
```

- 出力が空でない場合はストップ（未コミットの変更あり）。

#### 3-3. トピックブランチがoriginにプッシュ済みで、未プッシュのコミットがないこと

```bash
git status --short --branch
```

- `ahead` が含まれる場合はストップ（未プッシュのコミットあり）。
- `origin/...` のリモート追跡ブランチが存在しない場合はストップ（originにプッシュされていない）。

### 4. ベースブランチを最新化する

```bash
git switch "$BASE_BRANCH"
git pull
```

### 5. トピックブランチを削除する

```bash
git branch -d "$TOPIC_BRANCH"
```

`-d` (safe delete) を使うこと。マージ未確認のブランチは Git が拒否するので、`-D` での強制削除は使わない。
