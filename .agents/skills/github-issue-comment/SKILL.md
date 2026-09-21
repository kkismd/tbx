---
name: github-issue-comment
description: Use this skill when the user asks to comment on a GitHub issue, post investigation results to an issue, record an implementation plan in an issue, or otherwise submit a multi-line GitHub issue comment. Write the comment body to a temporary file under `.tmp/`, use `gh issue comment --body-file`, and delete the temporary file after the comment is posted.
---

# GitHub Issue Comment

Use this skill for GitHub issue comments.

Workflow:

1. Create the comment body as a temporary file under `.tmp/` with `apply_patch`.
2. Use `gh issue comment ... --body-file .tmp/<file>.md`.
3. After the comment is posted successfully, delete the temporary file.

Rules:

- Use `.tmp/`, not `/tmp`.
- Do not inline long comment text with shell quoting or heredocs.
- Use descriptive filenames such as `.tmp/issue-494-plan.md`.
- If posting fails, keep the file only long enough to retry, then delete it.
