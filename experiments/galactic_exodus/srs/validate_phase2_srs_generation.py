#!/usr/bin/env python3
"""Validate Galactic Exodus Phase 2 SRS generation contracts."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

EXPECTED_MAP_SIZES = [[9, 9], [11, 11]]
EXPECTED_SECTOR_TYPES = {
    "NORMAL",
    "BASE",
    "RESOURCE",
    "NEBULA",
    "ASTEROID",
    "GRAVITY",
    "RIFT",
}
EXPECTED_TERRAIN_TYPES = {
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
EXPECTED_OBJECT_TYPES = {"STAR", "PLANET", "STATION", "RESOURCE_CACHE", "SALVAGE"}
FORBIDDEN_TERMS = {"WALL", "BASE_NODE", "STATION_STRUCTURE", "WARP_POINT", "WarpZone"}
EXPECTED_GENERATION_ORDER = [
    "FLOOR_INIT",
    "RIFT_BARRIER",
    "WARP_FLOOR_RESERVATION",
    "REQUIRED_TERRAIN",
    "OPTIONAL_TERRAIN",
    "STAR",
    "PLANET",
    "STATION",
    "VALUE_OBJECTS",
    "WARP_FLAG_DERIVATION",
    "VALIDATION",
]
EXPECTED_REQUIRED_TARGETS = [
    "ALL_WARP_FLAGGED_CELLS",
    "STATION",
    "UNCONSUMED_RESOURCE_CACHE",
    "UNCOLLECTED_SALVAGE",
]
EXPECTED_REPORT_FIELDS = [
    "seed",
    "derived_seeds",
    "retry_index",
    "requested_counts",
    "actual_counts",
    "validation_results",
]
EXPECTED_CLUSTERED_TERRAINS = {
    "NEBULA",
    "ASTEROID_FIELD",
    "DEBRIS",
    "GRAVITY_FIELD_VERTICAL",
    "GRAVITY_FIELD_HORIZONTAL",
    "RIFT_DISTORTION",
}
EXPECTED_WARP_FLAGS = ["N", "E", "S", "W"]
SIZE_KEYS = ("9x9", "11x11")

EXPECTED_PLANET_RANGES = {
    "9x9": {"min": 2, "max": 4},
    "11x11": {"min": 3, "max": 6},
}
EXPECTED_VALUE_OBJECT_RANGES = {
    "NORMAL": {
        "9x9": {"SALVAGE": {"min": 0, "max": 1}},
        "11x11": {"SALVAGE": {"min": 0, "max": 1}},
    },
    "BASE": {"9x9": {}, "11x11": {}},
    "RESOURCE": {
        "9x9": {
            "RESOURCE_CACHE": {"min": 1, "max": 2},
            "SALVAGE": {"min": 0, "max": 1},
        },
        "11x11": {
            "RESOURCE_CACHE": {"min": 1, "max": 3},
            "SALVAGE": {"min": 0, "max": 1},
        },
    },
    "NEBULA": {
        "9x9": {"SALVAGE": {"min": 0, "max": 1}},
        "11x11": {"SALVAGE": {"min": 0, "max": 1}},
    },
    "ASTEROID": {
        "9x9": {
            "RESOURCE_CACHE": {"min": 0, "max": 1},
            "SALVAGE": {"min": 0, "max": 2},
        },
        "11x11": {
            "RESOURCE_CACHE": {"min": 0, "max": 1},
            "SALVAGE": {"min": 0, "max": 2},
        },
    },
    "GRAVITY": {
        "9x9": {"SALVAGE": {"min": 0, "max": 1}},
        "11x11": {"SALVAGE": {"min": 0, "max": 1}},
    },
    "RIFT": {
        "9x9": {
            "RESOURCE_CACHE": {"min": 1, "max": 2},
            "SALVAGE": {"min": 1, "max": 2},
        },
        "11x11": {
            "RESOURCE_CACHE": {"min": 1, "max": 2},
            "SALVAGE": {"min": 1, "max": 2},
        },
    },
}
EXPECTED_SECTOR_OPTIONAL_OBJECTS = {
    "NORMAL": ["SALVAGE"],
    "BASE": [],
    "RESOURCE": ["SALVAGE"],
    "NEBULA": ["SALVAGE"],
    "ASTEROID": ["RESOURCE_CACHE", "SALVAGE"],
    "GRAVITY": ["SALVAGE"],
    "RIFT": [],
}
EXPECTED_SECTOR_REQUIRED_OBJECTS = {
    "NORMAL": ["STAR", "PLANET"],
    "BASE": ["STAR", "PLANET", "STATION"],
    "RESOURCE": ["STAR", "PLANET", "RESOURCE_CACHE"],
    "NEBULA": ["STAR", "PLANET"],
    "ASTEROID": ["STAR", "PLANET"],
    "GRAVITY": ["STAR", "PLANET"],
    "RIFT": ["STAR", "PLANET", "RESOURCE_CACHE", "SALVAGE"],
}
EXPECTED_TERRAIN_COUNT_RANGES = {
    "NORMAL": {
        "9x9": {"DEBRIS": {"min": 0, "max": 5}},
        "11x11": {"DEBRIS": {"min": 0, "max": 8}},
    },
    "BASE": {
        "9x9": {"DEBRIS": {"min": 0, "max": 4}},
        "11x11": {"DEBRIS": {"min": 0, "max": 6}},
    },
    "RESOURCE": {
        "9x9": {
            "DEBRIS": {"min": 6, "max": 12},
            "ASTEROID_FIELD": {"min": 0, "max": 5},
            "ASTEROID": {"min": 0, "max": 2},
        },
        "11x11": {
            "DEBRIS": {"min": 9, "max": 18},
            "ASTEROID_FIELD": {"min": 0, "max": 8},
            "ASTEROID": {"min": 0, "max": 3},
        },
    },
    "NEBULA": {
        "9x9": {"NEBULA": {"min": 12, "max": 22}, "DEBRIS": {"min": 0, "max": 4}},
        "11x11": {"NEBULA": {"min": 18, "max": 32}, "DEBRIS": {"min": 0, "max": 6}},
    },
    "ASTEROID": {
        "9x9": {
            "ASTEROID_FIELD": {"min": 10, "max": 18},
            "ASTEROID": {"min": 3, "max": 7},
            "DEBRIS": {"min": 0, "max": 4},
        },
        "11x11": {
            "ASTEROID_FIELD": {"min": 15, "max": 27},
            "ASTEROID": {"min": 5, "max": 10},
            "DEBRIS": {"min": 0, "max": 6},
        },
    },
    "GRAVITY": {
        "9x9": {"GRAVITY_FIELD_TOTAL": {"min": 10, "max": 20}},
        "11x11": {"GRAVITY_FIELD_TOTAL": {"min": 15, "max": 30}},
    },
    "RIFT": {
        "9x9": {
            "RIFT_DISTORTION": {
                "candidate_pool": "BARRIER_INNER_ADJACENT_FLOOR",
                "min_percent": 30,
                "max_percent": 60,
                "minimum_count": 1,
            },
            "DEBRIS": {"min": 0, "max": 4},
            "ASTEROID_FIELD": {"min": 0, "max": 4},
            "ASTEROID": {"min": 0, "max": 2},
            "GRAVITY_FIELD_TOTAL": {"min": 0, "max": 5},
        },
        "11x11": {
            "RIFT_DISTORTION": {
                "candidate_pool": "BARRIER_INNER_ADJACENT_FLOOR",
                "min_percent": 30,
                "max_percent": 60,
                "minimum_count": 1,
            },
            "DEBRIS": {"min": 0, "max": 6},
            "ASTEROID_FIELD": {"min": 0, "max": 6},
            "ASTEROID": {"min": 0, "max": 3},
            "GRAVITY_FIELD_TOTAL": {"min": 0, "max": 8},
        },
    },
}
EXPECTED_SPECIAL_TERRAIN_LIMITS = {
    "NORMAL": {
        "9x9": {"mode": "FIXED_MAX", "counted_terrains": ["DEBRIS"], "max": 5},
        "11x11": {"mode": "FIXED_MAX", "counted_terrains": ["DEBRIS"], "max": 8},
    },
    "BASE": {
        "9x9": {"mode": "FIXED_MAX", "counted_terrains": ["DEBRIS"], "max": 4},
        "11x11": {"mode": "FIXED_MAX", "counted_terrains": ["DEBRIS"], "max": 6},
    },
    "RESOURCE": {
        "9x9": {
            "mode": "FIXED_MAX",
            "counted_terrains": ["DEBRIS", "ASTEROID_FIELD", "ASTEROID"],
            "max": 16,
        },
        "11x11": {
            "mode": "FIXED_MAX",
            "counted_terrains": ["DEBRIS", "ASTEROID_FIELD", "ASTEROID"],
            "max": 24,
        },
    },
    "NEBULA": {
        "9x9": {"mode": "FIXED_MAX", "counted_terrains": ["NEBULA", "DEBRIS"], "max": 24},
        "11x11": {"mode": "FIXED_MAX", "counted_terrains": ["NEBULA", "DEBRIS"], "max": 35},
    },
    "ASTEROID": {
        "9x9": {
            "mode": "FIXED_MAX",
            "counted_terrains": ["ASTEROID_FIELD", "ASTEROID", "DEBRIS"],
            "max": 25,
        },
        "11x11": {
            "mode": "FIXED_MAX",
            "counted_terrains": ["ASTEROID_FIELD", "ASTEROID", "DEBRIS"],
            "max": 38,
        },
    },
    "GRAVITY": {
        "9x9": {
            "mode": "FIXED_MAX",
            "counted_terrains": ["GRAVITY_FIELD_VERTICAL", "GRAVITY_FIELD_HORIZONTAL"],
            "max": 20,
        },
        "11x11": {
            "mode": "FIXED_MAX",
            "counted_terrains": ["GRAVITY_FIELD_VERTICAL", "GRAVITY_FIELD_HORIZONTAL"],
            "max": 30,
        },
    },
    "RIFT": {
        "9x9": {
            "mode": "BASE_TERRAIN_PLUS_ADDITIONAL_MAX",
            "base_terrain": "RIFT_BARRIER",
            "counted_additional_terrains": [
                "RIFT_DISTORTION",
                "DEBRIS",
                "ASTEROID_FIELD",
                "ASTEROID",
                "GRAVITY_FIELD_VERTICAL",
                "GRAVITY_FIELD_HORIZONTAL",
            ],
            "additional_max": 14,
        },
        "11x11": {
            "mode": "BASE_TERRAIN_PLUS_ADDITIONAL_MAX",
            "base_terrain": "RIFT_BARRIER",
            "counted_additional_terrains": [
                "RIFT_DISTORTION",
                "DEBRIS",
                "ASTEROID_FIELD",
                "ASTEROID",
                "GRAVITY_FIELD_VERTICAL",
                "GRAVITY_FIELD_HORIZONTAL",
            ],
            "additional_max": 22,
        },
    },
}
EXPECTED_CONSTRAINT_DEFINITIONS = {
    "CELESTIAL_NOT_ON_OUTER_EDGE": {
        "subjects": ["STAR", "PLANET"],
        "type": "outer_edge_forbidden",
    },
    "CELESTIAL_PAIR_MIN_CHEBYSHEV_2": {
        "subjects": ["STAR", "PLANET"],
        "type": "pairwise_min_chebyshev_distance",
        "min_distance": 2,
    },
    "CELESTIAL_FROM_WARP_FLAG_MIN_CHEBYSHEV_2": {
        "subjects": ["STAR", "PLANET"],
        "anchor": "warp_flagged_cell",
        "type": "anchor_min_chebyshev_distance",
        "min_distance": 2,
    },
    "STATION_FROM_CELESTIAL_MIN_CHEBYSHEV_2": {
        "subjects": ["STATION"],
        "anchors": ["STAR", "PLANET"],
        "type": "anchor_min_chebyshev_distance",
        "min_distance": 2,
    },
    "STATION_NEIGHBORHOOD_RESERVED_FLOOR": {
        "subjects": ["STATION"],
        "type": "neighbor_reserved_terrain",
        "metric": "chebyshev",
        "radius": 1,
        "reserved_terrain": "FLOOR",
    },
    "VALUE_OBJECT_FROM_WARP_FLAG_MIN_CHEBYSHEV_2": {
        "subjects": ["RESOURCE_CACHE", "SALVAGE"],
        "anchor": "warp_flagged_cell",
        "type": "anchor_min_chebyshev_distance",
        "min_distance": 2,
    },
    "VALUE_OBJECT_FROM_IMPASSABLE_CELESTIAL_MIN_CHEBYSHEV_2": {
        "subjects": ["RESOURCE_CACHE", "SALVAGE"],
        "anchors": ["STAR", "PLANET", "STATION"],
        "type": "anchor_min_chebyshev_distance",
        "min_distance": 2,
    },
    "VALUE_OBJECTS_INDIVIDUALLY_REACHABLE": {
        "subjects": ["RESOURCE_CACHE", "SALVAGE"],
        "type": "must_be_reachable_individually",
    },
    "RESOURCE_FIELD_IMPASSABLE_BALANCE": {
        "sector_type": "RESOURCE",
        "type": "terrain_count_relation",
        "left": ["ASTEROID_FIELD", "ASTEROID"],
        "operator": "<=",
        "right": ["DEBRIS"],
    },
    "ASTEROID_CLUSTER_IMPASSABLE_BALANCE": {
        "sector_type": "ASTEROID",
        "type": "terrain_count_relation",
        "left": ["ASTEROID"],
        "operator": "<=",
        "right": {"terrain": "ASTEROID_FIELD", "divisor": 2},
    },
    "GRAVITY_TOTAL_MIN_1": {
        "sector_type": "GRAVITY",
        "type": "terrain_sum_min",
        "terrains": ["GRAVITY_FIELD_VERTICAL", "GRAVITY_FIELD_HORIZONTAL"],
        "min": 1,
    },
    "RIFT_DISTORTION_BARRIER_ADJACENT_PERCENT": {
        "sector_type": "RIFT",
        "type": "candidate_percent_range",
        "terrain": "RIFT_DISTORTION",
        "candidate_pool": "BARRIER_INNER_ADJACENT_FLOOR",
        "min_percent": 30,
        "max_percent": 60,
        "minimum_count": 1,
        "selection_order": "SEEDED_SHUFFLE_AFTER_COORDINATE_ORDER",
    },
}
EXPECTED_PLACEMENT_CONSTRAINTS = {
    "NORMAL": [
        "CELESTIAL_NOT_ON_OUTER_EDGE",
        "CELESTIAL_PAIR_MIN_CHEBYSHEV_2",
        "CELESTIAL_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_IMPASSABLE_CELESTIAL_MIN_CHEBYSHEV_2",
        "VALUE_OBJECTS_INDIVIDUALLY_REACHABLE",
    ],
    "BASE": [
        "CELESTIAL_NOT_ON_OUTER_EDGE",
        "CELESTIAL_PAIR_MIN_CHEBYSHEV_2",
        "CELESTIAL_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "STATION_FROM_CELESTIAL_MIN_CHEBYSHEV_2",
        "STATION_NEIGHBORHOOD_RESERVED_FLOOR",
    ],
    "RESOURCE": [
        "CELESTIAL_NOT_ON_OUTER_EDGE",
        "CELESTIAL_PAIR_MIN_CHEBYSHEV_2",
        "CELESTIAL_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_IMPASSABLE_CELESTIAL_MIN_CHEBYSHEV_2",
        "VALUE_OBJECTS_INDIVIDUALLY_REACHABLE",
        "RESOURCE_FIELD_IMPASSABLE_BALANCE",
    ],
    "NEBULA": [
        "CELESTIAL_NOT_ON_OUTER_EDGE",
        "CELESTIAL_PAIR_MIN_CHEBYSHEV_2",
        "CELESTIAL_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_IMPASSABLE_CELESTIAL_MIN_CHEBYSHEV_2",
        "VALUE_OBJECTS_INDIVIDUALLY_REACHABLE",
    ],
    "ASTEROID": [
        "CELESTIAL_NOT_ON_OUTER_EDGE",
        "CELESTIAL_PAIR_MIN_CHEBYSHEV_2",
        "CELESTIAL_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_IMPASSABLE_CELESTIAL_MIN_CHEBYSHEV_2",
        "VALUE_OBJECTS_INDIVIDUALLY_REACHABLE",
        "ASTEROID_CLUSTER_IMPASSABLE_BALANCE",
    ],
    "GRAVITY": [
        "CELESTIAL_NOT_ON_OUTER_EDGE",
        "CELESTIAL_PAIR_MIN_CHEBYSHEV_2",
        "CELESTIAL_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_IMPASSABLE_CELESTIAL_MIN_CHEBYSHEV_2",
        "VALUE_OBJECTS_INDIVIDUALLY_REACHABLE",
        "GRAVITY_TOTAL_MIN_1",
    ],
    "RIFT": [
        "CELESTIAL_NOT_ON_OUTER_EDGE",
        "CELESTIAL_PAIR_MIN_CHEBYSHEV_2",
        "CELESTIAL_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_WARP_FLAG_MIN_CHEBYSHEV_2",
        "VALUE_OBJECT_FROM_IMPASSABLE_CELESTIAL_MIN_CHEBYSHEV_2",
        "VALUE_OBJECTS_INDIVIDUALLY_REACHABLE",
        "RIFT_DISTORTION_BARRIER_ADJACENT_PERCENT",
    ],
}
EXPECTED_SEED_ENCODING = {
    "serialization": "CANONICAL_JSON_OBJECT_UTF8",
    "required_fields": [
        "generation_schema_version",
        "galaxy_seed",
        "sector_x",
        "sector_y",
        "sector_descriptor",
    ],
    "unicode_normalization": "NFC",
    "object_key_order": "LEXICOGRAPHIC_ASCENDING",
    "set_like_field_encoding": "SORTED_ARRAY",
    "sector_descriptor_encoding": {
        "serialization": "CANONICAL_JSON_OBJECT",
        "object_key_order": "LEXICOGRAPHIC_ASCENDING",
        "blocked_edges_encoding": {"container": "ARRAY", "order": ["N", "E", "S", "W"]},
        "future_fields_rule": "APPLY_SAME_CANONICAL_JSON_RULES",
    },
    "digest_to_integer": {
        "bytes": "FULL_32_BYTES",
        "byte_order": "BIG_ENDIAN",
        "signed": False,
    },
}
EXPECTED_ATTEMPT_SEED = {
    "payload_fields": ["base_seed", "retry_index"],
    "serialization": "SAME_CANONICAL_JSON_OBJECT_ENCODING_AS_BASE_SEED",
}
EXPECTED_DERIVED_SEED_ENCODING = {
    "payload_fields": ["attempt_seed", "phase_label"],
    "serialization": "SAME_CANONICAL_JSON_OBJECT_ENCODING_AS_BASE_SEED",
}


class ValidationError(ValueError):
    """Raised when the Phase 2 SRS generation contract is inconsistent."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


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


