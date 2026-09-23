import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import tbx_implement


class FakeWorkflow(tbx_implement.IssueWorkflow):
    def __init__(self, *, codex_status="success", checks=None, codex_exit=0, push_fails=False, pr_fails=False):
        self.calls = []
        self.codex_status = codex_status
        self.checks = list(tbx_implement.REQUIRED_CHECKS) if checks is None else checks
        self.codex_exit = codex_exit
        self.push_fails = push_fails
        self.pr_fails = pr_fails
        self.branch = "main"
        super().__init__(self.fake_run)

    def fake_run(self, args, **kwargs):
        args = tuple(args)
        self.calls.append(args)
        if args[:3] == ("git", "show-ref", "--verify"):
            return tbx_implement.CommandResult(1, "")
        if args[:3] == ("git", "switch", "-c"):
            self.branch = args[3]
            return tbx_implement.CommandResult(0, "")
        if args[:2] == ("codex", "exec"):
            if self.codex_exit:
                return tbx_implement.CommandResult(self.codex_exit, "", "Codex failed")
            schema = args[args.index("--output-last-message") + 1]
            result = {
                "status": self.codex_status,
                "summary": "Implemented issue.",
                "checks_passed": self.checks if self.codex_status == "success" else [],
                "commit_sha": "new-sha" if self.codex_status == "success" else None,
            }
            Path(schema).write_text(json.dumps(result), encoding="utf-8")
            return tbx_implement.CommandResult(0, "{}\n")
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

    def fetch_issue(self, number):
        self.calls.append(("fetch-issue", number))
        return ({"number": number, "title": "Work item"}, {}, {})

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
        class FetchFailure(FakeWorkflow):
            def fetch_issue(self, number):
                raise tbx_implement.WorkflowError("issue_fetch", "unavailable")

        workflow = FetchFailure()
        with self.assertRaises(tbx_implement.WorkflowError):
            workflow.run_issue(1917)
        self.assertFalse(any(call[:3] == ("git", "switch", "-c") for call in workflow.calls))

    def test_linked_issues_and_prs_are_fetched_separately(self):
        workflow = tbx_implement.IssueWorkflow()
        responses = iter([
            {"number": 1917, "title": "Implement", "body": "See #1916 and PR #1908", "state": "OPEN", "url": "", "comments": []},
            {"number": 1908, "title": "Prior PR"},
            {"number": 1916, "title": "ADR"},
        ])
        workflow.gh_json = lambda phase, args: next(responses)
        issue, linked_issues, linked_prs = workflow.fetch_issue(1917)
        self.assertIn("1916", linked_issues)
        self.assertIn("1908", linked_prs)


if __name__ == "__main__":
    unittest.main()
