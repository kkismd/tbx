from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from enum import Enum
import random
from types import MappingProxyType
from typing import Mapping, Sequence

from experiments.galactic_exodus.srs.log import ENCOUNTER_ROLLED, make_turn_event
from experiments.galactic_exodus.srs.model import (
    Position,
    SectorType,
    SrsCombatState,
    SrsEnemyCombatState,
    SrsEnemyTier,
    SrsGameState,
    SrsObjectType,
    SrsTerrainType,
    create_enemy_combat_state,
)


class SrsEncounterError(ValueError):
    pass


class EncounterRollDisposition(str, Enum):
    REQUIRED = "REQUIRED"
    SKIPPED_COMMAND = "SKIPPED_COMMAND"
    SKIPPED_NO_TURN_ADVANCE = "SKIPPED_NO_TURN_ADVANCE"
    SKIPPED_ENEMY_PRESENCE = "SKIPPED_ENEMY_PRESENCE"
    SUPPRESSED_BASE_DOCKED = "SUPPRESSED_BASE_DOCKED"


@dataclass(frozen=True, slots=True)
class EncounterCompositionOption:
    weight_percent: int
    tiers: tuple[SrsEnemyTier, ...]

    def __post_init__(self) -> None:
        object.__setattr__(self, "tiers", tuple(self.tiers))
        if self.weight_percent <= 0:
            raise SrsEncounterError("composition weight_percent must be positive")
        if not self.tiers:
            raise SrsEncounterError("composition tiers must not be empty")


@dataclass(frozen=True, slots=True)
class FixedEncounterRoll:
    roll_result: str
    danger_score: int | None = None
    composition: tuple[SrsEnemyTier, ...] = ()

    def __post_init__(self) -> None:
        object.__setattr__(self, "roll_result", str(self.roll_result))
        object.__setattr__(self, "composition", tuple(self.composition))
        if self.roll_result not in {"success", "failure"}:
            raise SrsEncounterError("roll_result must be success or failure")
        if self.roll_result == "success":
            if self.danger_score is None:
                raise SrsEncounterError("successful encounter roll requires danger_score")
            if not self.composition:
                raise SrsEncounterError("successful encounter roll requires composition")
            validate_fixed_encounter_composition(
                danger_score=self.danger_score,
                composition=self.composition,
            )
        elif self.danger_score is not None or self.composition:
            raise SrsEncounterError("failed encounter roll must not specify danger_score or composition")


def _freeze_mapping(mapping: Mapping[object, object]) -> Mapping[object, object]:
    return MappingProxyType(dict(mapping))


_TIER_ORDER = {
    SrsEnemyTier.TIER1: 1,
    SrsEnemyTier.TIER2: 2,
    SrsEnemyTier.TIER3: 3,
    SrsEnemyTier.TIER4: 4,
}

EXPECTED_SRS_TURNS = 4
ENCOUNTERS_PER_LRS_STEP = 0.75
BASE_ENCOUNTER_CHANCE_PER_SRS_TURN = 0.18

ENEMY_GROUP_COSTS: Mapping[SrsEnemyTier, int] = _freeze_mapping(
    {
        SrsEnemyTier.TIER1: 1,
        SrsEnemyTier.TIER2: 2,
        SrsEnemyTier.TIER3: 3,
        SrsEnemyTier.TIER4: 5,
    }
)

ENEMY_SALVAGE_DROP_CHANCES: Mapping[SrsEnemyTier, float] = _freeze_mapping(
    {
        SrsEnemyTier.TIER1: 0.25,
        SrsEnemyTier.TIER2: 0.35,
        SrsEnemyTier.TIER3: 0.50,
        SrsEnemyTier.TIER4: 0.75,
    }
)

ENCOUNTER_GROUP_BUDGETS: Mapping[int, tuple[int, int]] = _freeze_mapping(
    {
        0: (1, 1),
        1: (1, 2),
        2: (2, 3),
        3: (3, 4),
        4: (4, 5),
    }
)

