import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import tbx_codex_progress as progress


class CodexProgressTests(unittest.TestCase):
    def test_known_events_are_rendered_and_unknown_events_are_ignored(self):
        cases = [
            ({"type": "item.completed", "item": {"type": "reasoning", "text": "Inspecting"}}, "Codex要約: Inspecting"),
            ({"type": "item.completed", "item": {"type": "agent_message", "text": "Found the issue"}}, "Codex: Found the issue"),
            ({"type": "item.started", "item": {"type": "command_execution", "command": "cargo test"}}, "コマンド: cargo test"),
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
            stdout = iter(lines)

            def wait(self):
                return 0

        with patch.object(progress.subprocess, "Popen", return_value=Process()), patch.object(progress, "progress") as display:
            result = progress.run_codex_jsonl(["codex", "exec", "--json"], cwd=Path("."))
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual([call.args[0] for call in display.call_args_list], ["コマンド: cargo test", "コマンド完了: cargo test"])


if __name__ == "__main__":
    unittest.main()
