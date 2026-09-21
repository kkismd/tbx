---
name: issue-investigation
description: Use this skill when the user asks to investigate an issue, reproduce a bug, find the cause, or inspect related code paths. Use an explorer sub-agent when parallel codebase inspection would help.
---

# Issue Investigation

Use this skill for "investigate the issue and produce an evidence-based result" work.

Scope:

- Read the issue and comments from task-provided context first. When working locally, use available repository or GitHub integrations to obtain missing context. If the required context is unavailable and no network or connector is available, do not guess; report the missing information.
- Reproduce the problem locally when possible.
- Identify the root cause in code.
- Summarize the cause, evidence, and a concrete fix plan.
- Prepare an issue-ready investigation summary when the user asks for one; the actual publishing mechanism is supplied by the environment.

Sub-agent guidance:

- Use an `explorer` sub-agent when the issue may involve multiple code paths or when parallel code inspection would shorten the investigation.
- Keep the main agent on the critical path: reproduction, direct source reading, and final synthesis usually stay local.
- Ask sub-agents for bounded outputs such as "find the code path for X" or "review this PR for bugs", not for vague full ownership of the investigation.

Output expectations:

- Lead with the root cause.
- Distinguish observed behavior, direct cause, and proposed fix.
- Include concrete file references and, when useful, minimal reproduction snippets.
