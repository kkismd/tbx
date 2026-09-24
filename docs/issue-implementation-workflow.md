# 実装issueからPRを作成するworkflow

ローカルシェルから `python3 scripts/tbx_implement.py ISSUE_NUMBER` を実行すると、issueの実装からPR作成までを進めます。Python 3、Git、このリポジトリに対して認証済みのGitHub CLI（`gh`）、Codex CLIが必要です。

このworkflowは、変更のないローカルの `main` から開始します。リポジトリの場所、worktree、進行中のGit操作を確認し、`origin/main` を取得します。ローカルの `main` が `origin/main` と一致しない場合は停止します。

入力できるのは、GitHub上に存在し、open状態で、本文に `**種別: 実装**` を含むissueだけです。ADR、調査・計画、種別表示のないissue、closed issueは、topic branchを作る前に拒否します。

`gh` で対象issueとコメント、および本文やコメントから直接参照されているissueとPRを取得します。入力条件を確認した後、`issue/ISSUE_NUMBER-implement` ブランチを作成し、取得した文脈とリポジトリ内のガイドに基づいてCodex CLI（`codex exec`）を実行します。

Codexはissueを実装し、リポジトリで必須とされているチェックをすべて実行してから、日本語のcommitを1つ作成します。Codexの最終応答はJSON Schemaで形式を定め、`success`（成功）、`failed`（失敗）、`human_review_required`（人間の判断が必要）のいずれかを返します。成功時は、親workflowがブランチ、報告されたHEAD、開始時点からのcommit数と親子関係、worktreeがcleanであることを再確認します。これらの確認後にのみブランチをpushし、draftではないPRを作成します。

通常のGitHub通信は親workflowが担当します。Codexへの指示では、`gh`、push、PR作成、merge、issue close、履歴を書き換えるGitコマンドを禁止しています。いずれかの段階で失敗すると、その後の副作用を実行せず、失敗段階を含むJSON結果を出力します。

workflowの段階表示とCodex実行イベントは標準エラーへ逐次表示します。CodexのJSONLイベントは共通表示層で人間向けに整形し、未対応イベントは無視します。標準出力には最後の機械可読JSON結果だけを出力します。Codex側のイベントにreasoning要約が含まれる場合も、その要約だけを表示します。

Codex実行中に Ctrl-C で中断すると、親workflowはCodex専用プロセスグループへSIGINTを送り、終了を最大5秒待ちます。まだ動作していればSIGTERMを送り、さらに最大5秒後も終了しなければSIGKILLを送ります。各段階でグループ終了を確認してから後始末します。中断時はstdoutへ `status: "interrupted"`、`stage`、branch、HEAD、worktree状態（`clean` / `dirty` / `unknown`）を含むJSONを1件出力し、終了コード130を返します。停止を確認できなかった場合も成功として扱わず、JSONにその旨を含めます。中断時にtopic branchや未commit変更を自動削除・巻き戻ししません。表示されたbranch、HEAD、worktree状態を復旧調査の起点にしてください。

topic branch作成後に失敗した場合も、そのブランチは調査できるよう残します。再実行するには `main` に戻り、topic branchの状態を確認してから対処してください。push成功後にPR作成が失敗した場合、push済みブランチはそのまま残ります。

PRのmergeとissueのcloseは行いません。`main` の更新も自動では行いません。開始前にローカルの `main` を更新し、変更のない状態にしてください。

workflow境界のテストは、次のコマンドで実行できます。

```sh
python3 -m unittest discover -s tests -p 'test_issue_workflow.py'
```

## 既存PRのフィードバックを1回処理する

`python3 scripts/tbx_review_feedback.py PR_NUMBER` は、openかつ未mergeのPRに対してフィードバックを1回処理します。親workflowがPR本文・差分・conversation comment・review・inline comment・review thread・head SHAを取得し、実行開始時点の候補を固定してCodexへ渡します。threadのresolved/outdated状態は文脈として渡し、単独で処理済み判定には使いません。

起動前に、現在のbranchがPRのhead branchと一致し、worktreeがcleanで進行中のGit操作がなく、HEADがPR head SHAと一致することを確認します。不一致時はCodexを起動せず停止します。未処理候補がなければ変更・commit・pushを行いません。

W01の処理記録は機械可読marker付きのPR commentとして投稿します。修正commitには修正対象だけの `Review-Feedback: <kind>:<id>` trailerを含め、Codex終了後に親workflowがbranch、HEAD、worktree、開始時headの祖先関係、trailerを検証してからpushします。push後に処理記録投稿が失敗してもcommitは巻き戻さず、次回はGit履歴のtrailerを使って同じmessage revisionへの重複修正を防ぎます。

変更不要の場合もcommitせずPR処理記録を投稿します。仕様衝突や判断不足など人間判断が必要な場合は、Codexが人間判断待ちで停止します。PR mergeとissue closeは行いません。
