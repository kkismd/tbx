#!/usr/bin/env python3
"""Validate Galactic Exodus Phase 2 SRS movement contracts."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


EXPECTED_DIRECTIONS = ["N", "E", "S", "W"]
EXPECTED_DIAGONALS = ["NE", "SE", "SW", "NW"]
EXPECTED_MOVEMENT_RULES = {"VECTOR_COMMAND", "MOVEMENT_POINTS", "DIRECTIONAL_THRUST"}
EXPECTED_EVENTS = {
    "MOVE_ACCEPTED",
    "MOVE_REJECTED",
    "STOPPED_BEFORE_IMPASSABLE",
    "INTERACT_ACCEPTED",
    "INTERACT_REJECTED",
    "OBJECT_CONSUMED",
    "STATION_ACTIVATED",
    "WARP_EXIT_ACCEPTED",
    "WARP_EXIT_REJECTED",
    "OBSERVATION_UPDATED",
}
EXPECTED_METRICS = {
    "goal_reach_rate",
    "mean_commands_to_exit",
    "collision_count",
    "blocked_command_count",
    "route_replanning_count",
    "requested_vs_actual_distance",
    "endpoint_rounding_error",
    "unused_movement_points",
    "unknown_space_collision_rate",
    "resource_use_rate",
    "resource_refuel_amount",
    "resource_detour_cost",
    "station_use_rate",
    "value_object_detour_rate",
    "object_acquisition_rate",
    "fuel_waste_on_full_resource",
    "repeat_object_attempt_rate",
    "turn_only_vs_shared_fuel_failure_delta",
}
FORBIDDEN_TERMS = {
    "WALL",
    "WARP_POINT",
    "WarpZone",
    "BASE_NODE",
    "STATION_STRUCTURE",
    "GRAVITY_FIELD",
    "seven_by_seven",
    "7x7",
}


class ValidationError(Exception):
    """Raised when the movement contract is invalid."""


def load_json(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise ValidationError(f"{path}: invalid JSON: {exc}") from exc
    if not isinstance(payload, dict):
        raise ValidationError(f"{path}: root must be an object")
    return payload


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def validate_forbidden_terms(payload: Any) -> None:
    text = json.dumps(payload, ensure_ascii=False)
    for term in sorted(FORBIDDEN_TERMS):
        if term in text:
            raise ValidationError(f"forbidden term {term} remains in movement contract")


def validate_baseline(payload: dict[str, Any]) -> None:
    baseline = payload.get("baseline")
    require(isinstance(baseline, dict), "baseline must be an object")
    expected = {
        "cost_mode": "TURN_ONLY",
        "movement_rule": "MOVEMENT_POINTS",
        "path_input_mode": "ROUTE_PREVIEW",
        "interaction_mode": "EXPLICIT_INTERACT",
        "collision_behavior": "STOP_BEFORE",
        "observation_mode": "LOCAL_3X3",
        "max_srs_turns": 40,
    }
    for key, value in expected.items():
        require(baseline.get(key) == value, f"baseline.{key} must be {value!r}")


def validate_cost_units(payload: dict[str, Any]) -> None:
    cost = payload.get("cost_units")
    require(isinstance(cost, dict), "cost_units must be an object")
    require(cost.get("raw_cost_denominator") == 10, "raw_cost_denominator must be 10")
    require(cost.get("movement_points_per_turn") == 4, "movement_points_per_turn must be 4")
    require(cost.get("movement_cost_budget_raw") == 40, "movement_cost_budget_raw must be 40")
    require(cost.get("orthogonal_raw_cost") == 10, "orthogonal_raw_cost must be 10")
    require(cost.get("diagonal_raw_cost") == 14, "diagonal_raw_cost must be 14")
    require(cost.get("turn_only_consumes_lrs_fuel") is False, "TURN_ONLY must not consume LRS fuel")
    require(
        cost.get("shared_fuel_consumption") == "SUM_RAW_MOVEMENT_COST_DIV_10_ROUNDED_UP",
        "SHARED_FUEL consumption rule mismatch",
    )


def validate_turn_rules(payload: dict[str, Any]) -> None:
    turn = payload.get("command_turn_rules")
    require(isinstance(turn, dict), "command_turn_rules must be an object")
    require(turn.get("invalid_or_rejected_command_consumes_turn") is False, "rejected command must not consume turn")
    require(turn.get("accepted_movement_command_consumes_srs_turn") is True, "accepted movement must consume turn")
    require(turn.get("movement_turn_cost") == 1, "movement_turn_cost must be 1")
    require(turn.get("accepted_interact_command_consumes_srs_turn") is True, "accepted INTERACT must consume turn")
    require(turn.get("interact_turn_cost") == 1, "interact_turn_cost must be 1")
    require(turn.get("accepted_warp_or_exit_command_consumes_srs_turn") is True, "accepted warp/exit must consume turn")
    require(turn.get("warp_or_exit_turn_cost") == 1, "warp_or_exit_turn_cost must be 1")


def validate_movement_rules(payload: dict[str, Any]) -> None:
    rules = payload.get("movement_rules")
    require(isinstance(rules, dict), "movement_rules must be an object")
    require(set(rules) == EXPECTED_MOVEMENT_RULES, "movement_rules must define exactly the three comparison rules")

    points = rules["MOVEMENT_POINTS"]
    require(points.get("allowed_step_directions") == EXPECTED_DIRECTIONS, "MOVEMENT_POINTS directions mismatch")
    require(points.get("diagonal_allowed") is False, "MOVEMENT_POINTS must be 4-directional")
    require(points.get("direction_changes_within_turn") == "FREE", "MOVEMENT_POINTS direction changes mismatch")
    require(points.get("unused_budget_carryover") is False, "movement points must not carry over")
    require(points.get("budget_raw") == 40, "MOVEMENT_POINTS budget_raw must be 40")
    require(points.get("route_budget_behavior") == "EXECUTE_LONGEST_PREFIX_WITHIN_BUDGET", "MOVEMENT_POINTS budget behavior mismatch")
    move_to = points.get("move_to_pathfinding")
    require(isinstance(move_to, dict), "MOVE_TO pathfinding contract missing")
    require(move_to.get("known_state_only") is True, "MOVE_TO must use known state only")
    require(move_to.get("neighbor_order") == EXPECTED_DIRECTIONS, "MOVE_TO neighbor order mismatch")

    vector = rules["VECTOR_COMMAND"]
    angle = vector.get("angle_degrees")
    require(isinstance(angle, dict), "VECTOR_COMMAND angle_degrees missing")
    require(angle.get("origin") == "N", "VECTOR_COMMAND angle origin must be N")
    require(angle.get("rotation") == "CLOCKWISE", "VECTOR_COMMAND angle rotation must be CLOCKWISE")
    require(angle.get("min") == 0 and angle.get("max") == 359, "VECTOR_COMMAND angle range mismatch")
    require(vector.get("distance") == {"min": 1, "max": 4, "integer_only": True}, "VECTOR_COMMAND distance contract mismatch")
    require(vector.get("path_resolution") == "SUPERCOVER_LINE", "VECTOR_COMMAND must use SUPERCOVER_LINE")
    require(vector.get("budget_raw") == 40, "VECTOR_COMMAND budget_raw must be 40")
    endpoint = vector.get("endpoint_formula")
    require(isinstance(endpoint, dict), "VECTOR_COMMAND endpoint formula missing")
    require(endpoint.get("reject_if_endpoint_equals_source") is True, "VECTOR_COMMAND same-cell endpoint must be rejected")

    thrust = rules["DIRECTIONAL_THRUST"]
    require(thrust.get("allowed_directions") == ["N", "NE", "E", "SE", "S", "SW", "W", "NW"], "THRUST directions mismatch")
    require(thrust.get("distance") == {"min": 1, "max": 4, "integer_only": True}, "THRUST distance contract mismatch")
    require(thrust.get("path_resolution") == "STRAIGHT_DIRECTIONAL_RAY", "THRUST path resolution mismatch")
    require(thrust.get("direction_changes_within_command") == "FORBIDDEN", "THRUST must forbid turns within command")
    require(thrust.get("budget_raw") == 40, "THRUST budget_raw must be 40")


def validate_collision(payload: dict[str, Any]) -> None:
    collision = payload.get("collision")
    require(isinstance(collision, dict), "collision must be an object")
    require(collision.get("behavior") == "STOP_BEFORE", "collision behavior must be STOP_BEFORE")
    require(collision.get("movement_cost_consumed_on_collision_cell") is False, "collision cell cost must not be consumed")
    first = collision.get("first_blocked_cell")
    require(isinstance(first, dict), "first_blocked_cell contract missing")
    require(first.get("position") == "UNCHANGED", "first blocked position must be unchanged")
    require(first.get("movement_raw_cost") == 0, "first blocked movement cost must be 0")
    require(first.get("movement_command_consumes_srs_turn") is True, "first blocked movement must consume turn")
    require(first.get("observation_update") is False, "first blocked movement must not update observation")
    partial = collision.get("after_partial_movement")
    require(isinstance(partial, dict), "after_partial_movement contract missing")
    require(partial.get("position") == "LAST_ENTERED_PASSABLE_CELL", "partial collision position mismatch")
    diagonal = collision.get("diagonal_corner_cutting")
    require(isinstance(diagonal, dict), "diagonal_corner_cutting contract missing")
    require(
        diagonal.get("rule") == "FORBID_CUTTING_BETWEEN_TWO_ORTHOGONALLY_BLOCKED_CELLS",
        "diagonal corner cutting rule mismatch",
    )


def validate_observation(payload: dict[str, Any]) -> None:
    obs = payload.get("observation")
    require(isinstance(obs, dict), "observation must be an object")
    require(set(obs) == {"FULL", "LOCAL_3X3"}, "observation must define FULL and LOCAL_3X3")
    local = obs["LOCAL_3X3"]
    require(local.get("default_size") == 5, "LOCAL_3X3 default_size must be 5")
    require(local.get("nebula_size") == 3, "LOCAL_3X3 nebula_size must be 3")
    require(local.get("use_destination_terrain") is True, "observation must use destination terrain")
    require(local.get("update_after_each_successful_step") is True, "observation must update after each successful step")
    require(local.get("known_map_is_cumulative") is True, "known map must be cumulative")
    require(local.get("failed_or_rejected_command_updates_observation") is False, "failed commands must not update observation")
    require(local.get("first_blocked_cell_collision_updates_observation") is False, "first blocked collision must not update observation")


def validate_interaction(payload: dict[str, Any]) -> None:
    interaction = payload.get("interaction")
    require(isinstance(interaction, dict), "interaction must be an object")
    require(interaction.get("mode") == "EXPLICIT_INTERACT", "interaction mode must be EXPLICIT_INTERACT")
    require(interaction.get("invalid_target_consumes_turn") is False, "invalid interact target must not consume turn")

    resource = interaction.get("RESOURCE_CACHE")
    require(isinstance(resource, dict), "RESOURCE_CACHE interaction missing")
    require(resource.get("range") == "SAME_CELL", "RESOURCE_CACHE range must be SAME_CELL")
    require(resource.get("effect") == "REFUEL_PARTIAL", "RESOURCE_CACHE effect mismatch")
    require(resource.get("sector_total_refuel_amount") == 5, "RESOURCE_CACHE sector total must be +5")
    require(resource.get("per_cache_split_by_count") == {"1": [5], "2": [3, 2], "3": [2, 2, 1]}, "RESOURCE_CACHE split mismatch")
    require(resource.get("clamp_to_max_fuel") is True, "RESOURCE_CACHE must clamp to max fuel")
    require(resource.get("consume_when_refuel_amount_is_zero") is False, "zero-refuel RESOURCE_CACHE must not be consumed")
    require(resource.get("persistent_field") == "consumed_object_ids", "RESOURCE_CACHE persistent field mismatch")

    station = interaction.get("STATION")
    require(isinstance(station, dict), "STATION interaction missing")
    require(station.get("range") == "ADJACENT", "STATION range must be ADJACENT")
    require(station.get("effect") == "REFUEL_TO_MAX", "STATION effect mismatch")
    require(station.get("reusable") is True, "STATION must be reusable")
    require(station.get("persistent_field") == "activated_object_ids", "STATION persistent field mismatch")

    salvage = interaction.get("SALVAGE")
    require(isinstance(salvage, dict), "SALVAGE interaction missing")
    require(salvage.get("effect") == "DEFERRED_PLACEHOLDER", "SALVAGE must be deferred placeholder")
    require(salvage.get("persistent_field") == "consumed_object_ids", "SALVAGE persistent field mismatch")


def validate_warp_exit(payload: dict[str, Any]) -> None:
    warp = payload.get("warp_exit")
    require(isinstance(warp, dict), "warp_exit must be an object")
    require(warp.get("warp_requires_cell_with_direction_flag") is True, "warp must require direction flag")
    require(warp.get("blocked_edge_or_outer_edge_is_rejected") is True, "blocked/outer warp must be rejected")
    require(warp.get("no_return_flag_candidate_is_generation_error") is True, "missing return flag must be generation error")
    require(warp.get("accepted_warp_or_exit_consumes_srs_turn") is True, "accepted warp/exit must consume turn")
    require(warp.get("invalid_warp_or_exit_consumes_turn") is False, "invalid warp/exit must not consume turn")


def validate_game_log(payload: dict[str, Any]) -> None:
    log = payload.get("game_log")
    require(isinstance(log, dict), "game_log must be an object")
    events = log.get("required_events")
    require(isinstance(events, list), "game_log.required_events must be a list")
    require(set(events) == EXPECTED_EVENTS, "game_log required events mismatch")
    fields = log.get("required_event_fields")
    require(isinstance(fields, list), "game_log.required_event_fields must be a list")
    for required in ("srs_turn", "command_type", "movement_rule", "cost_mode", "outcome"):
        require(required in fields, f"game_log.required_event_fields missing {required}")


def validate_metrics(payload: dict[str, Any]) -> None:
    metrics = payload.get("evaluation_metrics")
    require(isinstance(metrics, list), "evaluation_metrics must be a list")
    require(set(metrics) == EXPECTED_METRICS, "evaluation_metrics mismatch")


def validate(payload_or_path: dict[str, Any] | Path) -> dict[str, Any]:
    payload = load_json(payload_or_path) if isinstance(payload_or_path, Path) else payload_or_path
    validate_forbidden_terms(payload)

    require(payload.get("movement_schema_version") == 1, "movement_schema_version must be 1")
    require(payload.get("directions") == EXPECTED_DIRECTIONS, "directions must be N/E/S/W")
    require(payload.get("diagonal_directions") == EXPECTED_DIAGONALS, "diagonal_directions mismatch")
    refs = payload.get("contract_references")
    require(isinstance(refs, dict), "contract_references must be an object")
    require(refs.get("issue") == 1089, "contract_references.issue must be 1089")
    require(refs.get("elements") == "phase2_srs_elements.json", "elements reference mismatch")
    require(refs.get("generation") == "phase2_srs_generation.json", "generation reference mismatch")
    require(refs.get("initial_values") == "phase2_initial_values.json", "initial_values reference mismatch")

    coordinate = payload.get("coordinate_contract")
    require(isinstance(coordinate, dict), "coordinate_contract must be an object")
    require(coordinate.get("x_axis") == "E_INCREASES_X", "x_axis must increase east")
    require(coordinate.get("y_axis") == "N_INCREASES_Y", "y_axis must increase north")
    require(coordinate.get("path_cells") == "SOURCE_EXCLUDED_DESTINATION_INCLUDED", "path cell inclusion mismatch")

    validate_baseline(payload)
    validate_cost_units(payload)
    validate_turn_rules(payload)
    validate_movement_rules(payload)
    validate_collision(payload)
    validate_observation(payload)
    validate_interaction(payload)
    validate_warp_exit(payload)
    validate_game_log(payload)
    validate_metrics(payload)
    return payload


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("movement", type=Path)
    args = parser.parse_args(argv)

    try:
        payload = validate(args.movement)
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    print("Phase 2 SRS movement contract: OK")
    print(f"movement rules: {len(payload['movement_rules'])}")
    print(f"metrics: {len(payload['evaluation_metrics'])}")
    print("TURN_ONLY: OK")
    print("SHARED_FUEL: comparison")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
