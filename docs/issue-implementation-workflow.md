# Issue implementation workflow

`python3 scripts/tbx_implement.py ISSUE_NUMBER` runs the issue-to-PR workflow from a local shell. It requires Python 3, Git, the GitHub CLI (`gh`) authenticated for this repository, and Codex CLI.

The workflow only starts from a clean local `main` checkout. It checks the repository root, worktree, and in-progress Git operations, fetches `origin/main`, and stops unless local `main` matches it. It fetches the open issue, its comments, and directly referenced issues and PRs through `gh`, then creates `issue/ISSUE_NUMBER-implement` and starts a finite `codex exec` session with that snapshot and the repository guidance.

Codex implements the issue, runs all repository-required checks, and creates one Japanese commit. Its final response uses a JSON Schema with explicit `success`, `failed`, or `human_review_required` status. After success, the parent workflow checks the branch, reported HEAD, single-commit ancestry, and clean worktree again. It pushes the branch and creates a non-draft PR only after those checks pass.

The parent process owns all routine GitHub communication. The Codex prompt prohibits `gh`, push, PR creation, merge, issue close, and history-rewriting Git commands. A failed phase stops the workflow before later side effects and prints a JSON result with the phase. Once the topic branch is created, failures leave it available for inspection; return to `main` and resolve or remove the topic branch before starting again. If the push succeeds but PR creation fails, the pushed branch is left intact for recovery.

The command does not merge the PR or close the issue. It does not update `main` automatically; update the checkout and return to a clean `main` before starting.

Workflow boundary tests can be run with:

```sh
python3 -m unittest discover -s tests -p 'test_issue_workflow.py'
```
