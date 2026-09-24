import sys
import unittest
from unittest.mock import patch
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import tbx_review_feedback as feedback


class FeedbackCandidateTests(unittest.TestCase):
    def setUp(self):
        self.messages = [
            {"kind": "issue-comment", "id": 20, "body": "second", "createdAt": "2026-01-02T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z"},
            {"kind": "review-comment", "id": 10, "body": "inline", "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"},
        ]

    def test_candidates_are_ordered_and_revision_scoped(self):
        candidates = feedback.extract_candidates(self.messages, [], {})
        self.assertEqual([feedback.message_key(row) for row in candidates], ["review-comment:10", "issue-comment:20"])
        edited = [dict(self.messages[0], updatedAt="2026-01-03T00:00:00Z")]
        records = [{"body": feedback.RECORD_MARKER + '{"head_sha":"head","results":[{"revision":"issue-comment:20@2026-01-02T00:00:00Z","result":"fixed"}]}'}]
        self.assertEqual(feedback.extract_candidates(edited, records, {}), edited)

    def test_processing_record_is_skipped_but_thread_state_is_not_a_completion_signal(self):
        record = {"body": feedback.RECORD_MARKER + '{"head_sha":"head","results":[{"revision":"review-comment:10@2026-01-01T00:00:00Z","result":"fixed"}]}'}
        self.assertEqual(feedback.extract_candidates(self.messages, [record], {}), [self.messages[0]])

    def test_trailer_prevents_duplicate_after_record_post_failure_but_later_edit_is_candidate(self):
        trailers = {"issue-comment:20": "2026-01-02T01:00:00Z"}
        self.assertEqual(feedback.extract_candidates([self.messages[0]], [], trailers), [])
        edited = [dict(self.messages[0], updatedAt="2026-01-03T00:00:00Z")]
        self.assertEqual(feedback.extract_candidates(edited, [], trailers), edited)

    def test_thread_resolution_and_outdated_flags_do_not_decide_processing(self):
        message = dict(self.messages[1], threadResolved=True, threadOutdated=True)
        self.assertEqual(feedback.extract_candidates([message], [], {}), [message])

    def test_record_is_reused_only_when_recorded_head_is_in_current_history(self):
        message = self.messages[1]
        record = {"body": feedback.RECORD_MARKER + '{"head_sha":"old-head","results":[{"revision":"review-comment:10@2026-01-01T00:00:00Z","result":"fixed"}]}'}
        self.assertEqual(feedback.extract_candidates([message], [record], {}, {"old-head"}), [])
        self.assertEqual(feedback.extract_candidates([message], [record], {}, set()), [message])


class FeedbackSchemaTests(unittest.TestCase):
    def test_every_object_schema_disallows_additional_properties(self):
        def assert_object_constraints(schema):
            if isinstance(schema, dict):
                if schema.get("type") == "object":
                    self.assertIs(schema.get("additionalProperties"), False)
                for value in schema.values():
                    assert_object_constraints(value)
            elif isinstance(schema, list):
                for value in schema:
                    assert_object_constraints(value)

        assert_object_constraints(feedback.RESULT_SCHEMA)


class FeedbackPreflightTests(unittest.TestCase):
    def test_clean_matching_head_is_required(self):
        class Runner:
            values = {}
            def __call__(self, args, **kwargs):
                args = tuple(args)
                response = {
                    ("git", "rev-parse", "--show-toplevel"): str(feedback.REPO_ROOT),
                    ("git", "branch", "--show-current"): "topic",
                    ("git", "status", "--porcelain=v1", "--untracked-files=all"): "",
                    ("git", "rev-parse", "HEAD"): "head",
                }
                if args[:3] == ("git", "rev-parse", "--git-path"):
                    return feedback.CommandResult(0, str(feedback.REPO_ROOT / ".git" / args[3]))
                return feedback.CommandResult(0, self.values.get(args, response.get(args, "")))
        runner = Runner()
        workflow = feedback.ReviewFeedbackWorkflow(runner)
        pr = {"headRefName": "topic", "headRefOid": "head"}
        self.assertEqual(workflow.preflight(pr), "head")
        runner.values[("git", "status", "--porcelain=v1", "--untracked-files=all")] = " M file"
        with self.assertRaises(feedback.WorkflowError):
            workflow.preflight(pr)
        runner.values[("git", "status", "--porcelain=v1", "--untracked-files=all")] = ""
        runner.values[("git", "rev-parse", "HEAD")] = "other"
        with self.assertRaises(feedback.WorkflowError):
            workflow.preflight(pr)


