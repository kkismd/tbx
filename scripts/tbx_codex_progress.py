"""Shared Codex JSONL execution and human-readable progress reporting."""

from __future__ import annotations

import codecs
import json
import os
import signal
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence

MAX_DIAGNOSTIC_CHARS = 2_000
INTERRUPT_GRACE_SECONDS = 5


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str
    stderr: str = ""


class CodexInterrupted(Exception):
    """Raised after an interrupted Codex process group has been stopped."""


class CodexStopError(Exception):
    """Raised when the interrupted Codex process group could not be confirmed stopped."""


def interruption_git_state(cwd: Path) -> dict[str, object]:
    """Return a best-effort local Git snapshot without changing repository state."""
    def git(*args: str) -> str | None:
        try:
            result = subprocess.run(("git", *args), cwd=cwd, text=True, capture_output=True, check=False)
        except OSError:
            return None
        return result.stdout.strip() if result.returncode == 0 else None

    branch = git("branch", "--show-current")
    head = git("rev-parse", "HEAD")
    status = git("status", "--porcelain=v1", "--untracked-files=all")
    return {
        "branch": branch,
        "head": head,
        "worktree": "unknown" if status is None else "dirty" if status else "clean",
    }


def progress(message: str) -> None:
    print(f"[tbx-workflow] {message}", file=sys.stderr, flush=True)


def _text(value: object) -> str:
    if isinstance(value, str):
        return value.strip()
    return ""


def event_diagnostic(event: object) -> str | None:
    if not isinstance(event, dict):
        return None
    item = event.get("item")
    item_type = item.get("type") if isinstance(item, dict) else None
    failed_item = isinstance(item, dict) and event.get("type") == "item.completed" and item.get("status") in ("failed", "error")
    if event.get("type") not in ("error", "turn.failed") and item_type != "error" and not failed_item:
        return None
    for source in (event, item if isinstance(item, dict) else {}):
        for field in ("message", "error", "reason", "stderr", "output"):
            value = source.get(field)
            if isinstance(value, str) and value.strip():
                return value.strip()
            if isinstance(value, dict):
                message = _text(value.get("message"))
                if message:
                    return message
                return json.dumps(value, ensure_ascii=False)
    return "Codex reported an error event"


def _append_bounded(current: str, value: str, limit: int = MAX_DIAGNOSTIC_CHARS) -> str:
    return (current + value)[-limit:]


def _process_group_exists(pgid: int) -> bool:
    try:
        os.killpg(pgid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _wait_for_process_group_exit(process: subprocess.Popen, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    pgid = process.pid
    while _process_group_exists(pgid):
        process.poll()
        if time.monotonic() >= deadline:
            return False
        time.sleep(0.05)
    process.wait()
    return True


def _stop_process_group(process: subprocess.Popen) -> None:
    """Escalate signals only while the isolated Codex process group remains alive."""
    pgid = process.pid
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGKILL):
        if not _process_group_exists(pgid):
            process.wait()
            return
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            process.wait()
            return
        if _wait_for_process_group_exit(process, INTERRUPT_GRACE_SECONDS):
            return
    raise CodexStopError(f"Codex process group {pgid} remained after SIGKILL")


def describe_event(event: object) -> str | None:
    """Translate known Codex CLI JSONL events; unknown event types are ignored."""
    if not isinstance(event, dict):
        return None
    event_type = event.get("type")
    item = event.get("item")
    if not isinstance(item, dict):
        item = event
    item_type = item.get("type")
    status = "開始" if event_type == "item.started" else "完了"
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


def run_codex_jsonl(args: Sequence[str], *, cwd: Path, input_text: str | None = None) -> CommandResult:
    """Run Codex and render each recognized JSONL event as it arrives."""
    try:
        process = subprocess.Popen(
            list(args), cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, start_new_session=True,
        )
    except OSError as error:
        return CommandResult(127, "", str(error))
    stderr_detail = ""

    def drain_stderr() -> None:
        nonlocal stderr_detail
        if process.stderr is None:
            return
        read_chunk = getattr(process.stderr, "read1", process.stderr.read)
        decoder = codecs.getincrementaldecoder("utf-8")("replace")
        while chunk := read_chunk(256):
            text = decoder.decode(chunk) if isinstance(chunk, bytes) else chunk
            if text:
                sys.stderr.write(text)
                sys.stderr.flush()
                stderr_detail = _append_bounded(stderr_detail, text)
        remainder = decoder.decode(b"", final=True)
        if remainder:
            sys.stderr.write(remainder)
            sys.stderr.flush()
            stderr_detail = _append_bounded(stderr_detail, remainder)

    def write_stdin() -> None:
        if process.stdin is None:
            return
        try:
            if input_text is not None:
                process.stdin.write(input_text.encode("utf-8"))
            process.stdin.close()
        except BrokenPipeError:
            pass

    stderr_reader = threading.Thread(target=drain_stderr, daemon=True)
    stdin_writer = threading.Thread(target=write_stdin, daemon=True)
    stdout_diagnostic = ""
    stdin_started = False
    stderr_started = False
    try:
        stderr_reader.start()
        stderr_started = True
        stdin_writer.start()
        stdin_started = True
        for line in process.stdout or ():
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                progress("未解釈のCodex出力を受信")
                continue
            message = describe_event(event)
            if message:
                progress(message)
            diagnostic = event_diagnostic(event)
            if diagnostic:
                stdout_diagnostic = _append_bounded(stdout_diagnostic, diagnostic)
        returncode = process.wait()
        stdin_writer.join()
        stderr_reader.join()
    except KeyboardInterrupt:
        progress("中断を受信。Codexプロセスグループを停止します")
        previous_sigint_handler = signal.signal(signal.SIGINT, signal.SIG_IGN)
        try:
            _stop_process_group(process)
        except CodexStopError:
            raise
        except Exception as error:
            raise CodexStopError(f"could not stop Codex process group: {error}") from error
        finally:
            signal.signal(signal.SIGINT, previous_sigint_handler)
            if stdin_started:
                stdin_writer.join(timeout=1)
            if stderr_started:
                stderr_reader.join(timeout=1)
        raise CodexInterrupted("Codex execution interrupted") from None
    return CommandResult(returncode, stdout_diagnostic, stderr_detail.strip())
