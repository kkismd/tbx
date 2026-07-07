from __future__ import annotations

from dataclasses import replace

from experiments.galactic_exodus import engine, simulate
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SrsCell,
    SrsCombatState,
    SrsEnemyTier,
    SrsObjectType,
    SrsTerrainType,
    create_enemy_combat_state,
)
from experiments.galactic_exodus.srs.test_engine_movement import make_state as make_srs_state
from experiments.galactic_exodus.srs.test_engine_movement import place_object, reveal_positions
from experiments.galactic_exodus.test_engine import filled_cells, make_actual_map, make_state as make_lrs_state


def make_lrs_display_snapshot_state() -> engine.GameState:
    cells = filled_cells(".")
    cells[(4, 7)] = "N"
    actual_map = make_actual_map(
        cells=cells,
        base_position=(4, 5),
        resource_positions=((3, 6),),
    )
    vertical_rift = simulate.normalize_edge((3, 6), (4, 6))
    horizontal_rift = simulate.normalize_edge((3, 5), (3, 4))
    known_cells = {
        (1, 1): "S",
        (2, 1): ".",
        (3, 1): ".",
        (2, 4): ".",
        (3, 4): ".",
        (4, 4): ".",
        (2, 5): ".",
        (3, 5): ".",
        (4, 5): "B",
        (2, 6): ".",
        (3, 6): "R",
        (4, 6): ".",
        (2, 7): ".",
        (3, 7): ".",
        (4, 7): "N",
        (8, 8): "H",
    }
    return make_lrs_state(
        actual_map=actual_map,
        player_position=(3, 5),
        known_cells=known_cells,
        visited_cells={(1, 1), (3, 5)},
        known_routes={
            vertical_rift: engine.ROUTE_RIFT,
            horizontal_rift: engine.ROUTE_RIFT,
        },
        path=[(1, 1), (2, 1), (3, 1), (3, 5)],
    )


def expected_lrs_display_snapshot() -> str:
    return "\n".join(
        [
            "  +---+---+---+---+---+---+---+---+",
            "8 | ?   ?   ?   ?   ?   ?   ?   H |",
            "  +                               +",
            "7 | ?   .   .   N   ?   ?   ?   ? |",
            "  +           +                   +",
            "6 | ?   .   R | .   ?   ?   ?   ? |",
            "  +           +                   +",
            "5 | ?   .   @   B   ?   ?   ?   ? |",
            "  +       +---+                   +",
            "4 | ?   .   .   .   ?   ?   ?   ? |",
            "  +                               +",
            "3 | ?   ?   ?   ?   ?   ?   ?   ? |",
            "  +                               +",
            "2 | ?   ?   ?   ?   ?   ?   ?   ? |",
            "  +                               +",
            "1 | S   .   .   ?   ?   ?   ?   ? |",
            "  +---+---+---+---+---+---+---+---+",
            "    1   2   3   4   5   6   7   8",
        ]
    )


def make_lrs_dense_rift_snapshot_state() -> engine.GameState:
    cells = filled_cells(".")
    known_cells = {(x, y): "." for y in range(1, 9) for x in range(1, 9)}
    known_cells[(1, 1)] = "S"
    known_cells[(8, 8)] = "H"
    known_edges = (
        simulate.normalize_edge((2, 4), (2, 5)),
        simulate.normalize_edge((3, 4), (3, 5)),
        simulate.normalize_edge((4, 4), (4, 5)),
        simulate.normalize_edge((3, 4), (4, 4)),
        simulate.normalize_edge((3, 5), (4, 5)),
        simulate.normalize_edge((1, 8), (2, 8)),
        simulate.normalize_edge((8, 1), (8, 2)),
    )
    hidden_edge = simulate.normalize_edge((6, 6), (7, 6))
    actual_map = make_actual_map(
        cells=cells,
        rift_edges=known_edges + (hidden_edge,),
    )
    return make_lrs_state(
        actual_map=actual_map,
        player_position=(4, 4),
        known_cells=known_cells,
        visited_cells={(1, 1), (4, 4)},
        known_routes={edge: engine.ROUTE_RIFT for edge in known_edges},
    )


