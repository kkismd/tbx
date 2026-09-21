---
name: tmp-file-messaging
description: Use this skill when creating GitHub issue comments, PR bodies, review comments, or git commit messages that need multi-line text. It requires writing the text to a temporary file under `.tmp/`, using that file with `gh ... --body-file` or `git commit -F`, and deleting the temporary file after the command succeeds.
---

# Tmp File Messaging

For multi-line GitHub posts and commit messages, do not inline long text in the shell command.

Use this workflow:

1. Create a temporary file under `.tmp/` with `apply_patch`.
2. Put the full body text in that file.
3. Use the file with one of these patterns:
   - `gh issue comment ... --body-file .tmp/<file>.md`
   - `gh pr comment ... --body-file .tmp/<file>.md`
   - `gh pr create ... --title "$(cat .tmp/pr-title.txt)" --body-file .tmp/pr-body.md`
   - `git commit -F .tmp/commit-message.txt`
4. After the command succeeds, delete the temporary file.
5. Do not stage the temporary message file in `git add`; it is an operational artifact, not part of the repository changes.
6. When the command is `git commit -F`, verify after the commit that the temporary file is not present in `HEAD` if there is any doubt.

Rules:

- Use `.tmp/`, not `/tmp` and not inline heredocs.
- For PR feedback/comments, use `gh pr comment`, not `gh pr review`.
- Use `.tmp/pr-title.txt` exactly for PR titles created with `gh pr create`.
- Use `.tmp/pr-body.md` exactly for PR bodies created with `gh pr create --body-file`.
- Use `.tmp/commit-message.txt` exactly for commit messages.
- Temporary message files must never be committed; stage only the real project files.
- Delete the file after successful use so `.tmp/` does not accumulate stale files.
- If the command fails before submission, keep the file only as long as needed for retry, then delete it.
