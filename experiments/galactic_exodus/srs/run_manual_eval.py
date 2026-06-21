from __future__ import annotations

import argparse
import json
from datetime import datetime
from pathlib import Path
from typing import Iterable, Mapping

from experiments.galactic_exodus.srs.render import render_known_map_spaced
from experiments.galactic_exodus.srs.run_fixture import FIXTURES_DIR, SrsFixtureRunResult, run_fixture


DEFAULT_FIXTURES = (
    "move_route_basic_9x9.json",
    "move_to_known_9x9.json",
    "resource_cache_single_9x9.json",
    "station_refuel_9x9.json",
    "salvage_placeholder_9x9.json",
    "warp_exit_s_9x9.json",
    "rift_blocked_n_9x9.json",
    "shared_fuel_cost_9x9.json",
    "revisit_resource_consumed_9x9.json",
)

QUESTIONS = (
    ("natural", "自然だった点"),
    ("confusing", "分かりにくかった点"),
    ("concerns", "違和感・要調整候補"),
    ("auto_eval", "#1082 自動評価に渡したい観点"),
)


def _default_output_path() -> Path:
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    return Path(f"srs_manual_eval_{timestamp}.md")


def _fixture_paths(names: Iterable[str]) -> tuple[Path, ...]:
    paths: list[Path] = []
    for name in names:
        path = Path(name)
        if not path.is_absolute() and path.parent == Path("."):
            path = FIXTURES_DIR / name
        paths.append(path)
    return tuple(paths)


def _prompt_line(prompt: str, *, default: str = "") -> str:
    suffix = f" [{default}]" if default else ""
    answer = input(f"{prompt}{suffix}: ").strip()
    return answer or default


def _prompt_multiline(prompt: str) -> str:
    print(f"{prompt}（空行で終了。なければそのままEnter）")
    lines: list[str] = []
    while True:
        line = input("  > ").rstrip()
        if line == "":
            break
        lines.append(line)
    return "\n".join(lines)


def _markdown_list(text: str) -> str:
    if not text.strip():
        return "- なし"
    return "\n".join(f"- {line}" for line in text.splitlines())


def _event_rows(result: SrsFixtureRunResult) -> list[Mapping[str, object]]:
    return [
        {
            "srs_turn": event.srs_turn,
            "event_type": event.event_type,
            "payload": dict(event.payload),
        }
        for event in result.log.events
    ]


def _print_case(result: SrsFixtureRunResult) -> None:
    print("\n" + "=" * 80)
    print(result.fixture_id)
    print("-" * 80)
    print("summary:")
    print(json.dumps(result.summary, ensure_ascii=False, sort_keys=True, indent=2))
    print("\nlog events:")
    print(json.dumps(_event_rows(result), ensure_ascii=False, sort_keys=True, indent=2))
    print("\nknown map:")
    print(render_known_map_spaced(result.final_state))
    print("=" * 80)


def _write_header(path: Path, fixture_paths: tuple[Path, ...]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "# SRS Manual Evaluation",
        "",
        f"- created_at: {datetime.now().isoformat(timespec='seconds')}",
        "- renderer: render_known_map_spaced",
        "- note: compact render / JSON API は fixture regression 用に維持する",
        "",
        "## Fixtures",
        "",
    ]
    lines.extend(f"- `{fixture_path}`" for fixture_path in fixture_paths)
    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def _append_case_result(
    path: Path,
    *,
    result: SrsFixtureRunResult,
    status: str,
    answers: Mapping[str, str],
) -> None:
    render = render_known_map_spaced(result.final_state)
    event_rows = _event_rows(result)
    section = f"""
## {result.fixture_id}

### 判定

- {status}

### summary

```json
{json.dumps(result.summary, ensure_ascii=False, sort_keys=True, indent=2)}
```

### log events

```json
{json.dumps(event_rows, ensure_ascii=False, sort_keys=True, indent=2)}
```

### known map

```text
{render}
```

### 自然だった点

{_markdown_list(answers['natural'])}

### 分かりにくかった点

{_markdown_list(answers['confusing'])}

### 違和感・要調整候補

{_markdown_list(answers['concerns'])}

### #1082 自動評価に渡したい観点

{_markdown_list(answers['auto_eval'])}

"""
    with path.open("a", encoding="utf-8") as file:
        file.write(section)


def run_manual_eval(fixture_paths: tuple[Path, ...], *, output_path: Path) -> None:
    _write_header(output_path, fixture_paths)
    print(f"記録先: {output_path}")
    print("各caseで出力を確認し、質問に回答してください。回答はcaseごとにMarkdownへ追記されます。")

    for index, fixture_path in enumerate(fixture_paths, start=1):
        print(f"\ncase {index}/{len(fixture_paths)}: {fixture_path}")
        result = run_fixture(fixture_path)
        _print_case(result)

        proceed = _prompt_line("このcaseを記録しますか？ yes/no", default="yes").lower()
        if proceed not in {"yes", "y"}:
            continue

        status = _prompt_line("判定を入力してください: OK / 要調整 / 保留", default="OK")
        answers = {key: _prompt_multiline(label) for key, label in QUESTIONS}
        _append_case_result(output_path, result=result, status=status, answers=answers)
        print(f"記録しました: {output_path}")

    print("\n完了しました。")
    print(f"記録先: {output_path}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Run SRS fixtures one by one and record manual evaluation notes.")
    parser.add_argument(
        "fixtures",
        nargs="*",
        help="Fixture JSON paths or fixture names. Defaults to the required 9x9 fixture set.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Markdown output path. Defaults to srs_manual_eval_<timestamp>.md.",
    )
    args = parser.parse_args()

    fixture_names = tuple(args.fixtures) if args.fixtures else DEFAULT_FIXTURES
    output_path = args.output or _default_output_path()
    run_manual_eval(_fixture_paths(fixture_names), output_path=output_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
