#!/usr/bin/env python3
"""Process one snapshot of feedback on an existing pull request."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable, Sequence

from tbx_codex_progress import CodexInterrupted, CodexStopError, interruption_git_state, progress, run_codex_jsonl

REPO_ROOT = Path(__file__).resolve().parent.parent
REQUIRED_CHECKS = ("cargo ci-clippy", "cargo ci-test", "cargo ci-fmt")
MAX_CODEX_DIAGNOSTIC_CHARS = 2_000
RECORD_MARKER = "<!-- tbx-w01-review-feedback-record:v1 -->"
REVIEW_RESULT_MARKER_RE = re.compile(
    r"<!-- tbx-w01-review-result:v1 head=([0-9a-f]{40}) "
    r"result=(changes_required|no_additional_changes) -->"
)
REVIEW_RESULT_LIKE_RE = re.compile(r"tbx-w01-review-result")
FEEDBACK_KINDS = ("issue-comment", "review", "review-comment")
ID_RE = re.compile(r"^(issue-comment|review|review-comment):([1-9][0-9]*)$")
SHA_RE = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
TRAILER_RE = re.compile(r"^Review-Feedback:\s*(\S+)\s*$", re.MULTILINE)


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str
    stderr: str = ""


class WorkflowError(Exception):
    def __init__(self, phase: str, message: str, *, human: bool = False):
        super().__init__(message)
        self.phase = phase
        self.human = human


def run_command(args: Sequence[str], *, cwd: Path = REPO_ROOT, input_text: str | None = None) -> CommandResult:
    if args and args[0] == "codex" and "--json" in args:
        return run_codex_jsonl(args, cwd=cwd, input_text=input_text)
    try:
        result = subprocess.run(list(args), cwd=cwd, input=input_text, text=True, capture_output=True, check=False)
    except OSError as error:
        return CommandResult(127, "", str(error))
    return CommandResult(result.returncode, result.stdout, result.stderr)


def message_key(message: dict) -> str:
    kind, identifier = message.get("kind"), message.get("id")
    if not isinstance(identifier, (str, int)) or not ID_RE.fullmatch(f"{kind}:{identifier}"):
        raise ValueError("invalid feedback message identity")
    return f"{kind}:{identifier}"


def message_revision(message: dict) -> str:
    return f"{message_key(message)}@{message.get('updatedAt') or message.get('createdAt') or ''}"


def timestamp(value: str) -> datetime | None:
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    if parsed.tzinfo is None:
        return None
    return parsed.astimezone(timezone.utc)


def derive_review_result(pr: dict, comments: list[dict], messages: list[dict]) -> dict:
    """Derive the latest unexpired result for the PR's current head."""
    head_sha = pr.get("headRefOid")
    current_head = head_sha if isinstance(head_sha, str) else None
    markers = []
    for comment in comments:
        body = comment.get("body")
        created_at = comment.get("created_at")
        if not isinstance(body, str) or not isinstance(created_at, str):
            continue
        match = REVIEW_RESULT_MARKER_RE.search(body)
        if not match or match.group(1) != current_head:
            continue
        posted_at = timestamp(created_at)
        identifier = comment.get("id")
        if posted_at is None or not isinstance(identifier, int):
            continue
        markers.append((posted_at, identifier, match.group(2)))

    if not markers:
        return {"status": "none", "head_sha": current_head}

    posted_at, _, result = max(markers, key=lambda marker: (marker[0], marker[1]))
    for message in messages:
        updated_at = message.get("updatedAt")
        created_at = message.get("createdAt")
        latest_feedback = timestamp(updated_at) if isinstance(updated_at, str) else None
        if latest_feedback is None and isinstance(created_at, str):
            latest_feedback = timestamp(created_at)
        if latest_feedback is not None and latest_feedback > posted_at:
            return {"status": "none", "head_sha": current_head}
    return {"status": result, "head_sha": current_head}


