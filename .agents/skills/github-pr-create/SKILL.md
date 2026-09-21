---
name: github-pr-create
description: Use this skill when the user asks to create a GitHub pull request, open a PR, or prepare a PR body. Write the PR title/body content to `.tmp/pr-title.txt` and `.tmp/pr-body.md`, use `gh pr create` with those files, and delete the temporary files after the PR is created.
---

# GitHub PR Create

Use this skill for GitHub pull request creation.

Workflow:

1. Draft the PR title in `.tmp/pr-title.txt` with `apply_patch`.
2. Draft the PR body in `.tmp/pr-body.md` with `apply_patch`.
3. If the branch has local commits that are not yet on the remote, run `git push -u origin <branch>` first and wait for it to succeed.
4. Only after the push completes successfully, run `gh pr create ... --title "$(cat .tmp/pr-title.txt)" --body-file .tmp/pr-body.md`.
   In restricted-network environments, request escalated execution for the first `gh pr create` attempt instead of probing with an unprivileged command first, because PR creation must contact GitHub.
5. After the PR is created successfully, delete the temporary files.

Rules:

- Use `.tmp/pr-title.txt` exactly for PR titles.
- Use `.tmp/pr-body.md` exactly for PR bodies.
- Use `.tmp/`, not `/tmp`.
- Do not inline long PR bodies in shell arguments.
- Do not run `git push` and `gh pr create` in parallel. `gh pr create` must observe the pushed remote head branch.
- If PR creation fails, keep the files only for retry/debug, then delete them.