def require_exact_list(actual: Any, expected: list[Any], message: str) -> None:
    require(actual == expected, message)


def require_exact_set(actual: Any, expected: set[str], message: str) -> None:
    require(isinstance(actual, list), message)
    require(set(actual) == expected, message)


def validate_no_forbidden_terms(data: Any, context: str = "root") -> None:
    if isinstance(data, dict):
        for key, value in data.items():
            require(key not in FORBIDDEN_TERMS, f"{context}: forbidden term {key} must not appear")
            validate_no_forbidden_terms(value, f"{context}.{key}")
    elif isinstance(data, list):
        for index, value in enumerate(data):
            validate_no_forbidden_terms(value, f"{context}[{index}]")
    elif isinstance(data, str):
        require(data not in FORBIDDEN_TERMS, f"{context}: forbidden term {data} must not appear")


def validate_range_object(range_obj: Any, message_prefix: str) -> None:
    require(isinstance(range_obj, dict), f"{message_prefix} must be an object")
    if {"candidate_pool", "min_percent", "max_percent", "minimum_count"} <= set(range_obj):
        require(
            range_obj.get("candidate_pool") == "BARRIER_INNER_ADJACENT_FLOOR",
            f"{message_prefix}.candidate_pool must be BARRIER_INNER_ADJACENT_FLOOR",
        )
        require(
            range_obj.get("min_percent") == 30
            and range_obj.get("max_percent") == 60
            and range_obj.get("minimum_count") == 1,
            f"{message_prefix} must be 30..60% with minimum_count 1",
        )
        return
    minimum = range_obj.get("min")
    maximum = range_obj.get("max")
    require(
        isinstance(minimum, int) and not isinstance(minimum, bool),
        f"{message_prefix}.min must be an integer",
    )
    require(
        isinstance(maximum, int) and not isinstance(maximum, bool),
        f"{message_prefix}.max must be an integer",
    )
    require(minimum <= maximum, f"{message_prefix} must satisfy min <= max")