def is_review_result_comment(body: object) -> bool:
    """Exclude valid and malformed review-result comments from feedback."""
    return isinstance(body, str) and REVIEW_RESULT_LIKE_RE.search(body) is not None


def parse_records(comments: list[dict]) -> dict[str, dict]:
    records: dict[str, dict] = {}
    for comment in comments:
        body = comment.get("body")
        if not isinstance(body, str) or RECORD_MARKER not in body:
            continue
        payload = body.split(RECORD_MARKER, 1)[1].strip()
        try:
            record = json.loads(payload)
        except json.JSONDecodeError as error:
            raise WorkflowError("feedback_fetch", f"invalid W01 processing record: {error}") from error
        if not isinstance(record, dict) or not isinstance(record.get("results"), list):
            raise WorkflowError("feedback_fetch", "W01 processing record is missing its results")
        for item in record["results"]:
            if not isinstance(item, dict) or not isinstance(item.get("revision"), str):
                raise WorkflowError("feedback_fetch", "W01 processing record has an invalid result entry")
            records[item["revision"]] = {**record, **item}
    return records


def extract_candidates(messages: list[dict], comments: list[dict], trailer_keys: dict[str, str], valid_record_heads: set[str] | None = None) -> list[dict]:
    """Choose unprocessed messages from the immutable start-of-run snapshot."""
    records = parse_records(comments)
    candidates = []
    for message in messages:
        key = message_key(message)
        revision = message_revision(message)
        record = records.get(revision)
        if record and (valid_record_heads is None or record.get("head_sha") in valid_record_heads):
            result = record.get("result")
            if result in ("fixed", "no_change"):
                continue
        # A pushed fix may exist even when posting its processing record failed.
        # A changed revision is deliberately reconsidered.
        if key in trailer_keys and not record:
            updated = message.get("updatedAt") or message.get("createdAt")
            feedback_time = timestamp(updated) if updated else None
            commit_time = timestamp(trailer_keys[key])
            if feedback_time and commit_time and commit_time >= feedback_time:
                continue
        candidates.append(message)
    def sort_key(message: dict):
        thread_order = message.get("threadOrder")
        if thread_order is not None:
            return (message.get("createdAt") or "", 0, str(message.get("threadId") or ""), int(thread_order), int(message["id"]))
        return (message.get("createdAt") or "", 1, "", int(message["id"]))
    return sorted(candidates, key=sort_key)


