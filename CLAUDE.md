# CLAUDE.md

See AGENTS.md for all project instructions.

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Agent roles

| Agent | Role |
|---|---|
| `plan-issue` | Reads issue, records implementation plan as issue comment |
| `spec-discussion` | Discusses spec options, records decisions to issue |
| `implement-issue` | Implements from issue, creates PR (no review cycle) |
| `review-implementation` | Reviews PR, posts review comments / opens issues |
| `blueprint-updater` | Updates `blueprint*.md` files and creates PR |

## Agent spawning rules

- Do **not** use `isolation: "worktree"` when spawning `implement-issue` or `fix-pr` agents. Worktree isolation prevents the `after-merge` skill from detecting the topic branch and requires manual worktree cleanup.
