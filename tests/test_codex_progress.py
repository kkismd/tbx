import io
import json
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import tbx_codex_progress as progress


class CodexProgressTests(unittest.TestCase):
    def test_known_events_are_rendered_and_unknown_events_are_ignored(self):
        cases = [
            ({"type": "item.completed", "item": {"type": "reasoning", "text": "Inspecting"}}, "Codex要約: Inspecting"),
            ({"type": "item.completed", "item": {"type": "agent_message", "text": "Found the issue"}}, "Codex: Found the issue"),
            ({"type": "item.started", "item": {"type": "command_execution", "command": "cargo test"}}, "コマンド開始: cargo test"),
            ({"type": "item.completed", "item": {"type": "command_execution", "command": "cargo test", "status": "failed"}}, "コマンド失敗: cargo test"),
            ({"type": "item.completed", "item": {"type": "file_change", "changes": [{"path": "src/main.rs"}]}}, "ファイル変更: src/main.rs"),
            ({"type": "item.completed", "item": {"type": "todo_list", "title": "Plan updated"}}, "計画更新: Plan updated"),
            ({"type": "error", "message": "failed"}, "エラー: failed"),
        ]
        for event, expected in cases:
            with self.subTest(event=event):
                self.assertEqual(progress.describe_event(event), expected)
        self.assertIsNone(progress.describe_event({"type": "future.event", "data": 1}))

    def test_jsonl_events_are_rendered_incrementally_and_stdout_is_not_returned(self):
        lines = [
            json.dumps({"type": "item.started", "item": {"type": "command_execution", "command": "cargo test"}}) + "\n",
            json.dumps({"type": "item.completed", "item": {"type": "command_execution", "command": "cargo test"}}) + "\n",
            json.dumps({"type": "future.event"}) + "\n",
        ]

        class Process:
            stdin = None
            stderr = None
            stdout = iter(lines)

            def wait(self):
                return 0

        with patch.object(progress.subprocess, "Popen", return_value=Process()), patch.object(progress, "progress") as display:
            result = progress.run_codex_jsonl(["codex", "exec", "--json"], cwd=Path("."))
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual([call.args[0] for call in display.call_args_list], ["コマンド開始: cargo test", "コマンド完了: cargo test"])

    def test_nonzero_exit_retains_stderr_then_jsonl_error_then_exit_code(self):
        cases = (
            ("stderr cause", '{"type":"error","message":"stdout cause"}\n', 7, "stderr cause", "stdout cause"),
            ("", '{"type":"error","message":"stdout cause"}\n', 7, "", "stdout cause"),
            ("", "", 7, "", ""),
        )
        for stderr_text, stdout_line, returncode, expected_stderr, expected_stdout in cases:
            class Process:
                stdin = None
                stderr = io.StringIO(stderr_text)
                stdout = iter([stdout_line] if stdout_line else [])

                def wait(self):
                    return returncode

            with self.subTest(stderr=stderr_text, stdout=stdout_line), patch.object(progress.subprocess, "Popen", return_value=Process()):
                captured_stdout = io.StringIO()
                captured_stderr = io.StringIO()
                with redirect_stdout(captured_stdout), redirect_stderr(captured_stderr):
                    result = progress.run_codex_jsonl(["codex", "exec", "--json"], cwd=Path("."))
            self.assertEqual(captured_stdout.getvalue(), "")
            if stderr_text:
                self.assertIn("stderr cause", captured_stderr.getvalue())
            self.assertEqual(result.returncode, returncode)
            self.assertEqual(result.stderr, expected_stderr)
            self.assertEqual(result.stdout, expected_stdout)

    def test_stderr_diagnostic_is_bounded(self):
        class Process:
            stdin = None
            stderr = io.StringIO("x" * 3_000)
            stdout = iter([])

            def wait(self):
                return 1

        with patch.object(progress.subprocess, "Popen", return_value=Process()), redirect_stderr(io.StringIO()):
            result = progress.run_codex_jsonl(["codex", "exec", "--json"], cwd=Path("."))
        self.assertEqual(len(result.stderr), progress.MAX_DIAGNOSTIC_CHARS)

    def test_nested_turn_failure_diagnostic_is_extracted(self):
        self.assertEqual(
            progress.event_diagnostic({"type": "turn.failed", "error": {"message": "nested failure"}}),
            "nested failure",
        )


if __name__ == "__main__":
    unittest.main()
