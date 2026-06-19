from __future__ import annotations

import json
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from types import MappingProxyType
from typing import Any, Mapping

from experiments.galactic_exodus.srs.model import CostMode, Direction, MovementRule, ObservationMode


class SrsContractError(ValueError):
    pass


def _freeze_mapping(mapping: Mapping[str, Any]) -> Mapping[str, Any]:
    return MappingProxyType(dict(mapping))


@dataclass(frozen=True, slots=True)
class InitialValuesContract:
    schema_version: int
    generation_schema_version: int
    baseline_observation_mode: ObservationMode
    baseline_cost_mode: CostMode
    baseline_movement_rule: MovementRule
    movement_points_per_turn: int
    max_srs_turns: int


@dataclass(frozen=True, slots=True)
class SrsElementsContract:
    schema_version: int
    raw: Mapping[str, Any]

    def __post_init__(self) -> None:
        object.__setattr__(self, "raw", _freeze_mapping(self.raw))


@dataclass(frozen=True, slots=True)
class SrsGenerationContract:
    generation_schema_version: int
    map_sizes: tuple[tuple[int, int], ...]
    raw: Mapping[str, Any]

    def __post_init__(self) -> None:
        object.__setattr__(self, "map_sizes", tuple(tuple(size) for size in self.map_sizes))
        object.__setattr__(self, "raw", _freeze_mapping(self.raw))


@dataclass(frozen=True, slots=True)
class SrsMovementContract:
    movement_schema_version: int
    directions: tuple[Direction, ...]
    baseline_observation_mode: ObservationMode
    baseline_cost_mode: CostMode
    baseline_movement_rule: MovementRule
    movement_cost_budget_raw: int
    orthogonal_raw_cost: int
    diagonal_raw_cost: int
    command_turn_rules: Mapping[str, Any]
    movement_rules: Mapping[str, Any]
    observation: Mapping[str, Any]
    interaction: Mapping[str, Any]
    warp_exit: Mapping[str, Any]

    def __post_init__(self) -> None:
        object.__setattr__(self, "directions", tuple(self.directions))
        object.__setattr__(self, "command_turn_rules", _freeze_mapping(self.command_turn_rules))
        object.__setattr__(self, "movement_rules", _freeze_mapping(self.movement_rules))
        object.__setattr__(self, "observation", _freeze_mapping(self.observation))
        object.__setattr__(self, "interaction", _freeze_mapping(self.interaction))
        object.__setattr__(self, "warp_exit", _freeze_mapping(self.warp_exit))


@dataclass(frozen=True, slots=True)
class SrsContracts:
    initial_values: InitialValuesContract
    elements: SrsElementsContract
    generation: SrsGenerationContract
    movement: SrsMovementContract


