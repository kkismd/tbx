"""Shared Codex JSONL execution and human-readable progress reporting."""

from __future__ import annotations

import json
import subprocess
import sys
from dataclasses import dataclass
from typing import Sequence


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str
    stderr: str = ""


def progress(message: str) -> None:
    print(f"[tbx-workflow] {message}", file=sys.stderr, flush=True)


def _text(value: object) -> str:
    if isinstance(value, str):
        return value.strip()
    return ""


def describe_event(event: object) -> str | None:
    """Translate known Codex CLI JSONL events; unknown event types are ignored."""
    if not isinstance(event, dict):
        return None
    event_type = event.get("type")
    item = event.get("item")
    if not isinstance(item, dict):
        item = event
    item_type = item.get("type")
    status = "" if event_type == "item.started" else "完了"
    if event_type == "item.completed" and item.get("status") in ("failed", "error"):
        status = "失敗"
    if item_type in ("reasoning", "reasoning_summary"):
        detail = _text(item.get("text")) or _text(item.get("summary"))
        return f"Codex要約: {detail}" if detail else None
    if item_type in ("agent_message", "message"):
        detail = _text(item.get("text")) or _text(item.get("message"))
        try:
            structured = json.loads(detail)
        except (TypeError, json.JSONDecodeError):
            structured = None
        if isinstance(structured, dict) and "status" in structured and ("commit_sha" in structured or "results" in structured):
            return "Codexの構造化終了結果を受信"
        return f"Codex: {detail}" if detail else None
    if item_type in ("command_execution", "command"):
        command = _text(item.get("command")) or _text(item.get("title")) or "command"
        if event_type in ("item.started", "item.completed"):
            return f"コマンド{status}: {command}"
    if item_type in ("file_change", "file_changes"):
        changes = item.get("changes")
        paths = [row.get("path") for row in changes if isinstance(row, dict) and isinstance(row.get("path"), str)] if isinstance(changes, list) else []
        detail = ", ".join(paths) or _text(item.get("path"))
        return f"ファイル変更: {detail}" if detail else "ファイル変更"
    if item_type in ("todo_list", "plan", "plan_update"):
        detail = _text(item.get("text")) or _text(item.get("title"))
        return f"計画更新: {detail}" if detail else "計画更新"
    if event_type in ("error", "turn.failed") or item_type == "error":
        detail = _text(event.get("message")) or _text(item.get("message")) or "Codexエラー"
        return f"エラー: {detail}"
    return None


def run_codex_jsonl(args: Sequence[str], *, cwd, input_text: str | None = None):
    """Run Codex and render each recognized JSONL event as it arrives."""
    try:
        process = subprocess.Popen(
            list(args), cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=None, text=True, bufsize=1,
        )
    except OSError as error:
        return CommandResult(127, "", str(error))
    if process.stdin is not None:
        try:
            if input_text is not None:
                process.stdin.write(input_text)
            process.stdin.close()
        except BrokenPipeError:
            pass
    for line in process.stdout or ():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            progress("未解釈のCodex出力を受信")
            continue
        message = describe_event(event)
        if message:
            progress(message)
    return CommandResult(process.wait(), "", "")
