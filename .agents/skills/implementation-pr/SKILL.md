---
name: implementation-pr
description: Use this skill when the user asks to implement an issue or fix a bug and bring the repository to a PR-ready state. Use worker sub-agents for bounded implementation tasks when parallel work helps.
---

# Implementation PR

Use this skill for "implement the issue and prepare the change for review" work.

Scope:

- Confirm the applicable `AGENTS.md` instructions and the current source of truth before implementation.
- When issue or PR context is supplied by the task, use that context first. When working locally, use available repository or GitHub integrations to obtain missing issue details. If the required context is unavailable and no network or connector is available, do not guess; report the missing information.
- Read the issue body and comments, related specifications, ADRs, existing implementation, and tests as applicable. Treat comments as part of the task context for requirements, clarifications, constraints, and prior investigation notes.
- Reconcile the requested scope with the current source of truth before coding.
- Implement the requested change.
- Add or update tests in proportion to risk.
- Run the required checks.
- Self-review the acceptance conditions against the implementation and tests.
- Prepare a PR-ready summary containing the change overview, verification, and issue reference. When the implementation fully satisfies an issue, use `Closes #<issue>` in the proposed PR body; use `Refs #<issue>` for partial work.
- Do not merge the change yourself or close the issue yourself.

Sub-agent guidance:

- Use a `worker` sub-agent only for bounded implementation tasks with clear file ownership when one is available.
- If the next critical step depends on the code change, keep it local rather than blocking on a worker.
- Good delegation examples:
  - add tests in one file while the main agent updates core logic
  - patch a separate module with non-overlapping write scope

## Environment handoff

Codex Cloud:

- Work in the checkout and branch provided by Cloud.
- Implement, test, and prepare the PR title/body information; do not require remote push or a command-line PR creation step.
- Hand the PR-ready summary and verification results to Cloud's standard PR creation flow.

Local CLI:

- Branch, commit, push, and PR creation are delegated to the available local Git/GitHub workflow and its global skills.
- This repository skill defines what the implementation must achieve, not the local command sequence used to submit it.

Verification:

- Run repository-required checks before handing off the PR-ready result.
- Report any check you could not run.
- Keep the proposed PR body focused on what changed, why, and how it was verified.