def _load_json(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise SrsContractError(f"missing contract file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SrsContractError(f"invalid JSON in {path}: {exc}") from exc
    except OSError as exc:
        raise SrsContractError(f"failed to read contract file {path}: {exc}") from exc
    if not isinstance(payload, dict):
        raise SrsContractError(f"{path}: root must be an object")
    return payload


def _enum(enum_type: type[Enum], value: str, *, field_name: str):
    try:
        return enum_type(value)
    except ValueError as exc:
        raise SrsContractError(f"{field_name} must be one of {[item.value for item in enum_type]}") from exc


def _reject_legacy_observation_mode(value: str) -> None:
    if value == "LOCAL_3X3":
        raise SrsContractError("LOCAL_3X3 is legacy; use LOCAL_MOVEMENT")


def _require_equal(actual: Any, expected: Any, *, field_name: str) -> None:
    if actual != expected:
        raise SrsContractError(f"{field_name} must be {expected!r}")


def load_initial_values(path: Path) -> InitialValuesContract:
    payload = _load_json(path)
    baseline = payload.get("baseline")
    if not isinstance(baseline, dict):
        raise SrsContractError("baseline must be an object")

    observation_mode = baseline.get("observation_mode")
    _reject_legacy_observation_mode(observation_mode)
    _require_equal(payload.get("schema_version"), 3, field_name="schema_version")
    _require_equal(payload.get("generation_schema_version"), 1, field_name="generation_schema_version")
    _require_equal(observation_mode, "LOCAL_MOVEMENT", field_name="baseline.observation_mode")
    _require_equal(baseline.get("cost_mode"), "TURN_ONLY", field_name="baseline.cost_mode")
    _require_equal(baseline.get("movement_rule"), "MOVEMENT_POINTS", field_name="baseline.movement_rule")
    _require_equal(baseline.get("movement_points_per_turn"), 4, field_name="baseline.movement_points_per_turn")
    _require_equal(baseline.get("max_srs_turns"), 40, field_name="baseline.max_srs_turns")

    return InitialValuesContract(
        schema_version=payload["schema_version"],
        generation_schema_version=payload["generation_schema_version"],
        baseline_observation_mode=_enum(
            ObservationMode,
            observation_mode,
            field_name="baseline.observation_mode",
        ),
        baseline_cost_mode=_enum(CostMode, baseline["cost_mode"], field_name="baseline.cost_mode"),
        baseline_movement_rule=_enum(
            MovementRule,
            baseline["movement_rule"],
            field_name="baseline.movement_rule",
        ),
        movement_points_per_turn=baseline["movement_points_per_turn"],
        max_srs_turns=baseline["max_srs_turns"],
    )


def load_srs_elements(path: Path) -> SrsElementsContract:
    payload = _load_json(path)
    _require_equal(payload.get("schema_version"), 1, field_name="schema_version")
    return SrsElementsContract(schema_version=payload["schema_version"], raw=payload)


def load_srs_generation(path: Path) -> SrsGenerationContract:
    payload = _load_json(path)
    _require_equal(payload.get("generation_schema_version"), 1, field_name="generation_schema_version")
    _require_equal(payload.get("map_sizes"), [[9, 9], [11, 11]], field_name="map_sizes")
    return SrsGenerationContract(
        generation_schema_version=payload["generation_schema_version"],
        map_sizes=tuple(tuple(size) for size in payload["map_sizes"]),
        raw=payload,
    )


def load_srs_movement(path: Path) -> SrsMovementContract:
    payload = _load_json(path)
    baseline = payload.get("baseline")
    cost_units = payload.get("cost_units")
    observation = payload.get("observation")
    if not isinstance(baseline, dict):
        raise SrsContractError("baseline must be an object")
    if not isinstance(cost_units, dict):
        raise SrsContractError("cost_units must be an object")
    if not isinstance(observation, dict):
        raise SrsContractError("observation must be an object")

    observation_mode = baseline.get("observation_mode")
    _reject_legacy_observation_mode(observation_mode)
    _require_equal(payload.get("movement_schema_version"), 1, field_name="movement_schema_version")
    _require_equal(payload.get("directions"), ["N", "E", "S", "W"], field_name="directions")
    _require_equal(observation_mode, "LOCAL_MOVEMENT", field_name="baseline.observation_mode")
    _require_equal(baseline.get("cost_mode"), "TURN_ONLY", field_name="baseline.cost_mode")
    _require_equal(baseline.get("movement_rule"), "MOVEMENT_POINTS", field_name="baseline.movement_rule")
    _require_equal(cost_units.get("movement_cost_budget_raw"), 40, field_name="cost_units.movement_cost_budget_raw")
    _require_equal(cost_units.get("orthogonal_raw_cost"), 10, field_name="cost_units.orthogonal_raw_cost")
    _require_equal(cost_units.get("diagonal_raw_cost"), 14, field_name="cost_units.diagonal_raw_cost")

    if "FULL" not in observation:
        raise SrsContractError("observation must define FULL")
    if "LOCAL_MOVEMENT" not in observation:
        raise SrsContractError("observation must define LOCAL_MOVEMENT")
    if "LOCAL_3X3" in observation:
        raise SrsContractError("LOCAL_3X3 is legacy; use LOCAL_MOVEMENT")

    return SrsMovementContract(
        movement_schema_version=payload["movement_schema_version"],
        directions=tuple(_enum(Direction, value, field_name="directions") for value in payload["directions"]),
        baseline_observation_mode=_enum(
            ObservationMode,
            observation_mode,
            field_name="baseline.observation_mode",
        ),
        baseline_cost_mode=_enum(CostMode, baseline["cost_mode"], field_name="baseline.cost_mode"),
        baseline_movement_rule=_enum(
            MovementRule,
            baseline["movement_rule"],
            field_name="baseline.movement_rule",
        ),
        movement_cost_budget_raw=cost_units["movement_cost_budget_raw"],
        orthogonal_raw_cost=cost_units["orthogonal_raw_cost"],
        diagonal_raw_cost=cost_units["diagonal_raw_cost"],
        command_turn_rules=payload.get("command_turn_rules", {}),
        movement_rules=payload.get("movement_rules", {}),
        observation=observation,
        interaction=payload.get("interaction", {}),
        warp_exit=payload.get("warp_exit", {}),
    )


def load_default_contracts(root: Path) -> SrsContracts:
    base = root / "experiments" / "galactic_exodus" / "srs"
    contracts = SrsContracts(
        initial_values=load_initial_values(base / "phase2_initial_values.json"),
        elements=load_srs_elements(base / "phase2_srs_elements.json"),
        generation=load_srs_generation(base / "phase2_srs_generation.json"),
        movement=load_srs_movement(base / "phase2_srs_movement.json"),
    )

    if contracts.initial_values.baseline_observation_mode != contracts.movement.baseline_observation_mode:
        raise SrsContractError("baseline observation mode mismatch between initial values and movement contract")
    if contracts.initial_values.baseline_cost_mode != contracts.movement.baseline_cost_mode:
        raise SrsContractError("baseline cost mode mismatch between initial values and movement contract")
    if contracts.initial_values.baseline_movement_rule != contracts.movement.baseline_movement_rule:
        raise SrsContractError("baseline movement rule mismatch between initial values and movement contract")
    if contracts.initial_values.movement_points_per_turn * 10 != contracts.movement.movement_cost_budget_raw:
        raise SrsContractError("movement_points_per_turn does not match movement_cost_budget_raw")

    return contracts
