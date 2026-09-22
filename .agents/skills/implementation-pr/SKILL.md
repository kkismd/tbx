---
name: implementation-pr
description: issueの実装、バグ修正、コード変更、ブランチ作成、コミット、GitHub pull requestの作成を依頼されたときに使用する。並列化が有効な場合は、範囲を限定した実装作業にworker sub-agentを使用し、コミットメッセージとPR本文には`.tmp/`配下のファイルを使用する。
---

# Implementation PR

「修正してPRを作成する」作業にはこのskillを使用する。

対象範囲:

- topic branchを作成する。
- branch作成または実装の前に、現在のgit branchとworktreeの状態を確認する。
- 現在のcheckoutが`main`以外のbranchで、未コミットの変更がある場合は、実装変更を行う前に停止し、どう進めるかユーザーに確認する。
- 実装対象がGitHub issueの番号、URL、または`gh issue`の文脈を参照している場合は、計画やコーディングの前にissue本文とコメントの両方を読む。コメントも要件、補足、制約、過去の調査メモを含むタスク文脈として扱う。
- 依頼された変更を実装する。
- リスクに応じてテストを追加または更新する。
- 必須のチェックを実行する。
- 変更をコミットする。
- ユーザーが明示的にdraftを依頼した場合、または作業が意図的に未完了の場合を除き、変更概要と検証結果を含む明確なnon-draft PRを作成する。

Sub-agentの指針:

- `worker` sub-agentは、担当ファイルが明確な範囲限定の実装作業にのみ使用する。
- 次の重要な手順がコード変更に依存する場合は、workerを待つために停止せず、手元で実施する。
- 委譲に適した例:
  - main agentがコアロジックを更新する間に、1つのファイルへテストを追加する。
  - 書き込み範囲が重ならない別モジュールを修正する。

CommitとPRのルール:

- commit messageについては`git-commit-message`と`tmp-file-messaging`に従う。
- commit messageを`.tmp/commit-message.txt`へ書き、`git commit -F`を使用してからファイルを削除する。
- PR作成については`github-pr-create`と`tmp-file-messaging`に従う。
- PR本文を`.tmp/pr-body.md`へ書き、topic branchをpushして成功を待ってから`gh pr create --body-file`を実行し、最後にファイルを削除する。
- 制限されたネットワーク環境では、最初のGitHubコマンドに対して昇格実行を要求する。必要な`gh`コマンドの範囲について再利用可能な承認が環境から提供される場合はそれを優先し、そうでなければ安全に連続実行できる隣接した`gh`操作だけをまとめる。承認を再利用するためだけに、ワークフローの順序を変更または遅延してはならない。
- 実装がissueの要件を一通り満たす場合はPR本文に`Closes #<issue>`を使用し、部分的な作業または後続作業が残る場合だけ`Refs #<issue>`を使用する。
- `git push`と`gh pr create`を並列化してはならない。

検証:

- PRを作成する前に、リポジトリで必須とされているチェックを実行する。
- 実行できなかったチェックがあれば報告する。
- PR本文は、何を変更したか、なぜ変更したか、どのように検証したかに焦点を絞る。