def validate_sector_range_tables(profile: dict[str, Any], sector_type: str) -> None:
    range_tables = profile.get("terrain_count_ranges")
    require(
        isinstance(range_tables, dict) and set(range_tables) == set(SIZE_KEYS),
        f"{sector_type}.terrain_count_ranges must define 9x9 and 11x11",
    )
    for size_key in SIZE_KEYS:
        table = range_tables[size_key]
        require(isinstance(table, dict), f"{sector_type}.{size_key} terrain ranges must be an object")
        for terrain_name, range_obj in table.items():
            validate_range_object(range_obj, f"{sector_type}.{size_key}.{terrain_name}")
        require(
            table == EXPECTED_TERRAIN_COUNT_RANGES[sector_type][size_key],
            f"{sector_type}.{size_key}.terrain_count_ranges contract mismatch",
        )


def validate_required_optional_forbidden(profile: dict[str, Any], sector_type: str) -> None:
    required = profile.get("required_terrain")
    optional = profile.get("optional_terrain")
    forbidden = profile.get("forbidden_terrain")
    require(isinstance(required, list), f"{sector_type}.required_terrain must be a list")
    require(isinstance(optional, list), f"{sector_type}.optional_terrain must be a list")
    require(isinstance(forbidden, list), f"{sector_type}.forbidden_terrain must be a list")

    required_set = set(required)
    optional_set = set(optional)
    forbidden_set = set(forbidden)
    require(required_set | optional_set | forbidden_set == EXPECTED_TERRAIN_TYPES, f"{sector_type} terrain classes must cover all terrain types")
    require(required_set.isdisjoint(optional_set), f"{sector_type}: required and optional terrain must not overlap")
    require(required_set.isdisjoint(forbidden_set), f"{sector_type}: required and forbidden terrain must not overlap")
    require(optional_set.isdisjoint(forbidden_set), f"{sector_type}: optional and forbidden terrain must not overlap")

    range_tables = profile["terrain_count_ranges"]
    for size_key in SIZE_KEYS:
        table = range_tables[size_key]
        for terrain_name in required:
            if terrain_name == "FLOOR":
                continue
            range_obj = table.get(terrain_name)
            if terrain_name == "RIFT_BARRIER":
                require(
                    sector_type == "RIFT",
                    f"{sector_type}.{size_key}: RIFT_BARRIER may only be required in RIFT",
                )
                continue
            require(
                isinstance(range_obj, dict),
                f"{sector_type}.{size_key}: required terrain {terrain_name} range is missing",
            )
            validate_range_object(range_obj, f"{sector_type}.{size_key}.{terrain_name}")
            if "min" in range_obj:
                require(
                    range_obj.get("min", 0) >= 1,
                    f"{sector_type}.{size_key}: required terrain {terrain_name} min must be 1 or more",
                )
            else:
                require(
                    range_obj.get("minimum_count") == 1,
                    f"{sector_type}.{size_key}: required terrain {terrain_name} minimum_count must be 1",
                )
        for terrain_name in optional:
            range_obj = table.get(terrain_name)
            if range_obj is None:
                continue
            require(
                isinstance(range_obj, dict),
                f"{sector_type}.{size_key}: optional terrain {terrain_name} range is invalid",
            )
            validate_range_object(range_obj, f"{sector_type}.{size_key}.{terrain_name}")
            require(
                range_obj.get("min") == 0,
                f"{sector_type}.{size_key}: optional terrain {terrain_name} must allow 0",
            )


