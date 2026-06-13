#!/usr/bin/env python3
"""Run Galactic Exodus manual play sessions and record feedback immediately.

For each requested seed:
  1. Launch play.py interactively and write a JSON log.
  2. Ask the player for subjective scores and notes.
  3. Read objective values from the JSON log.
  4. Append or update one row in prototype_manual_sessions.csv.

Example:
    python experiments/galactic_exodus/run_manual_sessions.py \
      --player-id kkismd

The script is resumable. Seeds already recorded with all subjective fields
filled are skipped unless --redo-seed is specified.
"""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


EXPECTED_SCHEMA_VERSION = 3

FIELDNAMES = [
    "session_id",
    "player_id",
    "requested_seed",
    "effective_seed",
    "outcome",
    "turn_count",
    "remaining_fuel",
    "base_visit_count",
    "base_refuel_count",
    "resource_visit_count",
    "resource_refuel_count",
    "rift_attempts",
    "route_decision_score",
    "information_score",
    "fuel_tension_score",
    "supply_choice_score",
    "rift_fairness_score",
    "readability_score",
    "defeat_clarity_score",
    "observation_range_score",
    "resource_reveal_score",
    "rift_asymmetry_score",
    "base_return_value_score",
    "base_loop_risk_score",
    "notes",
    "log_path",
]

SCORE_QUESTIONS = [
    (
        "route_decision_score",
        "複数の移動候補を比較して選べましたか",
    ),
    (
        "information_score",
        "判断に必要な情報量は足りていましたか",
    ),
    (
        "fuel_tension_score",
        "燃料制約は緊張感として機能しましたか",
    ),
    (
        "supply_choice_score",
        "B/Rを使う・使わない判断に意味がありましたか",
    ),
    (
        "rift_fairness_score",
        "未知断層による失敗は納得できましたか",
    ),
    (
        "readability_score",
        "盤面と状態表示は読みやすかったですか",
    ),
    (
        "defeat_clarity_score",
        "勝敗理由は理解しやすかったですか",
    ),
    (
        "observation_range_score",
        "3x3観測範囲は広すぎず狭すぎませんでしたか",
    ),
    (
        "resource_reveal_score",
        "B/Rの発見時期は早すぎず遅すぎませんでしたか",
    ),
    (
        "rift_asymmetry_score",
        "地形は見えるが断層だけ未知という仕様は自然でしたか",
    ),
    (
        "base_return_value_score",
        "Bへ戻る選択には意味がありましたか",
    ),
    (
        "base_loop_risk_score",
        "B往復が単調な常套手段になる問題は少なかったですか",
    ),
]

NOTE_QUESTIONS = [
    "最も迷った局面",
    "Bへ戻ることを検討したか、その理由",
    "Rを使う価値を感じたか",
    "断層で印象に残った場面",
    "ルールまたは表示で分かりにくかった点",
]


class SessionError(RuntimeError):
    """Raised when a session cannot be recorded safely."""


def sanitize_text(value: str) -> str:
    """Return text that can always be encoded as UTF-8.

    Valid Unicode, including emoji, is preserved. UTF-16 surrogate pairs are
    combined when possible, and isolated surrogate code points are replaced.
    """
    try:
        value.encode("utf-8")
        return value
    except UnicodeEncodeError:
        repaired = value.encode(
            "utf-16", errors="surrogatepass"
        ).decode("utf-16", errors="replace")
        return repaired.encode(
            "utf-8", errors="replace"
        ).decode("utf-8")


def sanitize_row(
    row: dict[str, str | int],
) -> dict[str, str | int]:
    return {
        key: sanitize_text(value) if isinstance(value, str) else value
        for key, value in row.items()
    }


def feedback_path_for(log_path: Path) -> Path:
    return log_path.with_name(f"{log_path.stem}-feedback.json")


def save_feedback(
    path: Path,
    seed: int,
    answers: dict[str, str],
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "requested_seed": seed,
        "answers": {
            key: sanitize_text(value)
            for key, value in answers.items()
        },
    }
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    temporary.replace(path)


