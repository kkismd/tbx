---
name: implementation-pr
description: Use this skill when the user asks to implement an issue, fix a bug, make code changes, create a branch, commit the work, and open a GitHub pull request. Use worker sub-agents for bounded implementation tasks when parallel work helps, and use `.tmp/` body files for commit messages and PR bodies.
---

# Implementation PR

Use this skill for "fix it and make a PR" work.

Scope:

- Create a topic branch.
- Before branching or implementation, check the current git branch and worktree state.
- If the current checkout is on a non-`main` branch and has uncommitted changes, stop and ask the user how to proceed before making implementation changes.
- When the implementation task references a GitHub issue by number, URL, or `gh issue` context, read both the issue body and its comments before planning or coding. Treat comments as part of the task context for requirements, clarifications, constraints, and prior investigation notes.
- Implement the requested change.
- Add or update tests in proportion to risk.
- Run the required checks.
- Commit the change.
- Open a non-draft PR with a clear summary and verification section unless the user explicitly asks for a draft or the work is intentionally incomplete.

Sub-agent guidance:

- Use a `worker` sub-agent only for bounded implementation tasks with clear file ownership.
- If the next critical step depends on the code change, keep it local rather than blocking on a worker.
- Good delegation examples:
  - add tests in one file while the main agent updates core logic
  - patch a separate module with non-overlapping write scope

Commit and PR rules:

- For commit messages, follow `git-commit-message` and `tmp-file-messaging`.
- Write the commit message to `.tmp/commit-message.txt`, use `git commit -F`, then delete the file.
- For PR creation, follow `github-pr-create` and `tmp-file-messaging`.
- Write the PR body to `.tmp/pr-body.md`, push the topic branch and wait for that push to succeed, then run `gh pr create --body-file`, and finally delete the file.
- In restricted-network environments, request escalated execution for the first GitHub command and group subsequent required `gh` operations into that approved execution where practical, so approval is not repeated for each network call.
- Use `Closes #<issue>` in the PR body when the implementation satisfies the issue requirements end to end; use `Refs #<issue>` only when the work is partial or follow-up work remains.
- Do not parallelize `git push` and `gh pr create`.

Verification:

- Run repository-required checks before opening the PR.
- Report any check you could not run.
- Keep the PR body focused on what changed, why, and how it was verified.
