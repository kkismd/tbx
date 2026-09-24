import json
import io
import contextlib
import sys
import unittest
from unittest.mock import patch
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import tbx_implement


class FakeWorkflow(tbx_implement.IssueWorkflow):
    def __init__(
        self, *, codex_status="success", checks=None, codex_exit=0, push_fails=False, pr_fails=False,
        issue_state="OPEN", issue_kind="実装", issue_body=None, returned_issue_number=1917,
        issue_fetch_fails=False,
    ):
        self.calls = []
        self.codex_status = codex_status
        self.checks = list(tbx_implement.REQUIRED_CHECKS) if checks is None else checks
        self.codex_exit = codex_exit
        self.push_fails = push_fails
        self.pr_fails = pr_fails
        self.issue_state = issue_state
        self.issue_body = issue_body if issue_body is not None else f"**種別: {issue_kind}**"
        self.returned_issue_number = returned_issue_number
        self.issue_fetch_fails = issue_fetch_fails
        self.progress_output = io.StringIO()
        self.branch = "main"
        super().__init__(self.fake_run, progress_stream=self.progress_output)

    def run_codex(self, command, input_text):
        if "--output-last-message" not in command:
            return super().run_codex(command, input_text)
        self.calls.append(("codex", "exec", "stream"))
        if self.codex_exit:
            return tbx_implement.CommandResult(self.codex_exit, "", "Codex failed")
        result_path = Path(command[command.index("--output-last-message") + 1])
        result = {
            "status": self.codex_status,
            "summary": "Implemented issue.",
            "checks_passed": self.checks if self.codex_status == "success" else [],
            "commit_sha": "new-sha" if self.codex_status == "success" else None,
        }
        result_path.write_text(json.dumps(result), encoding="utf-8")
        return tbx_implement.CommandResult(0, "")

    def fake_run(self, args, **kwargs):
        args = tuple(args)
        self.calls.append(args)
        if args[:3] == ("gh", "issue", "view"):
            if self.issue_fetch_fails:
                return tbx_implement.CommandResult(1, "", "issue unavailable")
            issue = {
                "number": self.returned_issue_number,
                "title": "Work item",
                "body": self.issue_body,
                "state": self.issue_state,
                "url": "https://github.com/kkismd/tbx/issues/1917",
                "comments": [],
            }
            return tbx_implement.CommandResult(0, json.dumps(issue))
        if args[:3] == ("git", "show-ref", "--verify"):
            return tbx_implement.CommandResult(1, "")
        if args[:3] == ("git", "switch", "-c"):
            self.branch = args[3]
            return tbx_implement.CommandResult(0, "")
        if args[:3] == ("git", "push", "--set-upstream") and self.push_fails:
            return tbx_implement.CommandResult(1, "", "push failed")
        if args[:3] == ("gh", "pr", "create"):
            if self.pr_fails:
                return tbx_implement.CommandResult(1, "", "PR create failed")
            return tbx_implement.CommandResult(0, "https://github.com/kkismd/tbx/pull/2222\n")
        return tbx_implement.CommandResult(0, "")

    def preflight(self):
        self.calls.append(("preflight",))
        return "base-sha"

    def verify_commit(self, branch, start_sha, reported_sha):
        self.calls.append(("verify-commit", branch, start_sha, reported_sha))
        if reported_sha != "new-sha":
            raise tbx_implement.WorkflowError("post_codex", "unexpected commit")


