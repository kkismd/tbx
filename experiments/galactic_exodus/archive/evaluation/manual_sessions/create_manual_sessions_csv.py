#!/usr/bin/env python3
"""Create prototype_manual_sessions.csv from Galactic Exodus JSON game logs."""
from __future__ import annotations
import argparse, csv, json, sys
from pathlib import Path
from typing import Any

EXPECTED_SCHEMA_VERSION = 3
FIELDNAMES = [
    "session_id","player_id","requested_seed","effective_seed","outcome",
    "turn_count","remaining_fuel","base_visit_count","base_refuel_count",
    "resource_visit_count","resource_refuel_count","rift_attempts",
    "route_decision_score","information_score","fuel_tension_score",
    "supply_choice_score","rift_fairness_score","readability_score",
    "defeat_clarity_score","observation_range_score","resource_reveal_score",
    "rift_asymmetry_score","base_return_value_score","base_loop_risk_score",
    "notes","log_path",
]
SUBJECTIVE_FIELDS = [name for name in FIELDNAMES if name.endswith("_score")] + ["notes"]

class LogValidationError(ValueError):
    pass

def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Create manual-session CSV from GameLog v3 JSON files")
    p.add_argument("--input-dir", type=Path, default=Path(".tmp/galactic_exodus/manual"))
    p.add_argument("--output", type=Path, default=Path("experiments/galactic_exodus/results/prototype_manual_sessions.csv"))
    p.add_argument("--player-id", default="kkismd")
    p.add_argument("--seed-start", type=int, default=1)
    p.add_argument("--seed-end", type=int, default=10)
    p.add_argument("--overwrite", action="store_true")
    return p.parse_args()

def req_obj(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise LogValidationError(f"{label} must be an object")
    return value

def req_int(obj: dict[str, Any], key: str, label: str) -> int:
    value = obj.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise LogValidationError(f"{label}.{key} must be an integer")
    return value

def req_str(obj: dict[str, Any], key: str, label: str) -> str:
    value = obj.get(key)
    if not isinstance(value, str) or not value:
        raise LogValidationError(f"{label}.{key} must be a non-empty string")
    return value

def load_row(path: Path, expected_seed: int, player_id: str) -> dict[str, str | int]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise LogValidationError(f"missing log file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise LogValidationError(f"invalid JSON in {path}: line {exc.lineno}, column {exc.colno}") from exc

    root = req_obj(payload, str(path))
    schema = req_int(root, "schema_version", str(path))
    if schema != EXPECTED_SCHEMA_VERSION:
        raise LogValidationError(f"{path}: expected schema_version=3, got {schema}")
    requested = req_int(root, "requested_seed", str(path))
    if requested != expected_seed:
        raise LogValidationError(f"{path}: expected requested_seed={expected_seed}, got {requested}")
    if root.get("generation_error") is not None:
        raise LogValidationError(f"{path}: generation_error is present")
    summary = req_obj(root.get("final_summary"), f"{path}.final_summary")

    row: dict[str, str | int] = {
        "session_id": f"manual-{requested:03d}",
        "player_id": player_id,
        "requested_seed": requested,
        "effective_seed": req_int(root, "effective_seed", str(path)),
        "outcome": req_str(summary, "outcome", f"{path}.final_summary"),
        "turn_count": req_int(summary, "turn_count", f"{path}.final_summary"),
        "remaining_fuel": req_int(summary, "remaining_fuel", f"{path}.final_summary"),
        "base_visit_count": req_int(summary, "base_visit_count", f"{path}.final_summary"),
        "base_refuel_count": req_int(summary, "base_refuel_count", f"{path}.final_summary"),
        "resource_visit_count": req_int(summary, "resource_visit_count", f"{path}.final_summary"),
        "resource_refuel_count": req_int(summary, "resource_refuel_count", f"{path}.final_summary"),
        "rift_attempts": req_int(summary, "rift_attempts", f"{path}.final_summary"),
        "log_path": path.as_posix(),
    }
    for field in SUBJECTIVE_FIELDS:
        row[field] = ""
    return row

def main() -> int:
    args = parse_args()
    if args.seed_start > args.seed_end:
        print("error: --seed-start must be <= --seed-end", file=sys.stderr)
        return 2
    if args.output.exists() and not args.overwrite:
        print(f"error: output already exists: {args.output}\nUse --overwrite to replace it.", file=sys.stderr)
        return 1
    try:
        rows = [
            load_row(args.input_dir / f"seed-{seed:03d}.json", seed, args.player_id)
            for seed in range(args.seed_start, args.seed_end + 1)
        ]
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("w", encoding="utf-8", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=FIELDNAMES)
            writer.writeheader()
            writer.writerows(rows)
    except LogValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(f"created: {args.output}")
    print(f"rows: {len(rows)}")
    print("Next: fill all *_score columns with 1..5 and add notes.")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