def load_feedback(path: Path, seed: int) -> dict[str, str]:
    if not path.exists():
        return {}
    try:
        payload = json.loads(
            path.read_text(encoding="utf-8", errors="replace")
        )
    except json.JSONDecodeError:
        return {}
    if payload.get("requested_seed") != seed:
        return {}
    answers = payload.get("answers")
    if not isinstance(answers, dict):
        return {}
    return {
        str(key): sanitize_text(str(value))
        for key, value in answers.items()
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Play seeds interactively and record immediate feedback."
    )
    parser.add_argument(
        "--play-script",
        type=Path,
        default=Path("experiments/galactic_exodus/play.py"),
        help="Path to the interactive Galactic Exodus CLI",
    )
    parser.add_argument(
        "--log-dir",
        type=Path,
        default=Path(".tmp/galactic_exodus/manual"),
        help="Directory for per-seed JSON logs",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path(
            "experiments/galactic_exodus/results/"
            "prototype_manual_sessions.csv"
        ),
        help="Output CSV path",
    )
    parser.add_argument(
        "--player-id",
        default="kkismd",
        help="Value written to player_id",
    )
    parser.add_argument("--seed-start", type=int, default=1)
    parser.add_argument("--seed-end", type=int, default=10)
    parser.add_argument(
        "--redo-seed",
        type=int,
        action="append",
        default=[],
        help="Replay and overwrite a specific seed. May be repeated.",
    )
    parser.add_argument(
        "--python",
        default=sys.executable,
        help="Python executable used to launch play.py",
    )
    return parser.parse_args()


def load_existing_rows(path: Path) -> dict[int, dict[str, str]]:
    if not path.exists():
        return {}

    with path.open(encoding="utf-8", errors="replace", newline="") as file:
        reader = csv.DictReader(file)
        if reader.fieldnames != FIELDNAMES:
            raise SessionError(
                f"{path}: CSV columns do not match the expected schema"
            )

        rows: dict[int, dict[str, str]] = {}
        for row in reader:
            try:
                seed = int(row["requested_seed"])
            except (TypeError, ValueError) as exc:
                raise SessionError(
                    f"{path}: invalid requested_seed in existing CSV"
                ) from exc
            if seed in rows:
                raise SessionError(
                    f"{path}: duplicate requested_seed={seed}"
                )
            rows[seed] = row
        return rows


def row_is_complete(row: dict[str, str]) -> bool:
    required = [
        key for key, _ in SCORE_QUESTIONS
    ] + ["notes"]
    return all(row.get(key, "").strip() for key in required)


