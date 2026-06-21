from __future__ import annotations

import argparse
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

MAP_LEGEND = (
    ("?", "未発見"),
    (".", "床/通行可"),
    (",", "残骸地形"),
    ("~", "星雲"),
    (":", "小惑星帯"),
    ("#", "通行不能"),
    ("*", "恒星"),
    ("o", "惑星"),
    ("S", "補給ステーション"),
    ("R", "資源/未消費"),
    ("r", "資源/消費済み"),
    ("$", "salvage/未回収"),
    ("s", "salvage/回収済み"),
    ("@", "現在位置"),
    ("^", "北warp"),
    (">", "東warp"),
    ("v", "南warp"),
    ("<", "西warp"),
    ("+", "複数warp"),
)

MAP_LEGEND_COLUMNS = 3
MAP_LEGEND_COLUMN_WIDTH = 24

VERDICT_GUIDE = (
    "OK: このcaseの目的をmap/logから判断できる",
    "要調整: 表示・ログ・仕様・fixtureのどれかを直したい",
    "保留: このcaseだけでは判断できず、追加確認が必要",
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


def _format_position(value: object) -> str:
    if isinstance(value, (list, tuple)) and len(value) == 2:
        return f"({value[0]},{value[1]})"
    return "-"


def _map_legend_items() -> list[str]:
    return [f"{symbol} {description}" for symbol, description in MAP_LEGEND]


def _wrap_columns(items: Iterable[str], *, columns: int, column_width: int) -> str:
    rows: list[str] = []
    row: list[str] = []
    for item in items:
        row.append(item.ljust(column_width))
        if len(row) == columns:
            rows.append("".join(row).rstrip())
            row = []
    if row:
        rows.append("".join(row).rstrip())
    return "\n".join(rows)


def _map_legend_text() -> str:
    return _wrap_columns(
        _map_legend_items(),
        columns=MAP_LEGEND_COLUMNS,
        column_width=MAP_LEGEND_COLUMN_WIDTH,
    )


def _verdict_guide_text() -> str:
    return "\n".join(VERDICT_GUIDE)


def _case_goal_text(fixture_id: str) -> str:
    if fixture_id == "move_route_basic_9x9":
        return "見ること: 初期観測範囲、現在位置@、salvage $、南warp v、MOVE_ROUTEの結果が読み取れるか。"
    if fixture_id == "move_to_known_9x9":
        return "見ること: MOVE_TOが既知map上の自動移動として自然で、経路結果をlogで追えるか。"
    if fixture_id == "resource_cache_single_9x9":
        return "見ること: resource取得、消費状態、fuel回復がmap/logで分かるか。"
    if fixture_id == "station_refuel_9x9":
        return "見ること: stationの位置、隣接interaction、refuel to maxが分かるか。"
    if fixture_id == "salvage_placeholder_9x9":
        return "見ること: salvage placeholderの表示と消費記録が、現段階の評価対象として十分か。"
    if fixture_id == "warp_exit_s_9x9":
        return "見ること: vと南WARP_EXITの関係、成功log、出口としての分かりやすさ。"
    if fixture_id == "rift_blocked_n_9x9":
        return "見ること: RIFT由来の北出口不可が、拒否理由として分かるか。"
    if fixture_id == "shared_fuel_cost_9x9":
        return "見ること: SHARED_FUELのfuel消費がlogで追え、重すぎないか。"
    if fixture_id == "revisit_resource_consumed_9x9":
        return "見ること: 再訪時に発見済み・消費済み状態が復元されているか。"
    return "見ること: このcaseの目的をmap/logから判断できるか。"


def _event_summary_lines(result: SrsFixtureRunResult) -> list[str]:
    lines: list[str] = []
    for event in result.log.events:
        payload = dict(event.payload)
        prefix = f"turn {event.srs_turn}: {event.event_type}"
        if event.event_type == "MOVE_ACCEPTED":
            lines.append(
                f"{prefix} "
                f"{payload.get('command_type', '-')} "
                f"{_format_position(payload.get('start_position'))} -> {_format_position(payload.get('end_position'))} "
                f"fuel {payload.get('fuel_before', '-')}->{payload.get('fuel_after', '-')}"
            )
        elif event.event_type == "OBSERVATION_UPDATED":
            lines.append(
                f"{prefix} "
                f"center={_format_position(payload.get('center'))} "
                f"new={payload.get('newly_discovered_count', '-')} "
                f"total={payload.get('total_discovered_count', '-')}"
            )
        elif "outcome" in payload:
            lines.append(f"{prefix} outcome={payload.get('outcome')}")
        else:
            lines.append(prefix)
    return lines


def _event_summary_text(result: SrsFixtureRunResult) -> str:
    lines = _event_summary_lines(result)
    if not lines:
        return "- なし"
    return "\n".join(f"- {line}" for line in lines)


def _print_case(result: SrsFixtureRunResult) -> None:
    print("\n" + "=" * 80)
    print(result.fixture_id)
    print("-" * 80)
    print("evaluation guide:")
    print(_case_goal_text(result.fixture_id))
    print(_verdict_guide_text())
    print("\nevent summary:")
    print(_event_summary_text(result))
    print("\nmap legend:")
    print(_map_legend_text())
    print("\nknown map:")
    print(render_known_map_spaced(result.final_state))
    print("=" * 80)


def _print_verdict_context(result: SrsFixtureRunResult) -> None:
    print("\n判定前の確認:")
    print(_case_goal_text(result.fixture_id))
    print(_verdict_guide_text())


def _write_header(path: Path, fixture_paths: tuple[Path, ...]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "# SRS Manual Evaluation",
        "",
        f"- created_at: {datetime.now().isoformat(timespec='seconds')}",
        "- renderer: render_known_map_spaced",
        "- note: compact render / JSON API は fixture regression 用に維持する",
        "",
        "## 判定基準",
        "",
        "```text",
        _verdict_guide_text(),
        "```",
        "",
        "## Map legend",
        "",
        "```text",
        _map_legend_text(),
        "```",
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
    section = f"""
## {result.fixture_id}

### このcaseで見ること

{_case_goal_text(result.fixture_id)}

### 判定

- {status}

### event summary

{_event_summary_text(result)}

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

        _print_verdict_context(result)
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
