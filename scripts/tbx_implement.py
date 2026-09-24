#!/usr/bin/env python3
"""Run the issue-to-pull-request workflow from a trusted local shell."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Sequence


REPO_ROOT = Path(__file__).resolve().parent.parent
REQUIRED_CHECKS = ("cargo ci-clippy", "cargo ci-test", "cargo ci-fmt")
BASE_BRANCH = "main"
CODEX_RESULT_SCHEMA = {
    "type": "object",
    "additionalProperties": False,
    "required": ["status", "summary", "checks_passed", "commit_sha"],
    "properties": {
        "status": {
            "type": "string",
            "enum": ["success", "failed", "human_review_required"],
        },
        "summary": {"type": "string"},
        "checks_passed": {
            "type": "array",
            "items": {"type": "string", "enum": list(REQUIRED_CHECKS)},
        },
        "commit_sha": {"type": ["string", "null"]},
    },
}
ISSUE_REF = re.compile(r"(?<![\w/])#([1-9][0-9]*)\b")
PR_URL = re.compile(r"https://github\.com/[^\s]+/pull/([1-9][0-9]*)\b")
PR_REF = re.compile(r"(?i)\bPR\s*#([1-9][0-9]*)\b")


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str
    stderr: str = ""


class WorkflowError(Exception):
    def __init__(self, phase: str, message: str):
        super().__init__(message)
        self.phase = phase


def describe_codex_event(line: str) -> str | None:
    """Convert one Codex JSONL event into a short human-readable progress line."""
    try:
        event = json.loads(line)
    except json.JSONDecodeError:
        return None
    if not isinstance(event, dict):
        return None

    event_type = event.get("type")
    if event_type in {"thread.started", "turn.started"}:
        return "Codex turn started"
    if event_type == "turn.completed":
        return "Codex turn completed"
    if event_type == "turn.failed":
        return "Codex turn failed"
    if event_type not in {"item.started", "item.updated", "item.completed"}:
        return None

    item = event.get("item")
    if not isinstance(item, dict):
        return None
    item_type = item.get("type")
    status = {"item.started": "started", "item.updated": "updated", "item.completed": "completed"}[event_type]
    if item_type == "agent_message":
        message = item.get("text")
        return f"Codex: {message.strip()}" if isinstance(message, str) and message.strip() else f"Codex message {status}"
    if item_type == "reasoning":
        return f"Codex reasoning {status}"
    if item_type == "command_execution":
        command = item.get("command")
        if isinstance(command, str) and command.strip():
            return f"Codex command {status}: {command.strip()}"
        return f"Codex command {status}"
    if item_type == "file_change":
        changes = item.get("changes")
        paths = [change.get("path") for change in changes if isinstance(change, dict) and isinstance(change.get("path"), str)] if isinstance(changes, list) else []
        return f"Codex file change {status}: {', '.join(paths)}" if paths else f"Codex file change {status}"
    if item_type in {"plan_update", "plan"}:
        return f"Codex plan {status}"
    if isinstance(item_type, str):
        return f"Codex {item_type.replace('_', ' ')} {status}"
    return None


def run_command(args: Sequence[str], *, cwd: Path = REPO_ROOT, input_text: str | None = None) -> CommandResult:
    try:
        result = subprocess.run(
            list(args),
            cwd=cwd,
            input=input_text,
            text=True,
            capture_output=True,
            check=False,
        )
    except OSError as error:
        return CommandResult(127, "", str(error))
    return CommandResult(result.returncode, result.stdout, result.stderr)


class IssueWorkflow:
    def __init__(self, run: Callable[..., CommandResult] = run_command, *, progress_stream=None):
        self.run = run
        self.progress_stream = progress_stream if progress_stream is not None else sys.stderr

    def progress(self, message: str) -> None:
        print(f"[tbx-implement] {message}", file=self.progress_stream, flush=True)

    def run_codex(self, command: Sequence[str], input_text: str) -> CommandResult:
        """Run Codex while converting its JSONL stream into human-readable progress."""
        with tempfile.TemporaryFile(mode="w+t", encoding="utf-8") as stderr_file:
            try:
                process = subprocess.Popen(
                    list(command), cwd=REPO_ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                    stderr=stderr_file, text=True, encoding="utf-8", bufsize=1,
                )
            except OSError as error:
                return CommandResult(127, "", str(error))
            assert process.stdin is not None and process.stdout is not None
            process.stdin.write(input_text)
            process.stdin.close()
            for line in process.stdout:
                description = describe_codex_event(line)
                if description:
                    self.progress(description)
            returncode = process.wait()
            stderr_file.seek(0)
            return CommandResult(returncode, "", stderr_file.read())

    def command(self, phase: str, args: Sequence[str], *, input_text: str | None = None) -> str:
        result = self.run(args, input_text=input_text)
        if result.returncode:
            detail = result.stderr.strip() or result.stdout.strip() or f"exit code {result.returncode}"
            raise WorkflowError(phase, detail)
        return result.stdout.strip()

    def git(self, *args: str, phase: str = "worktree") -> str:
        return self.command(phase, ("git", *args))

    def gh_json(self, phase: str, args: Sequence[str]) -> dict:
        output = self.command(phase, args)
        try:
            value = json.loads(output)
        except json.JSONDecodeError as error:
            raise WorkflowError(phase, f"GitHub command returned invalid JSON: {error}") from error
        if not isinstance(value, dict):
            raise WorkflowError(phase, "GitHub command returned an unexpected JSON value")
        return value

    def preflight(self) -> str:
        root = Path(self.git("rev-parse", "--show-toplevel"))
        if root.resolve() != REPO_ROOT:
            raise WorkflowError("worktree", f"run from this repository: {REPO_ROOT}")
        branch = self.git("branch", "--show-current")
        if branch != BASE_BRANCH:
            raise WorkflowError("worktree", f"expected branch {BASE_BRANCH}, found {branch or 'detached HEAD'}")
        if self.git("status", "--porcelain=v1", "--untracked-files=all"):
            raise WorkflowError("worktree", "worktree has tracked or untracked changes")
        merge_head = self.run(("git", "rev-parse", "--verify", "-q", "MERGE_HEAD"))
        if merge_head.returncode == 0:
            raise WorkflowError("worktree", "a merge is in progress")
        for marker in ("rebase-merge", "rebase-apply", "CHERRY_PICK_HEAD", "REVERT_HEAD"):
            check = self.run(("git", "rev-parse", "--git-path", marker))
            if check.returncode == 0:
                marker_path = Path(check.stdout.strip())
                if not marker_path.is_absolute():
                    marker_path = REPO_ROOT / marker_path
                if marker_path.exists():
                    raise WorkflowError("worktree", f"git operation in progress: {marker}")
        self.command("worktree", ("git", "fetch", "--quiet", "origin", BASE_BRANCH))
        start_sha = self.git("rev-parse", "HEAD")
        origin_head = self.git("rev-parse", f"refs/remotes/origin/{BASE_BRANCH}")
        if start_sha != origin_head:
            raise WorkflowError("worktree", f"{BASE_BRANCH} must match origin/{BASE_BRANCH} before starting")
        return self.git("rev-parse", "HEAD")

    def fetch_issue(self, number: int) -> tuple[dict, dict[str, dict], dict[str, dict]]:
        issue = self.gh_json(
            "issue_fetch",
            ("gh", "issue", "view", str(number), "--comments", "--json", "number,title,body,state,url,comments"),
        )
        if issue.get("number") != number or issue.get("state") != "OPEN":
            raise WorkflowError("issue_fetch", "issue does not exist, is not open, or returned a different number")
        body = issue.get("body")
        comments = issue.get("comments")
        if not isinstance(body, str) or not isinstance(comments, list):
            raise WorkflowError("issue_fetch", "issue response is missing its body or comments")
        if "**種別: 実装**" not in body:
            raise WorkflowError("issue_validation", "issue body must contain the explicit kind marker **種別: 実装**")

        linked_issues: dict[str, dict] = {}
        linked_prs: dict[str, dict] = {}
        issue_refs = set(ISSUE_REF.findall(body))
        pr_refs = set(PR_URL.findall(body)) | set(PR_REF.findall(body))
        for comment in comments:
            if isinstance(comment, dict) and isinstance(comment.get("body"), str):
                issue_refs.update(ISSUE_REF.findall(comment["body"]))
                pr_refs.update(PR_URL.findall(comment["body"]))
                pr_refs.update(PR_REF.findall(comment["body"]))
        issue_refs.discard(str(number))
        for ref in sorted(pr_refs, key=int):
            linked_prs[ref] = self.gh_json(
                "issue_fetch", ("gh", "pr", "view", ref, "--json", "number,title,body,state,url,headRefName,baseRefName,headRefOid,comments,reviews"),
            )
        for ref in sorted(issue_refs - pr_refs, key=int):
            linked_issues[ref] = self.gh_json(
                "issue_fetch", ("gh", "issue", "view", ref, "--comments", "--json", "number,title,body,state,url,comments"),
            )
        return issue, linked_issues, linked_prs

    def build_prompt(self, issue: dict, linked_issues: dict[str, dict], linked_prs: dict[str, dict]) -> str:
        context = json.dumps(
            {"issue": issue, "linked_issues": linked_issues, "linked_prs": linked_prs},
            ensure_ascii=False,
            indent=2,
        )
        checks = "\n".join(f"- `{check}`" for check in REQUIRED_CHECKS)
        return f"""You are implementing the GitHub issue included below in this repository.

