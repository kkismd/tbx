---
name: pr-review-loop
description: Use this skill when the user asks to review a pull request with a sub-agent, post review feedback to a PR, fix review findings with sub-agents, re-run checks, and repeat until the PR is ready. Use `gh pr comment`, not `gh pr review`, and use `.tmp/` body files for PR comments.
---

# PR Review Loop

Use this skill for "review the PR, fix findings, and comment back" work.

Scope:

- Have a sub-agent review the PR.
- If there are findings, implement fixes and re-test.
- Re-run review after fixes when needed.
- Post the review outcome back to the PR as a comment.

Sub-agent guidance:

- Use an `explorer` sub-agent for review.
- Ask for concrete review output: bugs, regressions, missing tests, or "approve equivalent, no findings".
- If findings are returned, use one or more `worker` sub-agents for bounded fixes when parallel edits are useful.
- After fixes, run the relevant checks locally before requesting another review pass.

PR comment rules:

- Use `gh pr comment`, not `gh pr review`.
- Write PR comments to `.tmp/pr-review-comment.md` with `apply_patch`.
- Use `gh pr comment ... --body-file .tmp/pr-review-comment.md`.
- Delete the temporary file after successful posting.

Review outcome handling:

- If findings exist, summarize them precisely and fix them before posting a final "approved" style comment.
- If no findings exist, post a concise approval-equivalent comment via `gh pr comment`.
- Mention the verification that was re-run after any fixes.