class ReviewFeedbackWorkflow:
    def __init__(self, run: Callable[..., CommandResult] = run_command):
        self.run = run

    def command(self, phase: str, args: Sequence[str], *, input_text: str | None = None) -> str:
        result = self.run(args, input_text=input_text)
        if result.returncode:
            detail = result.stderr.strip() or result.stdout.strip() or f"exit code {result.returncode}"
            raise WorkflowError(phase, detail)
        return result.stdout.strip()

    def git(self, *args: str, phase: str = "preflight") -> str:
        return self.command(phase, ("git", *args))

    def gh_json(self, phase: str, args: Sequence[str]):
        try:
            return json.loads(self.command(phase, args))
        except json.JSONDecodeError as error:
            raise WorkflowError(phase, f"GitHub command returned invalid JSON: {error}") from error

    def gh_pages(self, endpoint: str) -> list[dict]:
        pages = self.gh_json("pr_fetch", ("gh", "api", "--paginate", "--slurp", endpoint))
        if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
            raise WorkflowError("pr_fetch", "GitHub paginated response is invalid")
        return [row for page in pages for row in page if isinstance(row, dict)]

    def fetch_snapshot(self, number: int) -> tuple[dict, list[dict]]:
        progress(f"GitHub PR文脈を取得: #{number}")
        pr = self.gh_json("pr_fetch", ("gh", "pr", "view", str(number), "--json", "number,state,isDraft,mergedAt,headRefName,headRefOid,title,body,baseRefName"))
        if not isinstance(pr, dict) or pr.get("number") != number or pr.get("state") != "OPEN" or pr.get("mergedAt"):
            raise WorkflowError("pr_fetch", "pull request must exist, be open, and be unmerged")
        if not isinstance(pr.get("headRefName"), str) or not isinstance(pr.get("headRefOid"), str):
            raise WorkflowError("pr_fetch", "pull request response is missing head branch or SHA")
        repo = self.gh_json("pr_fetch", ("gh", "repo", "view", "--json", "nameWithOwner"))
        slug = repo.get("nameWithOwner") if isinstance(repo, dict) else None
        if not isinstance(slug, str):
            raise WorkflowError("pr_fetch", "repository response is missing nameWithOwner")
        pr["diff"] = self.command("pr_fetch", ("gh", "pr", "diff", str(number)))
        comments = self.gh_pages(f"repos/{slug}/issues/{number}/comments?per_page=100")
        reviews = self.gh_pages(f"repos/{slug}/pulls/{number}/reviews?per_page=100")
        inline = self.gh_pages(f"repos/{slug}/pulls/{number}/comments?per_page=100")
        pr["comments"], pr["reviews"] = comments, reviews
        messages: list[dict] = []
        for row in comments:
            if (isinstance(row, dict) and isinstance(row.get("body"), str)
                    and RECORD_MARKER not in row["body"] and not is_review_result_comment(row["body"])):
                messages.append({"kind": "issue-comment", "id": row.get("id"), "body": row["body"], "createdAt": row.get("created_at"), "updatedAt": row.get("updated_at"), "url": row.get("html_url")})
        for row in reviews:
            if isinstance(row, dict) and isinstance(row.get("body"), str) and row["body"].strip():
                messages.append({"kind": "review", "id": row.get("id"), "body": row["body"], "createdAt": row.get("submitted_at"), "updatedAt": row.get("submitted_at"), "url": row.get("html_url")})
        for row in inline:
            if isinstance(row, dict) and isinstance(row.get("body"), str):
                messages.append({"kind": "review-comment", "id": row.get("id"), "body": row["body"], "createdAt": row.get("created_at"), "updatedAt": row.get("updated_at"), "url": row.get("html_url"), "inReplyToId": row.get("in_reply_to_id"), "path": row.get("path"), "line": row.get("line"), "commitId": row.get("commit_id")})
        threads = self.gh_json("pr_fetch", ("gh", "api", "graphql", "-f", "query=query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{id isResolved isOutdated comments(first:100){nodes{databaseId}}}}}}}", "-F", f"owner={slug.split('/')[0]}", "-F", f"name={slug.split('/')[1]}", "-F", f"number={number}"))
        try:
            thread_nodes = threads["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"]
        except (TypeError, KeyError):
            raise WorkflowError("pr_fetch", "review thread response is invalid") from None
        thread_by_comment = {}
        for thread in thread_nodes:
            for order, node in enumerate(thread.get("comments", {}).get("nodes", [])):
                thread_by_comment[node.get("databaseId")] = {"threadId": thread.get("id"), "threadOrder": order, "threadResolved": thread.get("isResolved"), "threadOutdated": thread.get("isOutdated")}
        for message in messages:
            if message["kind"] == "review-comment":
                message.update(thread_by_comment.get(int(message["id"]), {}))
        for msg in messages:
            message_key(msg)
        referenced_issues = set(re.findall(r"(?<![\w/])#([1-9][0-9]*)\b", pr.get("body") or ""))
        for message in messages:
            referenced_issues.update(re.findall(r"(?<![\w/])#([1-9][0-9]*)\b", message["body"]))
        linked_issues = {}
        for reference in sorted(referenced_issues - {str(number)}, key=int):
            issue = self.gh_json("pr_fetch", ("gh", "issue", "view", reference, "--comments", "--json", "number,title,body,state,url,comments"))
            if not isinstance(issue, dict) or issue.get("number") != int(reference):
                raise WorkflowError("pr_fetch", f"could not validate referenced issue #{reference}")
            linked_issues[reference] = issue
        pr["linked_issues"] = linked_issues
        progress("GitHub PR文脈の取得が完了")
        return pr, messages

    def preflight(self, pr: dict) -> str:
        progress("事前確認を開始")
        if Path(self.git("rev-parse", "--show-toplevel")).resolve() != REPO_ROOT:
            raise WorkflowError("preflight", "run from this repository")
        branch = self.git("branch", "--show-current")
        if branch != pr["headRefName"]:
            raise WorkflowError("preflight", f"expected PR head branch {pr['headRefName']}, found {branch or 'detached HEAD'}")
        if self.git("status", "--porcelain=v1", "--untracked-files=all"):
            raise WorkflowError("preflight", "worktree has tracked or untracked changes")
        for marker in ("MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "rebase-merge", "rebase-apply"):
            path = Path(self.git("rev-parse", "--git-path", marker))
            if not path.is_absolute():
                path = REPO_ROOT / path
            if path.exists():
                raise WorkflowError("preflight", f"git operation in progress: {marker}")
        head = self.git("rev-parse", "HEAD")
        if head != pr["headRefOid"]:
            raise WorkflowError("preflight", "local HEAD does not match the PR head SHA")
        return head

    def trailer_keys(self, start_sha: str) -> dict[str, str]:
        raw = self.git("log", "--format=%B%x00%aI%x1e", start_sha, phase="preflight")
        seen: dict[str, str] = {}
        for entry in raw.split("\x1e"):
            if "\x00" not in entry:
                continue
            body, committed = entry.split("\x00", 1)
            for key in TRAILER_RE.findall(body):
                seen[key] = max(seen.get(key, ""), committed.strip())
        return seen

    def is_ancestor(self, ancestor: str, descendant: str) -> bool:
        result = self.run(("git", "merge-base", "--is-ancestor", ancestor, descendant))
        if result.returncode not in (0, 1):
            raise WorkflowError("preflight", result.stderr.strip() or "could not verify commit ancestry")
        return result.returncode == 0

    def run_pr(self, number: int) -> dict:
        pr, messages = self.fetch_snapshot(number)
        start_sha = self.preflight(pr)
        progress(f"事前確認が完了: PR #{number}")
        trailers = self.trailer_keys(start_sha)
        comments = pr["comments"]
        records = parse_records(comments)
        valid_record_heads = {
            head for record in records.values()
            if isinstance((head := record.get("head_sha")), str) and SHA_RE.fullmatch(head) and self.is_ancestor(head, start_sha)
        }
        candidates = extract_candidates(messages, comments, trailers, valid_record_heads)
        snapshot = {"pr": pr, "start_head_sha": start_sha, "candidate_revisions": [message_revision(m) for m in candidates], "candidates": candidates,
                    "context_messages": messages,
                    "review_result": derive_review_result(pr, comments, messages)}
        if not candidates:
            progress("未処理フィードバックなし。正常終了")
            return {"status": "success", "summary": "No unprocessed feedback.", "results": [], "modified_messages": [], "checks_passed": [], "commit_sha": None,
                    "review_result": snapshot["review_result"]}
        prompt = self.build_prompt(snapshot)
        temp_root = REPO_ROOT / ".tmp"
        temp_root.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="tbx-review-workflow-", dir=temp_root) as temp_dir:
            temp = Path(temp_dir)
            schema_path, result_path = temp / "result.schema.json", temp / "result.json"
            schema_path.write_text(json.dumps(RESULT_SCHEMA), encoding="utf-8")
            progress(f"Codex実行開始: PR #{number}")
            result = self.run(("codex", "exec", "--json", "--output-schema", str(schema_path), "--output-last-message", str(result_path), "-C", str(REPO_ROOT), "-"), input_text=prompt)
            progress("Codex実行終了")
            if result.returncode:
                detail = result.stderr.strip() or result.stdout.strip() or f"exit code {result.returncode}"
                raise WorkflowError("codex", detail[-MAX_CODEX_DIAGNOSTIC_CHARS:])
            try:
                codex = json.loads(result_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as error:
                raise WorkflowError("codex", f"missing or invalid structured result: {error}") from error
        if not isinstance(codex, dict) or codex.get("status") not in ("success", "human_review_required"):
            status = codex.get("status") if isinstance(codex, dict) else None
            raise WorkflowError("codex", "Codex did not complete successfully")
        human_review = codex["status"] == "human_review_required"
        if not isinstance(codex.get("summary"), str) or not codex["summary"].strip():
            raise WorkflowError("codex", "Codex result is missing its summary")
        expected_revisions = set(snapshot["candidate_revisions"])
        results = codex.get("results")
        if not isinstance(results, list) or {row.get("revision") for row in results if isinstance(row, dict)} != expected_revisions:
            raise WorkflowError("codex", "Codex results do not cover exactly the snapshot candidates")
        if len(results) != len(expected_revisions) or any(not isinstance(row.get("reason"), str) or not row["reason"].strip() for row in results):
            raise WorkflowError("codex", "Codex results contain duplicates or missing reasons")
        modified = codex.get("modified_messages")
        if not isinstance(modified, list) or not set(modified).issubset(expected_revisions):
            raise WorkflowError("codex", "Codex reported invalid modified message revisions")
        allowed_results = {"human_review_required"} if human_review else {"fixed", "no_change"}
        if any(row.get("result") not in allowed_results for row in results):
            raise WorkflowError("codex", "Codex candidate results do not match the overall status")
        fixed = {row["revision"] for row in results if row["result"] == "fixed"}
        if len(set(modified)) != len(modified) or set(modified) != fixed:
            raise WorkflowError("codex", "fixed feedback and modified message revisions do not match")
        checks = codex.get("checks_passed")
        commit = codex.get("commit_sha")
        if human_review:
            if modified or checks or commit is not None:
                raise WorkflowError("codex", "human review results must not report changes, checks, or a commit")
        elif modified:
            if checks != list(REQUIRED_CHECKS) or not isinstance(commit, str):
                raise WorkflowError("verification", "modified feedback requires all checks and a commit SHA")
            progress("Codex後のGit検証を開始")
            self.verify_commit(pr["headRefName"], start_sha, commit, {message_key(m) for m in candidates if message_revision(m) in modified})
            progress("Codex後のGit検証が完了")
        elif commit is not None or checks:
            raise WorkflowError("codex", "no-change results must not report a commit or checks")
        response = {"status": "human_review_required" if human_review else "success", "summary": codex.get("summary", ""), "results": results, "modified_messages": modified, "checks_passed": checks, "commit_sha": commit, "snapshot": snapshot}
        response["review_result"] = snapshot["review_result"]
        if modified:
            progress(f"push開始: {pr['headRefName']}")
            self.command("push", ("git", "push", "origin", pr["headRefName"]))
            progress("push完了")
        record = {"start_head_sha": start_sha, "head_sha": commit or start_sha, "results": results}
        body = "W01 PRフィードバック処理記録\n\n" + "\n".join(
            f"- `{row['revision']}`: {row['result']} — {row['reason']}" for row in results
        ) + f"\n\n状態: {response['status']}\n理由: {codex.get('summary', '')}\n確認時head: `{start_sha}`\n処理後head: `{record['head_sha']}`\n検証: {', '.join(checks) if checks else '未実行'}\n\n{RECORD_MARKER}\n" + json.dumps(record, ensure_ascii=False)
        progress("PR処理記録を投稿")
        self.command("record", ("gh", "pr", "comment", str(number), "--body", body))
        progress("処理記録投稿完了")
        if human_review:
            progress("人間判断待ち")
        response.pop("snapshot")
        return response

    def build_prompt(self, snapshot: dict) -> str:
        return f"""Process this fixed snapshot of PR feedback in this repository. Re-read applicable AGENTS.md, the implementation conventions skill, issue guidelines, and referenced ADRs/specifications. Resolve ambiguity or conflicts by returning human_review_required without changes. Treat candidate revisions as the only processing targets; context_messages are context only. New feedback after start_head_sha is out of scope. Make minimal changes only for valid feedback. For fixes, run all required checks and make one Japanese commit with one Review-Feedback: <kind>:<id> trailer for every changed message and none for context/no-change messages. Do not push or use gh. Return the required JSON only.

Snapshot:
```json
{json.dumps(snapshot, ensure_ascii=False, indent=2)}
```
"""

    def verify_commit(self, branch: str, start_sha: str, reported_sha: object, expected_trailers: set[str]) -> None:
        if self.git("branch", "--show-current", phase="post_codex") != branch:
            raise WorkflowError("post_codex", "branch changed after Codex")
        head = self.git("rev-parse", "HEAD", phase="post_codex")
        if not isinstance(reported_sha, str) or head != reported_sha:
            raise WorkflowError("post_codex", "HEAD does not match Codex result")
        if self.git("merge-base", "--is-ancestor", start_sha, head, phase="post_codex") != "":
            raise WorkflowError("post_codex", "starting head is not an ancestor of current HEAD")
        if self.git("status", "--porcelain=v1", "--untracked-files=all", phase="post_codex"):
            raise WorkflowError("post_codex", "worktree is not clean")
        raw = self.git("log", "--format=%B", f"{start_sha}..{head}", phase="post_codex")
        if set(TRAILER_RE.findall(raw)) != expected_trailers:
            raise WorkflowError("post_codex", "Review-Feedback trailers do not match modified messages")


RESULT_SCHEMA = {"type": "object", "additionalProperties": False, "required": ["status", "summary", "results", "modified_messages", "checks_passed", "commit_sha"], "properties": {
    "status": {"type": "string", "enum": ["success", "failed", "human_review_required"]}, "summary": {"type": "string"},
    "results": {"type": "array", "items": {"type": "object", "additionalProperties": False, "required": ["revision", "result", "reason"], "properties": {"revision": {"type": "string"}, "result": {"type": "string", "enum": ["fixed", "no_change", "human_review_required"]}, "reason": {"type": "string"}}}},
    "modified_messages": {"type": "array", "items": {"type": "string"}}, "checks_passed": {"type": "array", "items": {"type": "string", "enum": list(REQUIRED_CHECKS)}}, "commit_sha": {"type": ["string", "null"]}}}


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Process one snapshot of feedback on an existing pull request.")
    parser.add_argument("pull_request", type=int)
    args = parser.parse_args(argv)
    try:
        result = ReviewFeedbackWorkflow().run_pr(args.pull_request)
    except (CodexInterrupted, CodexStopError) as error:
        progress("workflow interrupted: codex_execution")
        print(json.dumps({
            "status": "interrupted",
            "stage": "codex_execution",
            **interruption_git_state(REPO_ROOT),
            "codex_process_group_stopped": isinstance(error, CodexInterrupted),
            **({"error": str(error)} if isinstance(error, CodexStopError) else {}),
        }, ensure_ascii=False))
        return 130
    except WorkflowError as error:
        status = "human_review_required" if error.human else "failed"
        progress(f"workflow {status}: {error.phase}")
        print(json.dumps({"status": status, "phase": error.phase, "error": str(error)}, ensure_ascii=False))
        return 1
    progress(f"workflow {result['status']}")
    print(json.dumps(result, ensure_ascii=False))
    return 2 if result["status"] == "human_review_required" else 0


if __name__ == "__main__":
    raise SystemExit(main())
