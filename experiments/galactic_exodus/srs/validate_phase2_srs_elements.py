#!/usr/bin/env python3
"""Validate Galactic Exodus Phase 2 SRS terrain and placement contracts."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

EXPECTED_TERRAINS = {
    "FLOOR",
    "DEBRIS",
    "NEBULA",
    "ASTEROID_FIELD",
    "ASTEROID",
    "GRAVITY_FIELD_VERTICAL",
    "GRAVITY_FIELD_HORIZONTAL",
    "RIFT_DISTORTION",
    "RIFT_BARRIER",
}
EXPECTED_SECTORS = {"NORMAL", "BASE", "RESOURCE", "NEBULA", "ASTEROID", "GRAVITY", "RIFT"}
EXPECTED_OBJECTS = {"STAR", "PLANET", "STATION", "RESOURCE_CACHE", "SALVAGE"}
IMPASSABLE = {"ASTEROID", "RIFT_BARRIER", "STAR", "PLANET", "STATION"}


class ValidationError(ValueError):
    """Raised when the SRS elements contract is inconsistent."""


def load(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ValidationError(f"missing file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ValidationError(f"invalid JSON in {path}: {exc}") from exc
    if not isinstance(data, dict):
        raise ValidationError("root must be an object")
    return data


def validate(path: Path) -> dict[str, Any]:
    data = load(path)
    if data.get("schema_version") != 1:
        raise ValidationError("schema_version must be 1")
    if data.get("map_sizes") != [[9, 9], [11, 11]]:
        raise ValidationError("map_sizes must be 9x9 and 11x11")
    if data.get("baseline_map_size") != [9, 9]:
        raise ValidationError("baseline_map_size must be 9x9")

    costs = data.get("geometric_costs", {})
    if costs != {"orthogonal": 10, "diagonal": 14}:
        raise ValidationError("geometric costs must be 10 and 14")

    observation = data.get("observation", {})
    expected_observation = {
        "default_size": 5,
        "nebula_size": 3,
        "update_after_each_successful_step": True,
        "known_map_is_cumulative": True,
        "use_destination_terrain": True,
    }
    if observation != expected_observation:
        raise ValidationError("observation contract mismatch")

    terrains = data.get("terrain_types")
    if not isinstance(terrains, dict) or set(terrains) != EXPECTED_TERRAINS:
        raise ValidationError("terrain_types must contain the exact expected set")
    if "WALL" in terrains or "STATION_STRUCTURE" in terrains:
        raise ValidationError("removed terrain types must not remain")

    for terrain_id, terrain in terrains.items():
        if not isinstance(terrain.get("passable"), bool):
            raise ValidationError(f"{terrain_id}.passable must be boolean")
        if terrain["passable"] and terrain.get("blocks_line_travel") is not False:
            raise ValidationError(f"{terrain_id}: passable terrain must not block line travel")
        if not terrain["passable"]:
            if terrain.get("blocks_line_travel") is not True:
                raise ValidationError(f"{terrain_id}: impassable terrain must block line travel")
            if terrain.get("collision_behavior") != "STOP_BEFORE":
                raise ValidationError(f"{terrain_id}: collision behavior must be STOP_BEFORE")
            if terrain.get("movement_cost_consumed_on_collision") is not False:
                raise ValidationError(f"{terrain_id}: collision must not consume movement cost")

    if terrains["FLOOR"].get("allowed_features") != ["WARP_FLAG"]:
        raise ValidationError("FLOOR must host WARP_FLAG")
    for terrain_id in EXPECTED_TERRAINS - {"FLOOR"}:
        if terrains[terrain_id].get("can_host_feature") is not False:
            raise ValidationError(f"{terrain_id} must not host features")

    if terrains["NEBULA"].get("move_multiplier") != 2 or terrains["NEBULA"].get("observation_size") != 3:
        raise ValidationError("NEBULA must have cost 2 and observation 3x3")
    if terrains["DEBRIS"].get("move_multiplier") != 2:
        raise ValidationError("DEBRIS cost must be 2")
    if terrains["ASTEROID_FIELD"].get("move_multiplier") != 3:
        raise ValidationError("ASTEROID_FIELD cost must be 3")
    if terrains["RIFT_DISTORTION"].get("move_multiplier") != 2:
        raise ValidationError("RIFT_DISTORTION cost must be 2")
    if terrains["RIFT_DISTORTION"].get("placement") != "RANDOM_INNER_ADJACENT_TO_RIFT_BARRIER":
        raise ValidationError("RIFT_DISTORTION placement mismatch")

    vertical = terrains["GRAVITY_FIELD_VERTICAL"]
    horizontal = terrains["GRAVITY_FIELD_HORIZONTAL"]
    if vertical.get("double_cost_when_axis_changes") != "X":
        raise ValidationError("vertical gravity must double X-changing movement")
    if horizontal.get("double_cost_when_axis_changes") != "Y":
        raise ValidationError("horizontal gravity must double Y-changing movement")

    objects = data.get("object_types")
    if not isinstance(objects, dict) or set(objects) != EXPECTED_OBJECTS:
        raise ValidationError("object_types must contain the exact expected set")
    if "BASE_NODE" in objects:
        raise ValidationError("BASE_NODE must not remain")
    for object_id in {"STAR", "PLANET", "STATION"}:
        obj = objects[object_id]
        if obj.get("passable") is not False or obj.get("blocks_line_travel") is not True:
            raise ValidationError(f"{object_id} must be impassable and block line travel")
        if obj.get("collision_behavior") != "STOP_BEFORE":
            raise ValidationError(f"{object_id} must use STOP_BEFORE")
        if obj.get("movement_cost_consumed_on_collision") is not False:
            raise ValidationError(f"{object_id} collision must not consume movement cost")
    if objects["STATION"].get("interaction_range") != "ADJACENT":
        raise ValidationError("STATION must use adjacent interaction")

    if set(data.get("impassable_elements", [])) != IMPASSABLE:
        raise ValidationError("impassable_elements mismatch")

    warp = data.get("warp_point", {})
    if warp.get("allowed_terrain") != ["FLOOR"]:
        raise ValidationError("WARP_POINT must be FLOOR-only")
    for key in ("edge_midpoint_only", "allows_object", "forbidden_on_rift_blocked_edge"):
        if key == "allows_object":
            if warp.get(key) is not False:
                raise ValidationError("WARP_POINT must forbid objects")
        elif warp.get(key) is not True:
            raise ValidationError(f"warp_point.{key} must be true")
    if warp.get("inner_adjacent_terrain") != "FLOOR":
        raise ValidationError("WARP_POINT inner adjacent terrain must be FLOOR")

    sector_matrix = data.get("sector_terrain_matrix")
    if not isinstance(sector_matrix, dict) or set(sector_matrix) != EXPECTED_SECTORS:
        raise ValidationError("sector_terrain_matrix must define every sector")
    gravity_invariants = sector_matrix["GRAVITY"].get("invariants", {})
    if gravity_invariants.get("gravity_field_total_min") != 1:
        raise ValidationError("GRAVITY must contain at least one gravity field cell")
    if gravity_invariants.get("orientation_mix") != "RANDOM":
        raise ValidationError("GRAVITY orientation mix must be random")

    object_matrix = data.get("terrain_object_matrix")
    if not isinstance(object_matrix, dict) or set(object_matrix) != EXPECTED_TERRAINS:
        raise ValidationError("terrain_object_matrix must define every terrain")
    for terrain_id, allowed in object_matrix.items():
        if not isinstance(allowed, list) or not set(allowed) <= EXPECTED_OBJECTS:
            raise ValidationError(f"{terrain_id}: invalid allowed objects")
    if set(object_matrix["FLOOR"]) != EXPECTED_OBJECTS:
        raise ValidationError("FLOOR must allow all current objects")
    if object_matrix["ASTEROID"] or object_matrix["RIFT_BARRIER"]:
        raise ValidationError("impassable terrain must not host objects")
    if "STATION" in set().union(*(set(v) for k, v in object_matrix.items() if k != "FLOOR")):
        raise ValidationError("STATION must be FLOOR-only")

    constraints = data.get("common_object_constraints", {})
    if constraints.get("max_objects_per_cell") != 1:
        raise ValidationError("max_objects_per_cell must be 1")
    if constraints.get("objects_forbidden_on_warp_point") is not True:
        raise ValidationError("objects must be forbidden on WARP_POINT")
    if constraints.get("objects_forbidden_on_impassable_terrain") is not True:
        raise ValidationError("objects must be forbidden on impassable terrain")
    if constraints.get("star_count") != 1:
        raise ValidationError("STAR count must be 1")
    if constraints.get("planet_count_min", 0) < 2:
        raise ValidationError("PLANET count minimum must be at least 2")
    if constraints.get("station_count_in_base") != 1 or constraints.get("station_terrain") != "FLOOR":
        raise ValidationError("BASE must contain one FLOOR STATION")

    deferred = data.get("deferred_to", {})
    if set(deferred) != {"1088", "1089"}:
        raise ValidationError("deferred decisions must target #1088 and #1089")

    return data


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        validate(args.path)
    except ValidationError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print("Phase 2 SRS elements: OK")
    print("terrain types: 9")
    print("object types: 5")
    print("map sizes: 9x9, 11x11")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