def validate_special_limits(profile: dict[str, Any], sector_type: str) -> None:
    limit = profile.get("special_terrain_limit")
    require(
        isinstance(limit, dict) and set(limit) == set(SIZE_KEYS),
        f"{sector_type}.special_terrain_limit must define 9x9 and 11x11",
    )
    for size_key in SIZE_KEYS:
        size_limit = limit[size_key]
        require(isinstance(size_limit, dict), f"{sector_type}.{size_key} special_terrain_limit must be an object")
        mode = size_limit.get("mode")
        require(
            mode in {"FIXED_MAX", "BASE_TERRAIN_PLUS_ADDITIONAL_MAX"},
            f"{sector_type}.{size_key} special_terrain_limit mode is invalid",
        )
        if mode == "FIXED_MAX":
            counted = size_limit.get("counted_terrains")
            require(isinstance(counted, list) and counted, f"{sector_type}.{size_key} counted_terrains must be non-empty")
            require(isinstance(size_limit.get("max"), int), f"{sector_type}.{size_key} special terrain max must be an integer")
        else:
            require(
                size_limit.get("base_terrain") == "RIFT_BARRIER",
                f"{sector_type}.{size_key} RIFT base_terrain must be RIFT_BARRIER",
            )
            counted = size_limit.get("counted_additional_terrains")
            require(
                isinstance(counted, list) and counted,
                f"{sector_type}.{size_key} counted_additional_terrains must be non-empty",
            )
            require(
                isinstance(size_limit.get("additional_max"), int),
                f"{sector_type}.{size_key} additional_max must be an integer",
            )
        require(
            size_limit == EXPECTED_SPECIAL_TERRAIN_LIMITS[sector_type][size_key],
            f"{sector_type}.{size_key}.special_terrain_limit contract mismatch",
        )

    impassable = profile.get("impassable_cell_limit")
    require(isinstance(impassable, dict), f"{sector_type}.impassable_cell_limit must be an object")
    require(impassable.get("counted_terrains") == ["ASTEROID"], f"{sector_type}.impassable counted_terrains must be ASTEROID only")
    require(
        impassable.get("counted_objects") == ["STAR", "PLANET", "STATION"],
        f"{sector_type}.impassable counted_objects must be STAR/PLANET/STATION",
    )
    require(
        impassable.get("excluded_terrains") == ["RIFT_BARRIER"],
        f"{sector_type}.impassable excluded_terrains must be RIFT_BARRIER",
    )
    require(impassable.get("9x9") == 10 and impassable.get("11x11") == 15, f"{sector_type}.impassable cell limit must be 10/15")