def expected_lrs_dense_rift_snapshot() -> str:
    return "\n".join(
        [
            "  +---+---+---+---+---+---+---+---+",
            "8 | . | .   .   .   .   .   .   H |",
            "  +   +                           +",
            "7 | .   .   .   .   .   .   .   . |",
            "  +                               +",
            "6 | .   .   .   .   .   .   .   . |",
            "  +           +                   +",
            "5 | .   .   . | .   .   .   .   . |",
            "  +   +---+---+---+               +",
            "4 | .   .   . | @   .   .   .   . |",
            "  +           +                   +",
            "3 | .   .   .   .   .   .   .   . |",
            "  +                               +",
            "2 | .   .   .   .   .   .   .   . |",
            "  +                           +---+",
            "1 | S   .   .   .   .   .   .   . |",
            "  +---+---+---+---+---+---+---+---+",
            "    1   2   3   4   5   6   7   8",
        ]
    )


def make_srs_display_snapshot_state():
    state = replace(make_srs_state(), player_position=Position(6, 3))

    barrier_positions = [Position(8, y) for y in range(6)]
    floor_positions = [
        Position(3, 6),
        Position(4, 6),
        Position(5, 6),
        Position(3, 5),
        Position(4, 5),
        Position(5, 5),
        Position(6, 5),
        Position(7, 5),
        Position(3, 4),
        Position(4, 4),
        Position(5, 4),
        Position(6, 4),
        Position(7, 4),
        Position(3, 3),
        Position(4, 3),
        Position(5, 3),
        Position(6, 3),
        Position(7, 3),
        Position(3, 2),
        Position(5, 2),
        Position(6, 2),
        Position(7, 2),
        Position(2, 1),
        Position(3, 1),
        Position(4, 1),
        Position(5, 1),
        Position(6, 1),
        Position(7, 1),
    ]
    warp_positions = [Position(x, 0) for x in range(2, 8)]
    known_positions = set(floor_positions)
    known_positions.update(barrier_positions)
    known_positions.update(warp_positions)
    known_positions.add(Position(4, 2))
    known_positions.add(Position(4, 4))

    for position in barrier_positions:
        state = _replace_srs_cell(
            state,
            position,
            terrain=SrsTerrainType.RIFT_BARRIER,
            warp_flags=frozenset(),
        )
    for position in warp_positions:
        state = _replace_srs_cell(state, position, warp_flags=frozenset({Direction.S}))

    state = place_object(state, Position(4, 2), SrsObjectType.SALVAGE, "salvage-a")
    enemy = create_enemy_combat_state(
        enemy_id="enemy-1",
        tier=SrsEnemyTier.TIER2,
        position=Position(4, 4),
    )
    state = replace(
        state,
        combat_state=SrsCombatState(
            enemies={"enemy-1": enemy},
            player_attack_target_id="enemy-1",
        ),
    )
    return reveal_positions(state, known_positions)


def expected_srs_display_snapshot() -> str:
    return "\n".join(
        [
            " 9   ?  ?  ?  ?  ?  ?  ?  ?  ?",
            " 8   ?  ?  ?  ?  ?  ?  ?  ?  ?",
            " 7   ?  ?  ?  .  .  .  ?  ?  ?",
            " 6   ?  ?  ?  .  .  .  .  .  #",
            " 5   ?  ?  ?  . e1  .  .  .  #",
            " 4   ?  ?  ?  .  .  .  @  .  #",
            " 3   ?  ?  ?  .  $  .  .  .  #",
            " 2   ?  ?  .  .  .  .  .  .  #",
            " 1   ?  ?  v  v  v  v  v  v  #",
            "",
            "     1  2  3  4  5  6  7  8  9",
        ]
    )


