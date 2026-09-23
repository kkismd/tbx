import sys
import unittest
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


if __name__ == "__main__":
    unittest.main()