def validate_object_contracts(profile: dict[str, Any], sector_type: str) -> None:
    require(
        profile.get("required_objects") == EXPECTED_SECTOR_REQUIRED_OBJECTS[sector_type],
        f"{sector_type}.required_objects contract mismatch",
    )
    require(
        profile.get("optional_objects") == EXPECTED_SECTOR_OPTIONAL_OBJECTS[sector_type],
        f"{sector_type}.optional_objects contract mismatch",
    )
    count_ranges = profile.get("object_count_ranges")
    require(
        isinstance(count_ranges, dict) and set(count_ranges) == set(SIZE_KEYS),
        f"{sector_type}.object_count_ranges must define 9x9 and 11x11",
    )
    for size_key in SIZE_KEYS:
        per_size = count_ranges[size_key]
        require(isinstance(per_size, dict), f"{sector_type}.{size_key}.object_count_ranges must be an object")
        star = per_size.get("STAR")
        require(star == {"min": 1, "max": 1}, f"{sector_type}.{size_key}.STAR must be exactly 1")
        planet = per_size.get("PLANET")
        require(
            planet == EXPECTED_PLANET_RANGES[size_key],
            f"{sector_type}.{size_key}.PLANET range must be {EXPECTED_PLANET_RANGES[size_key]['min']}..{EXPECTED_PLANET_RANGES[size_key]['max']}",
        )
        for object_name, expected_range in EXPECTED_VALUE_OBJECT_RANGES[sector_type][size_key].items():
            require(
                per_size.get(object_name) == expected_range,
                f"{sector_type}.{size_key}.{object_name} range contract mismatch",
            )
        allowed = set(EXPECTED_SECTOR_REQUIRED_OBJECTS[sector_type]) | set(
            EXPECTED_SECTOR_OPTIONAL_OBJECTS[sector_type]
        )
        require(
            set(per_size) == allowed,
            f"{sector_type}.{size_key}.object_count_ranges must define only the sector object contract",
        )