class IssueWorkflowTests(unittest.TestCase):
    def test_issue_number_must_be_positive_decimal(self):
        self.assertEqual(tbx_implement.positive_issue_number("1917"), 1917)
        for value in ("0", "-1", "1.2", "01", "word"):
            with self.subTest(value=value), self.assertRaises(tbx_implement.argparse.ArgumentTypeError):
                tbx_implement.positive_issue_number(value)

    def test_preflight_accepts_only_clean_main_at_origin_head(self):
        class PreflightRunner:
            dirty = False

            def __call__(self, args, **kwargs):
                args = tuple(args)
                if args == ("git", "rev-parse", "--show-toplevel"):
                    return tbx_implement.CommandResult(0, str(tbx_implement.REPO_ROOT))
                if args == ("git", "branch", "--show-current"):
                    return tbx_implement.CommandResult(0, "main")
                if args[:2] == ("git", "status"):
                    return tbx_implement.CommandResult(0, "?? stray\n" if self.dirty else "")
                if args == ("git", "rev-parse", "--verify", "-q", "MERGE_HEAD"):
                    return tbx_implement.CommandResult(1, "")
                if args == ("git", "fetch", "--quiet", "origin", "main"):
                    return tbx_implement.CommandResult(0, "")
                if args == ("git", "rev-parse", "HEAD") or args == (
                    "git", "rev-parse", "refs/remotes/origin/main"
                ):
                    return tbx_implement.CommandResult(0, "base-sha")
                if args[:3] == ("git", "rev-parse", "--git-path"):
                    return tbx_implement.CommandResult(0, str(tbx_implement.REPO_ROOT / ".git" / args[3]))
                raise AssertionError(args)

        runner = PreflightRunner()
        self.assertEqual(tbx_implement.IssueWorkflow(runner).preflight(), "base-sha")
        runner.dirty = True
        with self.assertRaises(tbx_implement.WorkflowError):
            tbx_implement.IssueWorkflow(runner).preflight()

    def test_success_creates_pr_only_after_commit_verification_and_push(self):
        workflow = FakeWorkflow()
        result = workflow.run_issue(1917)
        self.assertEqual(result["pull_request"], 2222)
        ordered = [call[0:2] for call in workflow.calls]
        self.assertLess(ordered.index(("verify-commit", "issue/1917-implement")), ordered.index(("git", "push")))
        self.assertLess(ordered.index(("git", "push")), ordered.index(("gh", "pr")))

    def test_workflow_progress_is_ordered_and_kept_on_stderr_stream(self):
        workflow = FakeWorkflow()
        workflow.run_issue(1917)
        lines = workflow.progress_output.getvalue().splitlines()
        expected = (
            "Starting issue #1917 workflow", "Checking the main worktree and origin/main",
            "Fetching issue #1917 and linked context", "Created branch issue/1917-implement",
            "Running Codex; live events follow", "Checking Codex result and required checks",
            "Commit and worktree checks passed; pushing branch", "Created PR #2222",
        )
        positions = [next(index for index, line in enumerate(lines) if phrase in line) for phrase in expected]
        self.assertEqual(positions, sorted(positions))

    def test_main_stdout_contains_only_final_json(self):
        workflow = FakeWorkflow()
        stdout = io.StringIO()
        stderr = io.StringIO()
        with patch.object(tbx_implement, "IssueWorkflow", return_value=workflow):
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                exit_code = tbx_implement.main(["1917"])
        self.assertEqual(exit_code, 0)
        self.assertEqual(json.loads(stdout.getvalue())["status"], "success")
        self.assertEqual(stderr.getvalue(), "")
        self.assertTrue(workflow.progress_output.getvalue().startswith("[tbx-implement]"))

    def test_codex_jsonl_events_are_summarized_and_unknown_events_ignored(self):
        events = (
            '{"type":"item.completed","item":{"type":"reasoning","text":"private reasoning"}}',
            '{"type":"item.started","item":{"type":"command_execution","command":"cargo test"}}',
            '{"type":"item.completed","item":{"type":"file_change","changes":[{"path":"src/lib.rs"}]}}',
            '{"type":"future.event","payload":{}}',
        )
        rendered = [tbx_implement.describe_codex_event(event) for event in events]
        self.assertEqual(rendered[:3], [
            "Codex reasoning completed", "Codex command started: cargo test",
            "Codex file change completed: src/lib.rs",
        ])
        self.assertIsNone(rendered[3])
        self.assertNotIn("private reasoning", " ".join(value or "" for value in rendered))

    def test_codex_events_are_written_as_the_process_emits_them(self):
        workflow = FakeWorkflow()

        class FakeProcess:
            returncode = 0

            def __init__(self):
                self.stdin = io.StringIO()
                self.stdout = self.lines()

            def lines(self):
                yield '{"type":"turn.started"}\n'
                self.assert_progress_visible()
                yield '{"type":"future.event"}\n'

            def assert_progress_visible(self):
                self_outer.assertIn("Codex turn started", workflow.progress_output.getvalue())

            def wait(self):
                return self.returncode

        self_outer = self
        with patch.object(tbx_implement.subprocess, "Popen", return_value=FakeProcess()):
            result = workflow.run_codex(("codex", "exec", "--json"), "prompt")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")

    def test_codex_failure_does_not_push_or_create_pr(self):
        for status in ("failed", "human_review_required"):
            with self.subTest(status=status):
                workflow = FakeWorkflow(codex_status=status)
                with self.assertRaises(tbx_implement.WorkflowError):
                    workflow.run_issue(1917)
                self.assertFalse(any(call[:2] == ("git", "push") for call in workflow.calls))
                self.assertFalse(any(call[:3] == ("gh", "pr", "create") for call in workflow.calls))

    def test_failed_codex_process_does_not_push(self):
        workflow = FakeWorkflow(codex_exit=1)
        with self.assertRaises(tbx_implement.WorkflowError) as error:
            workflow.run_issue(1917)
        self.assertEqual(error.exception.phase, "codex")
        self.assertFalse(any(call[:2] == ("git", "push") for call in workflow.calls))

    def test_incomplete_check_results_do_not_push(self):
        workflow = FakeWorkflow(checks=list(tbx_implement.REQUIRED_CHECKS[:-1]))
        with self.assertRaises(tbx_implement.WorkflowError) as error:
            workflow.run_issue(1917)
        self.assertEqual(error.exception.phase, "verification")
        self.assertFalse(any(call[:2] == ("git", "push") for call in workflow.calls))

    def test_post_codex_commit_verification_gates_push(self):
        class InvalidCommit(FakeWorkflow):
            def verify_commit(self, branch, start_sha, reported_sha):
                raise tbx_implement.WorkflowError("post_codex", "unexpected HEAD")

        workflow = InvalidCommit()
        with self.assertRaises(tbx_implement.WorkflowError) as error:
            workflow.run_issue(1917)
        self.assertEqual(error.exception.phase, "post_codex")
        self.assertFalse(any(call[:2] == ("git", "push") for call in workflow.calls))

    def test_verify_commit_checks_branch_head_parent_count_and_clean_worktree(self):
        class VerifyRunner:
            values = {}

            def __call__(self, args, **kwargs):
                args = tuple(args)
                key = args[1:]
                defaults = {
                    ("branch", "--show-current"): "issue/1917-implement",
                    ("rev-parse", "HEAD"): "new-sha",
                    ("rev-parse", "HEAD^"): "base-sha",
                    ("rev-list", "--count", "base-sha..HEAD"): "1",
                    ("status", "--porcelain=v1", "--untracked-files=all"): "",
                }
                return tbx_implement.CommandResult(0, self.values.get(key, defaults[key]))

        runner = VerifyRunner()
        workflow = tbx_implement.IssueWorkflow(runner)
        workflow.verify_commit("issue/1917-implement", "base-sha", "new-sha")
        invalid_states = (
            ({("branch", "--show-current"): "other"}, "new-sha"),
            ({("rev-parse", "HEAD"): "other"}, "new-sha"),
            ({("rev-parse", "HEAD^"): "other"}, "new-sha"),
            ({("rev-list", "--count", "base-sha..HEAD"): "2"}, "new-sha"),
            ({("status", "--porcelain=v1", "--untracked-files=all"): "?? leftover"}, "new-sha"),
            ({}, "reported-sha-mismatch"),
        )
        for values, reported_sha in invalid_states:
            with self.subTest(values=values, reported_sha=reported_sha):
                runner.values = values
                with self.assertRaises(tbx_implement.WorkflowError):
                    workflow.verify_commit("issue/1917-implement", "base-sha", reported_sha)

    def test_push_failure_does_not_create_pr(self):
        workflow = FakeWorkflow(push_fails=True)
        with self.assertRaises(tbx_implement.WorkflowError) as error:
            workflow.run_issue(1917)
        self.assertEqual(error.exception.phase, "push")
        self.assertFalse(any(call[:3] == ("gh", "pr", "create") for call in workflow.calls))

    def test_pr_failure_happens_only_after_successful_push(self):
        workflow = FakeWorkflow(pr_fails=True)
        with self.assertRaises(tbx_implement.WorkflowError) as error:
            workflow.run_issue(1917)
        self.assertEqual(error.exception.phase, "pr_create")
        self.assertTrue(any(call[:3] == ("git", "push", "--set-upstream") for call in workflow.calls))

    def test_issue_fetch_failure_does_not_create_branch(self):
        workflow = FakeWorkflow(issue_fetch_fails=True)
        with self.assertRaises(tbx_implement.WorkflowError):
            workflow.run_issue(1917)
        self.assert_no_later_side_effects(workflow)

    def test_only_open_implementation_issue_is_accepted_before_side_effects(self):
        rejected_inputs = (
            {"issue_kind": "ADR"},
            {"issue_kind": "調査・計画"},
            {"issue_body": "種別表示なし"},
            {"issue_state": "CLOSED"},
            {"returned_issue_number": 9999},
        )
        for options in rejected_inputs:
            with self.subTest(options=options):
                workflow = FakeWorkflow(**options)
                with self.assertRaises(tbx_implement.WorkflowError):
                    workflow.run_issue(1917)
                self.assert_no_later_side_effects(workflow)

        accepted = FakeWorkflow(issue_state="OPEN", issue_kind="実装")
        self.assertEqual(accepted.run_issue(1917)["status"], "success")

    def assert_no_later_side_effects(self, workflow):
        self.assertFalse(any(call[:3] == ("git", "switch", "-c") for call in workflow.calls))
        self.assertFalse(any(call[:2] == ("codex", "exec") for call in workflow.calls))
        self.assertFalse(any(call[:2] == ("git", "push") for call in workflow.calls))
        self.assertFalse(any(call[:3] == ("gh", "pr", "create") for call in workflow.calls))

    def test_linked_issues_and_prs_are_fetched_separately(self):
        workflow = tbx_implement.IssueWorkflow()
        responses = iter([
            {"number": 1917, "title": "Implement", "body": "**種別: 実装**\nSee #1916 and PR #1908", "state": "OPEN", "url": "", "comments": []},
            {"number": 1908, "title": "Prior PR"},
            {"number": 1916, "title": "ADR"},
        ])
        workflow.gh_json = lambda phase, args: next(responses)
        issue, linked_issues, linked_prs = workflow.fetch_issue(1917)
        self.assertIn("1916", linked_issues)
        self.assertIn("1908", linked_prs)


if __name__ == "__main__":
    unittest.main()
