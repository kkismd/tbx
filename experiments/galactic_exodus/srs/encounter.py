from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from types import MappingProxyType
from typing import Mapping, Sequence

from experiments.galactic_exodus.srs.model import (
    Position,
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


def _freeze_mapping(mapping: Mapping[object, object]) -> Mapping[object, object]:
    return MappingProxyType(dict(mapping))


_TIER_ORDER = {
    SrsEnemyTier.TIER1: 1,
    SrsEnemyTier.TIER2: 2,
    SrsEnemyTier.TIER3: 3,
    SrsEnemyTier.TIER4: 4,
}

ENEMY_GROUP_COSTS: Mapping[SrsEnemyTier, int] = _freeze_mapping(
    {
        SrsEnemyTier.TIER1: 1,
        SrsEnemyTier.TIER2: 2,
        SrsEnemyTier.TIER3: 3,
        SrsEnemyTier.TIER4: 5,
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
    return tuple(
        create_enemy_combat_state(
            enemy_id=f"enemy-{index}",
            tier=tier,
            position=position,
        )
        for index, (tier, position) in enumerate(zip(selected_tiers, candidates, strict=True), start=1)
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


def _validated_danger_score(danger_score: int) -> int:
    if danger_score not in ENCOUNTER_GROUP_BUDGETS:
        raise SrsEncounterError("danger_score must be in range 0..4")
    return danger_score


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