def validate_constraint_definitions(definitions: dict[str, Any]) -> None:
    require(
        definitions == EXPECTED_CONSTRAINT_DEFINITIONS,
        "constraint_definitions contract mismatch",
    )


def validate_global_contract(contract: dict[str, Any]) -> None:
    terrain_role = contract.get("terrain_role_contract", {})
    require(
        terrain_role == {"required_terrain_min_count": 1, "optional_terrain_min_count": 0},
        "terrain_role_contract must require required>=1 and optional>=0",
    )

    celestial = contract.get("celestial_objects", {})
    require(celestial.get("STAR") == {"count": 1}, "STAR must be exactly 1")
    require(celestial.get("PLANET") == EXPECTED_PLANET_RANGES, "PLANET range contract mismatch")

    value_objects = contract.get("value_objects", {})
    require(value_objects.get("allow_resource_cache_and_salvage_on_same_map") is True, "RESOURCE_CACHE and SALVAGE must be allowed together")
    require(value_objects.get("allow_same_cell_overlap") is False, "RESOURCE_CACHE and SALVAGE must not overlap on the same cell")

    warp = contract.get("warp", {})
    require(
        warp.get("representation", {}).get("per_cell_directional_flags") == EXPECTED_WARP_FLAGS,
        "warp_flags must allow only N/E/S/W",
    )
    reserved = warp.get("reserved_floor_cluster", {})
    require(
        reserved.get("min_width") == 2 and reserved.get("min_height") == 2 and reserved.get("min_clusters_per_open_edge") == 1,
        "each open edge must reserve at least one 2x2 FLOOR cluster",
    )
    flag_generation = warp.get("flag_generation", {})
    require(
        flag_generation == {
            "non_rift": "ALLOW_AND_REQUIRE_ON_EACH_EDGE_WITH_ADJACENT_SECTOR",
            "rift_open_edge": "ALLOW_AND_REQUIRE",
            "rift_blocked_edge": "FORBID_AND_FILL_WITH_RIFT_BARRIER",
            "galaxy_outer_edge": "FORBID",
        },
        "warp blocked-edge / outer-edge contract mismatch",
    )
    corners = warp.get("corners", {})
    require(
        corners.get("evaluate_each_touching_direction_independently") is True
        and corners.get("allow_two_direction_flags") is True,
        "warp corners must permit independent multi-direction flags",
    )
    arrival = warp.get("arrival_selection", {})
    require(
        arrival.get("candidate_rule") == "OPPOSITE_EDGE_WITH_RETURN_FLAG",
        "arrival selection must use opposite-edge return-flag candidates",
    )
    require(
        arrival.get("north_south", {}).get("primary") == "MIN_ABS_DEST_X_MINUS_SOURCE_X"
        and arrival.get("north_south", {}).get("tie_break") == ["LOWER_X", "LOWER_Y"],
        "north/south arrival tie-break contract mismatch",
    )
    require(
        arrival.get("east_west", {}).get("primary") == "MIN_ABS_DEST_Y_MINUS_SOURCE_Y"
        and arrival.get("east_west", {}).get("tie_break") == ["LOWER_Y", "LOWER_X"],
        "east/west arrival tie-break contract mismatch",
    )
    require(
        arrival.get("no_candidate_behavior") == "INVALID_MAP_RETRY",
        "maps without arrival candidates must retry without fallback",
    )

    require_exact_list(
        contract.get("generation_order"),
        EXPECTED_GENERATION_ORDER,
        "generation_order must contain the exact 11-step Phase 2 sequence",
    )

    resolution = contract.get("terrain_count_resolution", {})
    require_exact_list(
        resolution.get("steps"),
        [
            "DRAW_REQUIRED_TERRAIN_IN_RANGE",
            "DRAW_OPTIONAL_TERRAIN_IN_RANGE_ALLOWING_ZERO",
            "VALIDATE_SPECIAL_TERRAIN_LIMIT",
            "REDUCE_OPTIONAL_TERRAIN_IN_FIXED_PRIORITY_WITHOUT_REROLL",
            "REVALIDATE_REQUIRED_TERRAIN_MINIMUMS",
        ],
        "terrain_count_resolution steps mismatch",
    )

    clustering = contract.get("clustering", {})
    require(
        set(clustering.get("clustered_terrains", [])) == EXPECTED_CLUSTERED_TERRAINS,
        "clustered_terrains contract mismatch",
    )
    require(
        clustering.get("isolated_single_cell_ratio_max") == 0.2,
        "isolated single-cell ratio max must be 0.2",
    )
    require(
        clustering.get("forbid_small_isolated_regions") is True,
        "small isolated regions must be forbidden",
    )
    for size_key, expected_count in {"9x9": {"min": 1, "max": 2}, "11x11": {"min": 1, "max": 3}}.items():
        nebula = clustering.get("NEBULA", {}).get(size_key, {})
        require(nebula.get("cluster_count") == expected_count, f"NEBULA {size_key} cluster_count contract mismatch")
        require(nebula.get("cluster_min_size") == 4, f"NEBULA {size_key} cluster_min_size must be 4")
    asteroid = clustering.get("ASTEROID", {})
    require(
        asteroid.get("place_asteroid_field_first") is True
        and asteroid.get("asteroid_internal_or_neighbor_ratio_min") == 0.5,
        "ASTEROID cluster invariants mismatch",
    )
    gravity = clustering.get("GRAVITY", {})
    require(
        gravity.get("per_orientation_with_positive_count_min_clusters") == 1
        and gravity.get("prefer_same_orientation_within_cluster") is True,
        "GRAVITY cluster invariants mismatch",
    )

    reachability = contract.get("reachability", {})
    require(
        reachability.get("required_same_component_targets") == EXPECTED_REQUIRED_TARGETS,
        "reachability must include all warp-flagged cells and required objects",
    )
    movement_reference = reachability.get("movement_rule_reference", {})
    require(
        movement_reference == {"issue": 1089, "rule_id": "PASSABLE_ADJACENCY"},
        "movement_rule_reference must point to issue #1089 PASSABLE_ADJACENCY",
    )

    seed = contract.get("seed_and_retry", {})
    require(seed.get("seed_hash_algorithm") == "SHA-256", "seed hash must be SHA-256")
    require(
        seed.get("seed_encoding") == EXPECTED_SEED_ENCODING,
        "seed_encoding contract mismatch",
    )
    require(
        seed.get("attempt_seed") == EXPECTED_ATTEMPT_SEED,
        "attempt_seed contract mismatch",
    )
    require(
        seed.get("derived_seed_labels") == ["terrain_seed", "celestial_seed", "object_seed"],
        "derived_seed_labels contract mismatch",
    )
    require(
        seed.get("derived_seed_encoding") == EXPECTED_DERIVED_SEED_ENCODING,
        "derived_seed_encoding contract mismatch",
    )
    retry = seed.get("retry", {})
    require(
        retry.get("seed_source") == "ATTEMPT_SEED_FROM_BASE_SEED_AND_RETRY_INDEX",
        "retry.seed_source contract mismatch",
    )
    require(
        retry.get("attempt_count_max") == 64
        and retry.get("retry_index_min") == 0
        and retry.get("retry_index_max") == 63
        and retry.get("initial_attempt_retry_index") == 0,
        "retry contract must fix the 64-attempt window",
    )
    require(retry.get("fallback_map") is False, "fallback maps must be disabled")

    report = contract.get("generation_report_schema", {})
    require(
        report.get("required_fields") == EXPECTED_REPORT_FIELDS,
        "generation report required_fields contract mismatch",
    )


