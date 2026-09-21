---
name: issue-investigation
description: Use this skill when the user asks to investigate a GitHub issue, reproduce a bug, find the cause, inspect related code paths, or post an investigation summary or implementation plan back to the issue. Use an explorer sub-agent when parallel codebase inspection would help, and use `.tmp/` body files for GitHub issue comments.
---

# Issue Investigation

Use this skill for "investigate issue" work.

Scope:

- Read the GitHub issue and comments.
- Reproduce the problem locally when possible.
- Identify the root cause in code.
- Summarize the cause and a concrete fix plan.
- Post findings to the issue when the user asks for that.

Sub-agent guidance:

- Use an `explorer` sub-agent when the issue may involve multiple code paths or when parallel code inspection would shorten the investigation.
- Keep the main agent on the critical path: reproduction, direct source reading, and final synthesis usually stay local.
- Ask sub-agents for bounded outputs such as "find the code path for X" or "review this PR for bugs", not for vague full ownership of the investigation.

GitHub posting rules:

- For issue comments, follow the `github-issue-comment` and `tmp-file-messaging` workflow.
- Write the comment body to `.tmp/<descriptive-name>.md` with `apply_patch`.
- Use `gh issue comment ... --body-file .tmp/<file>.md`.
- Delete the temporary file after successful posting.

Output expectations:

- Lead with the root cause.
- Distinguish observed behavior, direct cause, and proposed fix.
- Include concrete file references and, when useful, minimal reproduction snippets.