def write_rows(
    path: Path,
    rows_by_seed: dict[int, dict[str, str | int]],
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")

    with temporary.open("w", encoding="utf-8", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=FIELDNAMES)
        writer.writeheader()
        for seed in sorted(rows_by_seed):
            writer.writerow(sanitize_row(rows_by_seed[seed]))

    temporary.replace(path)


def run_game(
    python_executable: str,
    play_script: Path,
    seed: int,
    log_path: Path,
) -> None:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    if log_path.exists():
        log_path.unlink()

    command = [
        python_executable,
        str(play_script),
        "--seed",
        str(seed),
        "--json-log",
        str(log_path),
    ]

    print()
    print("=" * 72)
    print(f"SEED {seed}: プレイを開始します")
    print("勝敗が確定するまで、通常どおり N/E/S/W を入力してください。")
    print("=" * 72)
    print()

    completed = subprocess.run(command, check=False)

    if completed.returncode != 0:
        raise SessionError(
            f"seed {seed}: play.py exited with code "
            f"{completed.returncode}"
        )
    if not log_path.exists():
        raise SessionError(
            f"seed {seed}: JSON log was not created: {log_path}"
        )


def require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise SessionError(f"{label} must be an object")
    return value


def require_int(mapping: dict[str, Any], key: str, label: str) -> int:
    value = mapping.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise SessionError(f"{label}.{key} must be an integer")
    return value


def require_str(mapping: dict[str, Any], key: str, label: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value:
        raise SessionError(f"{label}.{key} must be a non-empty string")
    return value


def load_objective_values(
    log_path: Path,
    expected_seed: int,
) -> dict[str, int | str]:
    try:
        payload = json.loads(log_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SessionError(
            f"{log_path}: invalid JSON at "
            f"line {exc.lineno}, column {exc.colno}"
        ) from exc

    root = require_mapping(payload, str(log_path))
    schema_version = require_int(root, "schema_version", str(log_path))
    if schema_version != EXPECTED_SCHEMA_VERSION:
        raise SessionError(
            f"{log_path}: expected schema_version="
            f"{EXPECTED_SCHEMA_VERSION}, got {schema_version}"
        )

    requested_seed = require_int(root, "requested_seed", str(log_path))
    if requested_seed != expected_seed:
        raise SessionError(
            f"{log_path}: requested_seed={requested_seed}, "
            f"expected {expected_seed}"
        )

    if root.get("generation_error") is not None:
        raise SessionError(
            f"{log_path}: generation_error is present"
        )

    final_summary = require_mapping(
        root.get("final_summary"),
        f"{log_path}.final_summary",
    )

    return {
        "requested_seed": requested_seed,
        "effective_seed": require_int(
            root, "effective_seed", str(log_path)
        ),
        "outcome": require_str(
            final_summary,
            "outcome",
            f"{log_path}.final_summary",
        ),
        "turn_count": require_int(
            final_summary,
            "turn_count",
            f"{log_path}.final_summary",
        ),
        "remaining_fuel": require_int(
            final_summary,
            "remaining_fuel",
            f"{log_path}.final_summary",
        ),
        "base_visit_count": require_int(
            final_summary,
            "base_visit_count",
            f"{log_path}.final_summary",
        ),
        "base_refuel_count": require_int(
            final_summary,
            "base_refuel_count",
            f"{log_path}.final_summary",
        ),
        "resource_visit_count": require_int(
            final_summary,
            "resource_visit_count",
            f"{log_path}.final_summary",
        ),
        "resource_refuel_count": require_int(
            final_summary,
            "resource_refuel_count",
            f"{log_path}.final_summary",
        ),
        "rift_attempts": require_int(
            final_summary,
            "rift_attempts",
            f"{log_path}.final_summary",
        ),
    }


def prompt_score(label: str) -> str:
    while True:
        raw = input(
            f"{label}\n"
            "  1=強い問題  2=問題あり  3=判断保留 "
            "  4=おおむね良い  5=良い\n"
            "> "
        ).strip()
        if raw in {"1", "2", "3", "4", "5"}:
            return raw
        print("1〜5の数字を入力してください。")


def prompt_nonempty(label: str) -> str:
    while True:
        value = sanitize_text(input(f"{label}\n> ").strip())
        if value:
            return value
        print("空欄にはできません。短い内容でも入力してください。")


def collect_subjective_values(
    seed: int,
    feedback_path: Path,
) -> dict[str, str]:
    print()
    print("-" * 72)
    print(f"SEED {seed}: プレイ直後の評価を入力してください")
    print("-" * 72)

    answers = load_feedback(feedback_path, seed)
    if answers:
        print(
            f"途中回答を読み込みました: {feedback_path}"
        )

    for key, question in SCORE_QUESTIONS:
        if key in answers and answers[key] in {"1", "2", "3", "4", "5"}:
            print(f"{question}: {answers[key]}（保存済み）")
            continue
        print()
        answers[key] = prompt_score(question)
        save_feedback(feedback_path, seed, answers)

    note_answers: list[str] = []
    print()
    print("最後に、以下を短く回答してください。")
    for index, question in enumerate(NOTE_QUESTIONS):
        key = f"note_{index}"
        if key in answers and answers[key].strip():
            answer = answers[key]
            print(f"{question}: {answer}（保存済み）")
        else:
            print()
            answer = prompt_nonempty(question)
            answers[key] = answer
            save_feedback(feedback_path, seed, answers)
        note_answers.append(f"{question}: {answer}")

    subjective = {
        key: answers[key]
        for key, _ in SCORE_QUESTIONS
    }
    subjective["notes"] = " / ".join(note_answers)
    save_feedback(feedback_path, seed, answers)
    return subjective


def build_row(
    seed: int,
    player_id: str,
    log_path: Path,
    objective: dict[str, int | str],
    subjective: dict[str, str],
) -> dict[str, str | int]:
    row: dict[str, str | int] = {
        "session_id": f"manual-{seed:03d}",
        "player_id": player_id,
        **objective,
        **subjective,
        "log_path": log_path.as_posix(),
    }

    missing = [name for name in FIELDNAMES if name not in row]
    if missing:
        raise SessionError(
            f"seed {seed}: missing CSV fields: {', '.join(missing)}"
        )
    return row


def confirm(prompt: str) -> bool:
    while True:
        answer = input(f"{prompt} [y/n] ").strip().lower()
        if answer in {"y", "yes"}:
            return True
        if answer in {"n", "no"}:
            return False
        print("y または n を入力してください。")


def main() -> int:
    args = parse_args()

    if args.seed_start > args.seed_end:
        print(
            "error: --seed-start must be <= --seed-end",
            file=sys.stderr,
        )
        return 2
    if not args.play_script.exists():
        print(
            f"error: play script not found: {args.play_script}",
            file=sys.stderr,
        )
        return 2

    try:
        rows: dict[int, dict[str, str | int]] = {
            seed: dict(row)
            for seed, row in load_existing_rows(args.output).items()
        }

        redo_seeds = set(args.redo_seed)
        seeds = range(args.seed_start, args.seed_end + 1)

        for seed in seeds:
            existing = rows.get(seed)
            if (
                existing is not None
                and row_is_complete(
                    {key: str(value) for key, value in existing.items()}
                )
                and seed not in redo_seeds
            ):
                print(f"seed {seed}: 完了済みのためスキップします")
                continue

            if existing is not None and seed not in redo_seeds:
                if not confirm(
                    f"seed {seed}には未完了の既存行があります。"
                    "プレイし直して上書きしますか"
                ):
                    print(f"seed {seed}: スキップしました")
                    continue

            log_path = args.log_dir / f"seed-{seed:03d}.json"
            feedback_path = feedback_path_for(log_path)

            reuse_existing_log = False
            if log_path.exists() and seed not in redo_seeds:
                reuse_existing_log = confirm(
                    f"seed {seed}の既存JSONログがあります。"
                    "プレイをやり直さず、このログから評価入力を再開しますか"
                )

            if not reuse_existing_log:
                run_game(
                    args.python,
                    args.play_script,
                    seed,
                    log_path,
                )
                if feedback_path.exists():
                    feedback_path.unlink()

            objective = load_objective_values(log_path, seed)
            subjective = collect_subjective_values(
                seed,
                feedback_path,
            )
            rows[seed] = build_row(
                seed,
                args.player_id,
                log_path,
                objective,
                subjective,
            )
            write_rows(args.output, rows)
            if feedback_path.exists():
                feedback_path.unlink()

            print()
            print(
                f"seed {seed}: CSVへ保存しました: {args.output}"
            )

            if seed != args.seed_end:
                if not confirm("次のseedへ進みますか"):
                    print(
                        "ここで終了します。次回は完了済みseedを"
                        "自動でスキップします。"
                    )
                    return 0

    except (SessionError, OSError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print(
            "\n中断しました。保存済みのseedは次回スキップされます。",
            file=sys.stderr,
        )
        return 130

    print()
    print("seed範囲のプレイと記録が完了しました。")
    print(f"CSV: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