def validate_sector_profiles(profiles: dict[str, Any]) -> None:
    require(set(profiles) == EXPECTED_SECTOR_TYPES, "sector_profiles must define all seven sector types")
    for sector_type, profile in profiles.items():
        require(isinstance(profile, dict), f"{sector_type} profile must be an object")
        validate_required_optional_forbidden(profile, sector_type)
        validate_sector_range_tables(profile, sector_type)
        validate_special_limits(profile, sector_type)
        validate_object_contracts(profile, sector_type)
        require(
            profile.get("placement_constraints") == EXPECTED_PLACEMENT_CONSTRAINTS[sector_type],
            f"{sector_type}.placement_constraints contract mismatch",
        )


def validate(path: Path) -> dict[str, Any]:
    data = load(path)
    validate_no_forbidden_terms(data)

    require(data.get("generation_schema_version") == 1, "generation_schema_version must be 1")
    require(data.get("map_sizes") == EXPECTED_MAP_SIZES, "map_sizes must be 9x9 and 11x11")
    require_exact_set(data.get("sector_types"), EXPECTED_SECTOR_TYPES, "sector_types must define all seven sectors")
    require_exact_set(data.get("terrain_types"), EXPECTED_TERRAIN_TYPES, "terrain_types must match the Phase 2 terrain contract")
    require_exact_set(data.get("object_types"), EXPECTED_OBJECT_TYPES, "object_types must match the Phase 2 object contract")

    validate_constraint_definitions(data.get("constraint_definitions", {}))
    validate_global_contract(data.get("global_generation_contract", {}))
    validate_sector_profiles(data.get("sector_profiles", {}))
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
    print("Phase 2 SRS generation: OK")
    print("sector types: 7")
    print("terrain types: 9")
    print("object types: 5")
    print("map sizes: 9x9, 11x11")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