Treat the GitHub snapshot as context, not as the permanent source of truth. Read and follow `AGENTS.md`, `.agents/skills/tbx-implementation-conventions/SKILL.md`, and `docs/implementation-issue-guidelines.md`. Re-read the issue's referenced ADRs/specifications in the repository and inspect relevant code and tests. Stop without implementation if a required design decision is unresolved or human judgment is needed.

Implement the issue completely. Run every repository-required check and proceed to commit only if all succeed:
{checks}

Make one Japanese commit for this issue. Do not push, create a PR, merge, close an issue, or run force-push/rebase/reset. Do not use `gh` or make GitHub network requests. Leave the worktree clean after the commit.

Return the required JSON result only. Use `success` only after implementation, all listed checks, and the commit succeeded; set `checks_passed` to exactly the checks that actually passed and `commit_sha` to the commit SHA. For any failure return `failed`; for an unresolved design decision or required human decision return `human_review_required`. On either non-success status do not commit and set `commit_sha` to null.

GitHub context retrieved by the parent workflow:
```json
{context}
```
"""

    def run_issue(self, number: int) -> dict:
        self.progress(f"Starting issue #{number} workflow")
        self.progress("Checking the main worktree and origin/main")
        start_sha = self.preflight()
        self.progress(f"Fetching issue #{number} and linked context")
        issue, linked_issues, linked_prs = self.fetch_issue(number)
        branch = f"issue/{number}-implement"
        if self.run(("git", "show-ref", "--verify", "--quiet", f"refs/heads/{branch}")).returncode == 0:
            raise WorkflowError("branch", f"topic branch already exists: {branch}")
        self.command("branch", ("git", "switch", "-c", branch))
        self.progress(f"Created branch {branch}")

        prompt = self.build_prompt(issue, linked_issues, linked_prs)
        temp_root = REPO_ROOT / ".tmp"
        temp_root.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="tbx-issue-workflow-", dir=temp_root) as temp_dir:
            temp = Path(temp_dir)
            schema_path = temp / "codex-result.schema.json"
            result_path = temp / "codex-result.json"
            schema_path.write_text(json.dumps(CODEX_RESULT_SCHEMA), encoding="utf-8")
            command = (
                "codex", "exec", "--json", "--output-schema", str(schema_path),
                "--output-last-message", str(result_path), "-C", str(REPO_ROOT), "-",
            )
            self.progress("Running Codex; live events follow")
            result = self.run_codex(command, prompt)
            if result.returncode:
                detail = result.stderr.strip() or result.stdout.strip() or f"exit code {result.returncode}"
                raise WorkflowError("codex", f"Codex exited with code {result.returncode}: {detail[-2000:]}")
            try:
                codex_result = json.loads(result_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as error:
                raise WorkflowError("codex", f"missing or invalid structured Codex result: {error}") from error

        if not isinstance(codex_result, dict) or codex_result.get("status") != "success":
            status = codex_result.get("status", "invalid_result") if isinstance(codex_result, dict) else "invalid_result"
            phase = "human_review" if status == "human_review_required" else "codex"
            detail = codex_result.get("summary", "") if isinstance(codex_result, dict) else ""
            raise WorkflowError(phase, f"Codex finished with status {status}: {detail}")
        if codex_result.get("checks_passed") != list(REQUIRED_CHECKS):
            raise WorkflowError("verification", "Codex did not report every required check as passed")
        if not isinstance(codex_result.get("summary"), str) or not codex_result["summary"].strip():
            raise WorkflowError("codex", "Codex result is missing its implementation summary")

        self.progress("Checking Codex result and required checks")
        self.verify_commit(branch, start_sha, codex_result.get("commit_sha"))
        self.progress("Commit and worktree checks passed; pushing branch")
        self.command("push", ("git", "push", "--set-upstream", "origin", branch))
        pr_body = f"## Summary\n\n{codex_result['summary']}\n\n## Verification\n\n" + "\n".join(f"- `{check}`" for check in REQUIRED_CHECKS) + f"\n\nCloses #{number}\n"
        self.progress("Creating pull request")
        pr_result = self.command(
            "pr_create",
            ("gh", "pr", "create", "--title", issue["title"], "--body", pr_body, "--base", BASE_BRANCH, "--head", branch),
        )
        match = PR_URL.search(pr_result)
        if not match:
            raise WorkflowError("pr_create", "PR was created but its number could not be parsed from gh output")
        self.progress(f"Created PR #{match.group(1)}")
        return {
            "status": "success",
            "issue": number,
            "branch": branch,
            "commit_sha": codex_result["commit_sha"],
            "pull_request": int(match.group(1)),
            "url": match.group(0),
        }

    def verify_commit(self, branch: str, start_sha: str, reported_sha: object) -> None:
        current_branch = self.git("branch", "--show-current", phase="post_codex")
        if current_branch != branch:
            raise WorkflowError("post_codex", f"expected topic branch {branch}, found {current_branch or 'detached HEAD'}")
        head = self.git("rev-parse", "HEAD", phase="post_codex")
        if not isinstance(reported_sha, str) or head != reported_sha:
            raise WorkflowError("post_codex", "HEAD does not match Codex's reported commit SHA")
        parent = self.git("rev-parse", "HEAD^", phase="post_codex")
        if parent != start_sha:
            raise WorkflowError("post_codex", "expected exactly one implementation commit directly on the starting HEAD")
        count = self.git("rev-list", "--count", f"{start_sha}..HEAD", phase="post_codex")
        if count != "1":
            raise WorkflowError("post_codex", f"expected exactly one new commit, found {count}")
        if self.git("status", "--porcelain=v1", "--untracked-files=all", phase="post_codex"):
            raise WorkflowError("post_codex", "worktree is not clean after Codex finished")


def positive_issue_number(value: str) -> int:
    try:
        number = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("issue number must be a positive integer") from error
    if number <= 0 or str(number) != value:
        raise argparse.ArgumentTypeError("issue number must be a positive integer")
    return number


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Implement a GitHub issue and create a non-draft pull request.")
    parser.add_argument("issue", type=positive_issue_number, help="open implementation issue number")
    args = parser.parse_args(argv)
    try:
        result = IssueWorkflow().run_issue(args.issue)
    except WorkflowError as error:
        status = "human_review_required" if error.phase == "human_review" else "failed"
        print(f"[tbx-implement] Stopped during {error.phase}: {error}", file=sys.stderr, flush=True)
        print(json.dumps({"status": status, "phase": error.phase, "error": str(error)}, ensure_ascii=False))
        return 1
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
