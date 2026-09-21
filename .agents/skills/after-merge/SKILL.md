---
name: after-merge
description: Use when a pull request has been merged and the local git workspace should be cleaned up. Return from the merged topic branch to the PR base branch, update that base branch, and safely delete the merged topic branch. If the user explicitly provides the base branch, use it. Otherwise detect the merged PR base ref from GitHub and fall back to `main` only if detection fails.
---

# After Merge

Use this skill when the user says a pull request was merged and wants the local repository cleaned up afterward.

## Goal

Clean up the local workspace after a merged PR by:

- recording the current topic branch
- determining the correct base branch
- refusing to proceed if the worktree is not safe to clean up
- switching back to the base branch
- updating the base branch
- deleting the merged topic branch with safe deletion

Do not assume the base branch is always `main`. Support intermediate integration branches as PR bases.

## Inputs

- If the user explicitly gives a base branch name, use it.
- Otherwise, detect the base branch from the merged PR associated with the current topic branch.

## Procedure

### 1. Record the current topic branch

Run:

```bash
git branch --show-current
```

If the result is empty, stop immediately and report that the repository is in detached HEAD state.

### 2. Determine the base branch

Use this priority order:

1. A base branch explicitly provided by the user.
2. The base ref from the merged PR for the current topic branch.
3. `main` as a fallback only when detection fails.

To auto-detect from GitHub, run:

```bash
gh pr list --head "$TOPIC_BRANCH" --state merged --json baseRefName --limit 1 --jq '.[0].baseRefName // ""'
```

If detection fails or returns an empty value, fall back to `main` and explicitly tell the user that fallback was used.

### 3. Run safety checks

Run all of the following checks before making branch changes. If any check fails, stop and report the reason to the user.

#### 3-1. Confirm the current branch is not already the base branch

If `TOPIC_BRANCH == BASE_BRANCH`, stop. There is no topic branch to clean up from.

#### 3-2. Confirm there are no uncommitted changes

Run:

```bash
git status --porcelain
```

If the output is not empty, stop.

#### 3-3. Confirm the topic branch is pushed and has no unpushed commits

Run:

```bash
git status --short --branch
```

Stop if either condition is true:

- the output shows `ahead`
- there is no remote tracking branch under `origin/...`

### 4. Switch to the base branch and update it

Run:

```bash
git switch "$BASE_BRANCH"
git pull
```

If `git pull` requires elevated permissions in this environment, request them rather than skipping the update.

### 5. Delete the merged topic branch safely

Run:

```bash
git branch -d "$TOPIC_BRANCH"
```

Use `-d`, not `-D`. If Git refuses deletion because the branch is not merged, stop and report that to the user instead of forcing deletion.

## Reporting

Tell the user:

- which base branch was used
- whether the base branch was auto-detected or manually specified
- whether fallback to `main` was needed
- whether the base branch update succeeded
- whether the local topic branch was deleted
- whether the remote topic branch was already absent, if that fact comes up during cleanup
