---
name: github-pr-comment
description: Use this skill when the user asks to comment on a GitHub pull request, post review feedback to a PR, leave an approval note on a PR, or otherwise submit a multi-line PR comment. Write the comment body to a temporary file under `.tmp/`, use `gh pr comment --body-file`, and delete the temporary file after the comment is posted. Do not use `gh pr review`.
---

# GitHub PR Comment

Use this skill for GitHub pull request comments and review feedback when the same account is both author and reviewer.

Workflow:

1. Create the comment body as a temporary file under `.tmp/` with `apply_patch`.
2. Use `gh pr comment ... --body-file .tmp/<file>.md`.
3. After the comment is posted successfully, delete the temporary file.

Rules:

- Use `gh pr comment`, not `gh pr review`.
- Use `.tmp/`, not `/tmp`.
- Do not inline long comment text with shell quoting or heredocs.
- Use descriptive filenames such as `.tmp/pr-493-review-comment.md`.
- If posting fails, keep the file only long enough to retry, then delete it.
