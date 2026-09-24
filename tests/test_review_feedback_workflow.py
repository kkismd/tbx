import sys
import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
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

    def test_review_result_comments_are_excluded_even_when_marker_is_malformed(self):
        for body in (
            "<!-- tbx-w01-review-result:v1 head=" + "a" * 40 + " result=no_additional_changes -->",
            "<!-- tbx-w01-review-result:v2 head=" + "a" * 40 + " result=no_additional_changes -->",
            "<!-- tbx-w01-review-result:v1 head=bad result=unknown -->",
            "broken tbx-w01-review-result marker",
        ):
            self.assertTrue(feedback.is_review_result_comment(body))
        self.assertFalse(feedback.is_review_result_comment("no_change feedback"))


class ReviewResultTests(unittest.TestCase):
    HEAD = "a" * 40

    def comment(self, identifier, result, posted="2026-01-02T00:00:00Z", *, head=None):
        target = head or self.HEAD
        body = f"<!-- tbx-w01-review-result:v1 head={target} result={result} -->"
        return {"id": identifier, "body": body, "created_at": posted}

    def derive(self, comments, messages=(), head=None):
        pr = {"headRefOid": head or self.HEAD}
        return feedback.derive_review_result(pr, comments, list(messages))

    def test_missing_marker_does_not_infer_result_from_free_text_or_no_change(self):
        self.assertEqual(self.derive([]), {"status": "none", "head_sha": self.HEAD})
        comments = [{"id": 1, "body": "LGTM merge ok", "created_at": "2026-01-02T00:00:00Z"}]
        message = {"kind": "issue-comment", "id": 1, "body": "no_change", "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"}
        self.assertEqual(self.derive(comments, [message])["status"], "none")

    def test_only_current_head_and_well_formed_v1_markers_are_accepted(self):
        comments = [
            self.comment(10, "no_additional_changes", head="b" * 40),
            {"id": 11, "body": f"<!-- tbx-w01-review-result:v2 head={self.HEAD} result=no_additional_changes -->", "created_at": "2026-01-03T00:00:00Z"},
            {"id": 12, "body": f"<!-- tbx-w01-review-result:v1 head={self.HEAD[:-1]}x result=no_additional_changes -->", "created_at": "2026-01-04T00:00:00Z"},
            self.comment(13, "changes_required"),
        ]
        self.assertEqual(self.derive(comments)["status"], "changes_required")
        self.assertEqual(self.derive(comments, head="b" * 40)["status"], "no_additional_changes")

    def test_latest_same_head_marker_uses_time_then_numeric_comment_id(self):
        comments = [
            self.comment(20, "changes_required", "2026-01-02T00:00:00Z"),
            self.comment(21, "no_additional_changes", "2026-01-03T00:00:00Z"),
            self.comment(22, "changes_required", "2026-01-03T00:00:00Z"),
        ]
        self.assertEqual(self.derive(comments)["status"], "changes_required")

    def test_new_or_edited_feedback_after_marker_invalidates_result(self):
        marker = self.comment(20, "no_additional_changes")
        for message in (
            {"kind": "issue-comment", "id": 1, "createdAt": "2026-01-03T00:00:00Z", "updatedAt": "2026-01-03T00:00:00Z"},
            {"kind": "review", "id": 2, "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-03T00:00:00Z"},
            {"kind": "review-comment", "id": 3, "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-03T00:00:00Z"},
        ):
            self.assertEqual(self.derive([marker], [message])["status"], "none")

    def test_feedback_before_marker_does_not_invalidate_and_cannot_override_newer_marker(self):
        marker = self.comment(20, "no_additional_changes")
        old_message = {"kind": "issue-comment", "id": 1, "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T12:00:00Z"}
        self.assertEqual(self.derive([marker], [old_message])["status"], "no_additional_changes")
        newer = self.comment(21, "changes_required", "2026-01-04T00:00:00Z")
        self.assertEqual(self.derive([marker, newer], [old_message])["status"], "changes_required")


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
    def test_cli_reports_codex_interruption_once_with_worktree_state(self):
        stdout = io.StringIO()
        with patch.object(feedback.ReviewFeedbackWorkflow, "run_pr", side_effect=feedback.CodexInterrupted()), \
                patch.object(feedback, "interruption_git_state", return_value={"branch": "feature", "head": "def", "worktree": "clean"}), \
                redirect_stdout(stdout), redirect_stderr(io.StringIO()):
            code = feedback.main(["42"])
        result = json.loads(stdout.getvalue())
        self.assertEqual(code, 130)
        self.assertEqual(result["status"], "interrupted")
        self.assertEqual(result["stage"], "codex_execution")
        self.assertEqual((result["branch"], result["head"], result["worktree"]), ("feature", "def", "clean"))
        self.assertTrue(result["codex_process_group_stopped"])
        self.assertEqual(len(stdout.getvalue().splitlines()), 1)

    def test_codex_interrupt_skips_verification_push_and_processing_record(self):
        outcome = self.result("success", "fixed", modified=["issue-comment:44@2026-01-01T00:00:00Z"], checks=list(feedback.REQUIRED_CHECKS), commit="b" * 40)
        runner = self.Runner(outcome, interrupt_codex=True)
        workflow = self.Workflow(runner)
        with self.assertRaises(feedback.CodexInterrupted):
            workflow.run_pr(42)
        self.assertEqual(runner.events, ["codex"])

    class Runner:
        def __init__(self, outcome=None, *, codex_result=None, fail_push=False, fail_record=False, interrupt_codex=False):
            self.outcome = outcome
            self.codex_result = codex_result or feedback.CommandResult(0, "")
            self.fail_push = fail_push
            self.fail_record = fail_record
            self.interrupt_codex = interrupt_codex
            self.events = []
            self.prompt = ""

        def __call__(self, args, *, input_text=None):
            args = tuple(args)
            if args[:2] == ("codex", "exec"):
                self.events.append("codex")
                self.prompt = input_text
                if self.interrupt_codex:
                    raise feedback.CodexInterrupted()
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
        self.assertEqual(result["review_result"], {"status": "none", "head_sha": None})
        self.assertEqual(runner.events, [])

    def test_no_candidates_returns_structured_review_result_without_running_codex(self):
        workflow, runner = self.workflow()
        head = "a" * 40
        marker = {"id": 50, "body": f"<!-- tbx-w01-review-result:v1 head={head} result=no_additional_changes -->", "created_at": "2026-01-02T00:00:00Z"}
        workflow.fetch_snapshot = lambda number: ({"headRefName": "topic", "headRefOid": head, "comments": [marker], "diff": ""}, [])
        result = workflow.run_pr(42)
        self.assertEqual(result["review_result"], {"status": "no_additional_changes", "head_sha": head})
        self.assertEqual(runner.events, [])

    def test_fetch_snapshot_uses_graphql_review_updated_at_to_invalidate_marker(self):
        head = "a" * 40
        marker = {"id": 50, "body": f"<!-- tbx-w01-review-result:v1 head={head} result=no_additional_changes -->", "created_at": "2026-01-02T00:00:00Z"}
        review = {"id": 60, "body": "review body", "submitted_at": "2026-01-01T00:00:00Z", "html_url": "review-url"}

        class Runner:
            def __call__(self, args, *, input_text=None):
                args = tuple(args)
                if args[:3] == ("gh", "pr", "view"):
                    return feedback.CommandResult(0, json.dumps({"number": 42, "state": "OPEN", "mergedAt": None, "headRefName": "topic", "headRefOid": head, "body": ""}))
                if args[:3] == ("gh", "repo", "view"):
                    return feedback.CommandResult(0, json.dumps({"nameWithOwner": "owner/repo"}))
                if args[:3] == ("gh", "pr", "diff"):
                    return feedback.CommandResult(0, "diff")
                if args[:4] == ("gh", "api", "--paginate", "--slurp"):
                    endpoint = args[4]
                    rows = [marker] if "issues/42/comments" in endpoint else [review] if "pulls/42/reviews" in endpoint else []
                    return feedback.CommandResult(0, json.dumps([rows]))
                if args[:3] == ("gh", "api", "graphql"):
                    self.query = args[args.index("-f") + 1]
                    payload = {"data": {"repository": {"pullRequest": {
                        "reviews": {"nodes": [{"databaseId": 60, "updatedAt": "2026-01-03T00:00:00Z"}]},
                        "reviewThreads": {"nodes": []},
                    }}}}
                    return feedback.CommandResult(0, json.dumps(payload))
                raise AssertionError(args)

        runner = Runner()
        workflow = feedback.ReviewFeedbackWorkflow(runner)
        pr, messages = workflow.fetch_snapshot(42)
        self.assertIn("updatedAt", runner.query)
        self.assertEqual(messages[0]["updatedAt"], "2026-01-03T00:00:00Z")
        self.assertEqual(feedback.derive_review_result(pr, pr["comments"], messages), {"status": "none", "head_sha": head})

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

    def test_failed_codex_status_preserves_summary_and_does_not_record(self):
        workflow, runner = self.workflow(self.result("failed", "no_change", reason="原因A"))
        with self.assertRaisesRegex(feedback.WorkflowError, "原因A") as raised:
            workflow.run_pr(42)
        self.assertEqual(raised.exception.phase, "codex")
        self.assertEqual(runner.events, ["codex"])

    def test_failed_codex_summary_is_preserved_in_final_json_error(self):
        stdout = io.StringIO()
        stderr = io.StringIO()
        with patch.object(feedback.ReviewFeedbackWorkflow, "run_pr", side_effect=feedback.WorkflowError("codex", "原因A")):
            with redirect_stdout(stdout), redirect_stderr(stderr):
                exit_code = feedback.main(["42"])
        self.assertEqual(exit_code, 1)
        self.assertEqual(json.loads(stdout.getvalue()), {"status": "failed", "phase": "codex", "error": "原因A"})
        self.assertIn("workflow failed: codex", stderr.getvalue())

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