class FeedbackRunPrTests(unittest.TestCase):
    class Runner:
        def __init__(self, outcome=None, *, codex_result=None, fail_push=False, fail_record=False):
            self.outcome = outcome
            self.codex_result = codex_result or feedback.CommandResult(0, "")
            self.fail_push = fail_push
            self.fail_record = fail_record
            self.events = []
            self.prompt = ""

        def __call__(self, args, *, input_text=None):
            args = tuple(args)
            if args[:2] == ("codex", "exec"):
                self.events.append("codex")
                self.prompt = input_text
                if self.codex_result.returncode:
                    return self.codex_result
                result_path = Path(args[args.index("--output-last-message") + 1])
                result_path.write_text(__import__("json").dumps(self.outcome), encoding="utf-8")
                return feedback.CommandResult(0, "")
            if args[:3] == ("git", "push", "origin"):
                self.events.append("push")
                return feedback.CommandResult(1, "", "push failed") if self.fail_push else feedback.CommandResult(0, "")
            if args[:3] == ("gh", "pr", "comment"):
                self.events.append("record")
                self.record_body = args[-1]
                return feedback.CommandResult(1, "", "record failed") if self.fail_record else feedback.CommandResult(0, "")
            raise AssertionError(args)

    class Workflow(feedback.ReviewFeedbackWorkflow):
        def __init__(self, runner, *, verify_error=None):
            super().__init__(runner)
            self.verify_error = verify_error
            self.fetches = 0

        def fetch_snapshot(self, number):
            self.fetches += 1
            message = {"kind": "issue-comment", "id": 44, "body": "please fix", "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"}
            return ({"headRefName": "topic", "comments": [], "diff": "snapshot"}, [message])

        def preflight(self, pr):
            return "a" * 40

        def trailer_keys(self, start_sha):
            return {}

        def is_ancestor(self, ancestor, descendant):
            return True

        def verify_commit(self, branch, start_sha, reported_sha, expected_trailers):
            self.run.events.append("verify")
            if self.verify_error:
                raise feedback.WorkflowError("post_codex", self.verify_error)

    @staticmethod
    def result(status="success", result="no_change", *, modified=None, checks=None, commit=None, reason="reviewed"):
        return {"status": status, "summary": reason, "results": [{"revision": "issue-comment:44@2026-01-01T00:00:00Z", "result": result, "reason": reason}],
                "modified_messages": modified or [], "checks_passed": checks or [], "commit_sha": commit}

    def workflow(self, outcome=None, *, verify_error=None, **kwargs):
        runner = self.Runner(outcome, **kwargs)
        return self.Workflow(runner, verify_error=verify_error), runner

    def test_no_candidates_has_no_codex_push_or_record(self):
        workflow, runner = self.workflow()
        workflow.fetch_snapshot = lambda number: ({"headRefName": "topic", "comments": [], "diff": ""}, [])
        result = workflow.run_pr(42)
        self.assertEqual(result["status"], "success")
        self.assertEqual(runner.events, [])

    def test_fixed_verifies_before_push_then_records(self):
        workflow, runner = self.workflow(self.result("success", "fixed", modified=["issue-comment:44@2026-01-01T00:00:00Z"], checks=list(feedback.REQUIRED_CHECKS), commit="b" * 40))
        workflow.run_pr(42)
        self.assertEqual(runner.events, ["codex", "verify", "push", "record"])

    def test_successful_workflow_reports_major_stages_in_order(self):
        outcome = self.result("success", "fixed", modified=["issue-comment:44@2026-01-01T00:00:00Z"], checks=list(feedback.REQUIRED_CHECKS), commit="b" * 40)
        workflow, _ = self.workflow(outcome)
        with patch.object(feedback, "progress") as display:
            workflow.run_pr(42)
        messages = [call.args[0] for call in display.call_args_list]
        stages = ["事前確認が完了", "Codex実行開始", "Codex実行終了", "Codex後のGit検証が完了", "push完了", "処理記録投稿完了"]
        positions = [next(i for i, message in enumerate(messages) if message.startswith(stage)) for stage in stages]
        self.assertEqual(positions, sorted(positions))

    def test_no_change_records_without_commit_or_push(self):
        workflow, runner = self.workflow(self.result())
        workflow.run_pr(42)
        self.assertEqual(runner.events, ["codex", "record"])

    def test_human_review_records_without_changes_and_remains_candidate(self):
        workflow, runner = self.workflow(self.result("human_review_required", "human_review_required", reason="design conflict"))
        result = workflow.run_pr(42)
        self.assertEqual(result["status"], "human_review_required")
        self.assertEqual(runner.events, ["codex", "record"])
        self.assertIn("確認時head:", runner.record_body)
        self.assertIn("human_review_required", runner.record_body)
        self.assertEqual(feedback.extract_candidates(workflow.fetch_snapshot(42)[1], [{"body": runner.record_body}], {}), workflow.fetch_snapshot(42)[1])

    def test_codex_checks_and_commit_validation_failures_stop_before_push_and_record(self):
        invalid_checks = self.result("success", "fixed", modified=["issue-comment:44@2026-01-01T00:00:00Z"], checks=[], commit="b" * 40)
        workflow, runner = self.workflow(invalid_checks)
        with self.assertRaises(feedback.WorkflowError):
            workflow.run_pr(42)
        self.assertEqual(runner.events, ["codex"])
        valid = self.result("success", "fixed", modified=["issue-comment:44@2026-01-01T00:00:00Z"], checks=list(feedback.REQUIRED_CHECKS), commit="b" * 40)
        workflow, runner = self.workflow(valid, verify_error="HEAD mismatch")
        with self.assertRaises(feedback.WorkflowError):
            workflow.run_pr(42)
        self.assertEqual(runner.events, ["codex", "verify"])

    def test_each_post_codex_git_mismatch_blocks_push_and_record(self):
        result = self.result("success", "fixed", modified=["issue-comment:44@2026-01-01T00:00:00Z"], checks=list(feedback.REQUIRED_CHECKS), commit="b" * 40)
        for mismatch in ("branch changed", "HEAD mismatch", "worktree dirty", "ancestry mismatch", "trailer mismatch"):
            workflow, runner = self.workflow(result, verify_error=mismatch)
            with self.subTest(mismatch=mismatch), self.assertRaises(feedback.WorkflowError):
                workflow.run_pr(42)
            self.assertEqual(runner.events, ["codex", "verify"])

    def test_push_failure_stops_before_processing_record(self):
        result = self.result("success", "fixed", modified=["issue-comment:44@2026-01-01T00:00:00Z"], checks=list(feedback.REQUIRED_CHECKS), commit="b" * 40)
        workflow, runner = self.workflow(result, fail_push=True)
        with self.assertRaises(feedback.WorkflowError):
            workflow.run_pr(42)
        self.assertEqual(runner.events, ["codex", "verify", "push"])

    def test_record_failure_keeps_push_and_prompt_uses_only_frozen_snapshot(self):
        workflow, runner = self.workflow(self.result(), fail_record=True)
        with self.assertRaises(feedback.WorkflowError):
            workflow.run_pr(42)
        self.assertEqual(runner.events, ["codex", "record"])
        self.assertEqual(workflow.fetches, 1)
        self.assertIn("issue-comment:44@2026-01-01T00:00:00Z", runner.prompt)

    def test_successful_push_without_record_is_deduplicated_by_trailer(self):
        revision = "issue-comment:44@2026-01-01T00:00:00Z"
        outcome = self.result("success", "fixed", modified=[revision], checks=list(feedback.REQUIRED_CHECKS), commit="b" * 40)
        workflow, runner = self.workflow(outcome, fail_record=True)
        with self.assertRaises(feedback.WorkflowError):
            workflow.run_pr(42)
        self.assertEqual(runner.events, ["codex", "verify", "push", "record"])
        message = {"kind": "issue-comment", "id": 44, "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"}
        self.assertEqual(feedback.extract_candidates([message], [], {"issue-comment:44": "2026-01-02T00:00:00Z"}), [])

    def test_failed_codex_status_does_not_record(self):
        workflow, runner = self.workflow(self.result("failed", "no_change"))
        with self.assertRaises(feedback.WorkflowError):
            workflow.run_pr(42)
        self.assertEqual(runner.events, ["codex"])

    def test_codex_nonzero_errors_use_stderr_stdout_or_exit_code(self):
        for codex_result, expected in (
            (feedback.CommandResult(1, "ignored", "stderr cause"), "stderr cause"),
            (feedback.CommandResult(1, '{"type":"error","message":"stdout cause"}', ""), "stdout cause"),
            (feedback.CommandResult(7, "", ""), "exit code 7"),
        ):
            workflow, _ = self.workflow(self.result(), codex_result=codex_result)
            with self.subTest(expected=expected), self.assertRaisesRegex(feedback.WorkflowError, expected):
                workflow.run_pr(42)


if __name__ == "__main__":
    unittest.main()