ENCOUNTER_COMPOSITION_TABLE: Mapping[int, tuple[EncounterCompositionOption, ...]] = _freeze_mapping(
    {
        0: (
            EncounterCompositionOption(100, (SrsEnemyTier.TIER1,)),
        ),
        1: (
            EncounterCompositionOption(70, (SrsEnemyTier.TIER1,)),
            EncounterCompositionOption(30, (SrsEnemyTier.TIER1, SrsEnemyTier.TIER1)),
        ),
        2: (
            EncounterCompositionOption(50, (SrsEnemyTier.TIER2,)),
            EncounterCompositionOption(35, (SrsEnemyTier.TIER1, SrsEnemyTier.TIER1)),
            EncounterCompositionOption(15, (SrsEnemyTier.TIER2, SrsEnemyTier.TIER1)),
        ),
        3: (
            EncounterCompositionOption(45, (SrsEnemyTier.TIER2, SrsEnemyTier.TIER1)),
            EncounterCompositionOption(30, (SrsEnemyTier.TIER3,)),
            EncounterCompositionOption(20, (SrsEnemyTier.TIER1, SrsEnemyTier.TIER1, SrsEnemyTier.TIER1)),
            EncounterCompositionOption(5, (SrsEnemyTier.TIER2, SrsEnemyTier.TIER2)),
        ),
        4: (
            EncounterCompositionOption(40, (SrsEnemyTier.TIER3, SrsEnemyTier.TIER1)),
            EncounterCompositionOption(25, (SrsEnemyTier.TIER2, SrsEnemyTier.TIER2)),
            EncounterCompositionOption(20, (SrsEnemyTier.TIER2, SrsEnemyTier.TIER1, SrsEnemyTier.TIER1)),
            EncounterCompositionOption(10, (SrsEnemyTier.TIER3, SrsEnemyTier.TIER2)),
            EncounterCompositionOption(5, (SrsEnemyTier.TIER4,)),
        ),
    }
)


def enemy_group_cost(tier: SrsEnemyTier) -> int:
    return ENEMY_GROUP_COSTS[tier]


def enemy_salvage_drop_chance(tier: SrsEnemyTier) -> float:
    return ENEMY_SALVAGE_DROP_CHANCES[tier]


def resolve_enemy_salvage_drop(
    *,
    tier: SrsEnemyTier,
    roll: float,
) -> tuple[bool, Mapping[str, object]]:
    if not 0.0 <= roll <= 1.0:
        raise SrsEncounterError("enemy salvage drop roll must be within 0.0..1.0")
    chance = enemy_salvage_drop_chance(tier)
    drop_salvage = roll < chance
    return (
        drop_salvage,
        {
            "enemy_tier": tier.value,
            "salvage_drop_chance": chance,
            "salvage_drop_roll": roll,
            "drop_salvage": drop_salvage,
        },
    )


def terrain_encounter_modifier(terrain: SrsTerrainType) -> float:
    if terrain is SrsTerrainType.NEBULA:
        return 0.7
    return 1.0


def actual_encounter_chance(state: SrsGameState) -> float:
    terrain = state.actual_map.cell_at(state.player_position).terrain
    return BASE_ENCOUNTER_CHANCE_PER_SRS_TURN * terrain_encounter_modifier(terrain)


def encounter_group_budget_range(danger_score: int) -> tuple[int, int]:
    return ENCOUNTER_GROUP_BUDGETS[_validated_danger_score(danger_score)]


def encounter_composition_options(danger_score: int) -> tuple[EncounterCompositionOption, ...]:
    return ENCOUNTER_COMPOSITION_TABLE[_validated_danger_score(danger_score)]


def validate_fixed_encounter_composition(
    *,
    danger_score: int,
    composition: Sequence[SrsEnemyTier],
) -> tuple[SrsEnemyTier, ...]:
    normalized = tuple(composition)
    if not normalized:
        raise SrsEncounterError("encounter composition must not be empty")
    budget_min, budget_max = encounter_group_budget_range(danger_score)
    budget = sum(enemy_group_cost(tier) for tier in normalized)
    if budget < budget_min or budget > budget_max:
        raise SrsEncounterError("encounter composition cost must fit the danger budget range")

    requested_counts = Counter(normalized)
    for option in encounter_composition_options(danger_score):
        if Counter(option.tiers) == requested_counts:
            return normalized
    raise SrsEncounterError("encounter composition must match a fixed option for the danger score")


def spawn_candidate_points(state: SrsGameState) -> tuple[Position, ...]:
    player_position = state.player_position
    candidates: list[Position] = []
    for position in _warp_point_positions(state):
        if abs(position.x - player_position.x) <= 1 and abs(position.y - player_position.y) <= 1:
            continue
        candidates.append(position)
    return tuple(sorted(candidates, key=_position_sort_key))


