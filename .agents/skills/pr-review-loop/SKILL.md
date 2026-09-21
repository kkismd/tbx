---
name: pr-review-loop
description: Use this skill when the user asks to review a pull request, fix review findings, re-run checks, and repeat until the PR is ready. Use an explorer or worker sub-agent when available and useful.
---

# PR Review Loop

Use this skill for "review the PR, fix findings, re-test, and produce a traceable outcome" work.

Scope:

- Review the PR when its diff and context are available. Use an explorer sub-agent when available and useful.
- Check the issue purpose, acceptance conditions, scope, specifications, ADRs, implementation, and tests for consistency.
- If there are findings, implement fixes and re-test.
- Re-run review after fixes when needed.
- Produce a traceable review outcome containing findings, impact conditions, fixes, and verification. The environment supplies the mechanism for publishing it.

Sub-agent guidance:

- Use an `explorer` sub-agent for review when one is available and parallel inspection is useful.
- Ask for concrete review output: bugs, regressions, missing tests, or "approve equivalent, no findings".
- If findings are returned, use one or more `worker` sub-agents for bounded fixes when available and parallel edits are useful.
- After fixes, run the relevant checks locally before requesting another review pass.

Review outcome handling:

- If findings exist, summarize them precisely and fix them before posting a final "approved" style comment.
- If no findings exist, report that there are no blockers and include the verification performed.
- When the same account is both author and reviewer, use a comment-equivalent outcome rather than an approval/request-changes review action.
- Mention the verification that was re-run after any fixes.

Environment handoff:

- Prefer PR and issue context supplied by the task or available repository integrations. If required context is unavailable and no network or connector exists, do not infer review results.
- Codex Cloud should return the review outcome to its standard PR workflow; it does not require command-line GitHub comments.
- Local CLI publishing is delegated to the available global GitHub skills or normal repository workflow.