def make_srs_symbol_contract_snapshot_state():
    state = make_srs_state()
    for y in range(state.actual_map.height):
        for x in range(state.actual_map.width):
            state = _replace_srs_cell(state, Position(x, y), warp_flags=frozenset())

    player_position = Position(0, 6)
    enemy_position = Position(1, 6)
    hidden_position = Position(0, 8)

    state = _replace_srs_cell(
        state,
        player_position,
        terrain=SrsTerrainType.ASTEROID,
        warp_flags=frozenset({Direction.N}),
    )
    state = place_object(state, player_position, SrsObjectType.STATION, "station-under-player")
    state = _replace_srs_cell(
        state,
        enemy_position,
        terrain=SrsTerrainType.ASTEROID,
        warp_flags=frozenset({Direction.E}),
    )
    state = place_object(state, enemy_position, SrsObjectType.SALVAGE, "salvage-under-enemy")

    visible_objects = (
        (Position(2, 6), SrsObjectType.SALVAGE, "salvage-a", False),
        (Position(3, 6), SrsObjectType.SALVAGE, "salvage-b", True),
        (Position(4, 6), SrsObjectType.RESOURCE_CACHE, "cache-a", False),
        (Position(5, 6), SrsObjectType.RESOURCE_CACHE, "cache-b", True),
        (Position(6, 6), SrsObjectType.STATION, "station-a", False),
        (Position(7, 6), SrsObjectType.STAR, "star-a", False),
        (Position(8, 6), SrsObjectType.PLANET, "planet-a", False),
    )
    for position, object_type, object_id, consumed in visible_objects:
        state = place_object(state, position, object_type, object_id)
        if consumed:
            state = replace(
                state,
                objects={
                    **state.objects,
                    object_id: replace(state.objects[object_id], consumed=True),
                },
            )

    state = _replace_srs_cell(
        state,
        Position(0, 5),
        terrain=SrsTerrainType.ASTEROID,
        warp_flags=frozenset(),
    )
    warp_cells = (
        (Position(1, 5), frozenset({Direction.N})),
        (Position(2, 5), frozenset({Direction.E})),
        (Position(3, 5), frozenset({Direction.S})),
        (Position(4, 5), frozenset({Direction.W})),
        (Position(5, 5), frozenset({Direction.N, Direction.E})),
    )
    for position, warp_flags in warp_cells:
        state = _replace_srs_cell(state, position, warp_flags=warp_flags)

    state = _replace_srs_cell(
        state,
        hidden_position,
        terrain=SrsTerrainType.ASTEROID,
        warp_flags=frozenset({Direction.S}),
    )
    state = place_object(state, hidden_position, SrsObjectType.STAR, "star-hidden")

    discovered = [
        Position(x, y)
        for y in range(state.actual_map.height)
        for x in range(state.actual_map.width)
        if Position(x, y) != hidden_position
    ]
    state = reveal_positions(state, discovered)
    visible_enemy = create_enemy_combat_state(
        enemy_id="enemy-1",
        tier=SrsEnemyTier.TIER2,
        position=enemy_position,
    )
    return replace(
        state,
        player_position=player_position,
        combat_state=SrsCombatState(
            enemies={"enemy-1": visible_enemy},
            player_attack_target_id="enemy-1",
        ),
    )


def expected_srs_symbol_contract_snapshot() -> str:
    return "\n".join(
        [
            " 9   ?  .  .  .  .  .  .  .  .",
            " 8   .  .  .  .  .  .  .  .  .",
            " 7   @ e1  $  s  R  r  S  *  o",
            " 6   #  ^  >  v  <  +  .  .  .",
            " 5   .  .  .  .  .  .  .  .  .",
            " 4   .  .  .  .  .  .  .  .  .",
            " 3   .  .  .  .  .  .  .  .  .",
            " 2   .  .  .  .  .  .  .  .  .",
            " 1   .  .  .  .  .  .  .  .  .",
            "",
            "     1  2  3  4  5  6  7  8  9",
        ]
    )


def _replace_srs_cell(state, position: Position, *, terrain=None, object_id=None, warp_flags=None):
    rows = [list(row) for row in state.actual_map.cells]
    current = state.actual_map.cell_at(position)
    rows[position.y][position.x] = SrsCell(
        terrain=current.terrain if terrain is None else terrain,
        object_id=current.object_id if object_id is None else object_id,
        actor_id=current.actor_id,
        warp_flags=current.warp_flags if warp_flags is None else warp_flags,
    )
    return replace(
        state,
        actual_map=replace(
            state.actual_map,
            cells=tuple(tuple(row) for row in rows),
        ),
    )