def apply_spawn_cap(planned_enemies: Sequence[SrsEnemyTier], spawn_cap: int) -> tuple[SrsEnemyTier, ...]:
    if spawn_cap < 0:
        raise SrsEncounterError("spawn_cap must be non-negative")
    if spawn_cap == 0:
        return ()

    strongest_first = sorted(planned_enemies, key=_tier_desc_sort_key)
    kept = strongest_first[:spawn_cap]
    return tuple(sorted(kept, key=_tier_asc_sort_key))


def spawn_enemies_for_encounter(
    state: SrsGameState,
    *,
    danger_score: int,
    composition: Sequence[SrsEnemyTier],
) -> tuple[SrsEnemyCombatState, ...]:
    planned_enemies = validate_fixed_encounter_composition(
        danger_score=danger_score,
        composition=composition,
    )
    candidates = spawn_candidate_points(state)
    selected_tiers = apply_spawn_cap(planned_enemies, len(candidates))
    rng = _enemy_salvage_drop_rng(
        state,
        danger_score=danger_score,
        composition=selected_tiers,
    )
    debug_payloads = [
        resolve_enemy_salvage_drop(tier=tier, roll=rng.random())
        for tier in selected_tiers
    ]
    return tuple(
        create_enemy_combat_state(
            enemy_id=f"enemy-{index}",
            tier=tier,
            position=position,
            drop_salvage=drop_salvage,
            salvage_drop_chance=payload["salvage_drop_chance"],
            salvage_drop_roll=payload["salvage_drop_roll"],
        )
        for index, ((tier, position), (drop_salvage, payload)) in enumerate(
            zip(
                zip(selected_tiers, candidates[: len(selected_tiers)], strict=True),
                debug_payloads,
                strict=True,
            ),
            start=1,
        )
    )


def combat_state_from_fixed_encounter(
    state: SrsGameState,
    *,
    danger_score: int,
    composition: Sequence[SrsEnemyTier],
    player_attack_target_id: str | None = None,
    base_combat_state: SrsCombatState | None = None,
) -> SrsCombatState:
    enemies = spawn_enemies_for_encounter(
        state,
        danger_score=danger_score,
        composition=composition,
    )
    if base_combat_state is None:
        return SrsCombatState(
            enemies={enemy.enemy_id: enemy for enemy in enemies},
            player_attack_target_id=player_attack_target_id,
        )
    return SrsCombatState(
        player=base_combat_state.player,
        enemies={enemy.enemy_id: enemy for enemy in enemies},
        weapon_profiles=base_combat_state.weapon_profiles,
        phase=base_combat_state.phase,
        combat_turn=base_combat_state.combat_turn,
        player_attack_target_id=player_attack_target_id,
    )


def encounter_roll_disposition(
    previous_state: SrsGameState,
    *,
    command_type: str,
    next_state: SrsGameState,
) -> EncounterRollDisposition:
    if command_type not in {"MOVE_ROUTE", "MOVE_TO", "WAIT"}:
        return EncounterRollDisposition.SKIPPED_COMMAND
    if next_state.srs_turn <= previous_state.srs_turn:
        return EncounterRollDisposition.SKIPPED_NO_TURN_ADVANCE
    if _enemy_presence(previous_state) or _enemy_presence(next_state):
        return EncounterRollDisposition.SKIPPED_ENEMY_PRESENCE
    if is_base_docked(next_state):
        return EncounterRollDisposition.SUPPRESSED_BASE_DOCKED
    return EncounterRollDisposition.REQUIRED


