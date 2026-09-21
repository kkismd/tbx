---
name: git-commit-message
description: Use this skill when the user asks to commit changes, create a git commit, or prepare a commit message. Write the commit message to `.tmp/commit-message.txt`, use `git commit -F .tmp/commit-message.txt`, and delete the temporary file after the commit succeeds.
---

# Git Commit Message

Use this skill for git commits that should not inline the commit message in the shell command.

Workflow:

1. Write the commit message to `.tmp/commit-message.txt` with `apply_patch`.
2. Run `git commit -F .tmp/commit-message.txt`.
3. After the commit succeeds, delete the temporary file.
4. Do not stage the temporary file in `git add`; it is only an input to `git commit -F`.
5. After committing, verify that the temporary file did not enter the commit when needed, for example with `git show --name-only --stat HEAD`.

Rules:

- Use `.tmp/commit-message.txt` exactly.
- Use `.tmp/`, not `/tmp`.
- Do not pass long commit messages with `git commit -m` when the message is prepared text.
- Do not include the temporary message file in the commit; only the intended source changes should be staged.
- If the commit fails, keep the file only long enough to retry, then delete it.