def resolve_fixed_encounter_roll(
    state: SrsGameState,
    *,
    command_type: str,
    roll: FixedEncounterRoll,
) -> tuple[SrsGameState, object]:
    chance = actual_encounter_chance(state)
    terrain = state.actual_map.cell_at(state.player_position).terrain
    if roll.roll_result == "failure":
        return (
            state,
            make_turn_event(
                srs_turn=state.srs_turn,
                event_type=ENCOUNTER_ROLLED,
                payload={
                    "command_type": command_type,
                    "terrain": terrain.value,
                    "terrain_modifier": terrain_encounter_modifier(terrain),
                    "base_encounter_chance_per_srs_turn": BASE_ENCOUNTER_CHANCE_PER_SRS_TURN,
                    "actual_encounter_chance": chance,
                    "roll_result": "failure",
                    "enemy_spawned": False,
                    "outcome": "NO_ENCOUNTER",
                },
            ),
        )

    updated_combat_state = combat_state_from_fixed_encounter(
        state,
        danger_score=roll.danger_score,
        composition=roll.composition,
        base_combat_state=state.combat_state,
    )
    updated_state = SrsGameState(
        descriptor=state.descriptor,
        actual_map=state.actual_map,
        known_state=state.known_state,
        persistent_state=state.persistent_state,
        player_position=state.player_position,
        objects=state.objects,
        combat_state=updated_combat_state,
        srs_turn=state.srs_turn,
        fuel=state.fuel,
        max_fuel=state.max_fuel,
    )
    return (
        updated_state,
        make_turn_event(
            srs_turn=state.srs_turn,
            event_type=ENCOUNTER_ROLLED,
            payload={
                "command_type": command_type,
                "terrain": terrain.value,
                "terrain_modifier": terrain_encounter_modifier(terrain),
                "base_encounter_chance_per_srs_turn": BASE_ENCOUNTER_CHANCE_PER_SRS_TURN,
                "actual_encounter_chance": chance,
                "roll_result": "success",
                "danger_score": roll.danger_score,
                "composition": [tier.value for tier in roll.composition],
                "enemy_spawned": True,
                "spawned_enemy_ids": sorted(updated_combat_state.enemies),
                "spawned_enemies": [
                    {
                        "enemy_id": enemy.enemy_id,
                        "enemy_tier": enemy.tier.value,
                        "position": [enemy.position.x, enemy.position.y],
                        "salvage_drop_chance": enemy.salvage_drop_chance,
                        "salvage_drop_roll": enemy.salvage_drop_roll,
                        "drop_salvage": enemy.drop_salvage,
                    }
                    for enemy in updated_combat_state.enemies.values()
                ],
                "outcome": "ENCOUNTER_STARTED",
            },
        ),
    )


def _validated_danger_score(danger_score: int) -> int:
    if danger_score not in ENCOUNTER_GROUP_BUDGETS:
        raise SrsEncounterError("danger_score must be in range 0..4")
    return danger_score


def is_base_docked(state: SrsGameState) -> bool:
    if state.descriptor.sector_type is not SectorType.BASE:
        return False
    for object_state in state.objects.values():
        if object_state.object_type is not SrsObjectType.STATION:
            continue
        if _is_adjacent(state.player_position, object_state.position):
            return True
    return False


def _warp_point_positions(state: SrsGameState) -> tuple[Position, ...]:
    positions: list[Position] = []
    for y, row in enumerate(state.actual_map.cells):
        for x, cell in enumerate(row):
            if not cell.warp_flags:
                continue
            if cell.terrain in {SrsTerrainType.ASTEROID, SrsTerrainType.RIFT_BARRIER}:
                continue
            if cell.object_id is not None:
                object_type = state.objects[cell.object_id].object_type
                if object_type in {SrsObjectType.STAR, SrsObjectType.PLANET, SrsObjectType.STATION}:
                    continue
            positions.append(Position(x, y))
    return tuple(positions)


def _position_sort_key(position: Position) -> tuple[int, int]:
    return (position.y, position.x)


def _tier_asc_sort_key(tier: SrsEnemyTier) -> int:
    return _TIER_ORDER[tier]


def _tier_desc_sort_key(tier: SrsEnemyTier) -> int:
    return -_tier_asc_sort_key(tier)


def _enemy_salvage_drop_rng(
    state: SrsGameState,
    *,
    danger_score: int,
    composition: Sequence[SrsEnemyTier],
) -> random.Random:
    composition_token = ",".join(tier.value for tier in composition)
    seed = (
        f"{state.descriptor.sector_id}|{state.descriptor.sector_seed}|"
        f"{state.srs_turn}|{state.player_position.x},{state.player_position.y}|"
        f"{danger_score}|{composition_token}"
    )
    return random.Random(seed)


def _enemy_presence(state: SrsGameState) -> bool:
    return state.combat_state is not None and state.combat_state.enemy_presence


def _is_adjacent(a: Position, b: Position) -> bool:
    return max(abs(a.x - b.x), abs(a.y - b.y)) == 1
