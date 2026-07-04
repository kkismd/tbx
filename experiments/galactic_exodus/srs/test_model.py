from __future__ import annotations

import unittest

from experiments.galactic_exodus.srs.model import (
    SrsCommand,
    SrsCommandResult,
    Direction,
    Position,
    SrsCombatPhase,
    SrsCombatState,
    SrsEnemyTier,
    SrsEnemyReaction,
    SectorDescriptor,
    SectorType,
    SrsActualMap,
    SrsCell,
    SrsGameState,
    SrsModelError,
    SrsKnownState,
    SrsPlayerCombatState,
    SrsObjectState,
    SrsObjectType,
    SrsPersistentState,
    SrsPlayerAttackAction,
    SrsTerrainType,
    SrsWeaponType,
    create_enemy_combat_state,
    default_weapon_profiles,
    derive_lrs_blocked_routes,
    validate_sector_descriptor,
)


class SrsModelTests(unittest.TestCase):
    def test_direction_enum_round_trip(self) -> None:
        self.assertIs(Direction(Direction.N.value), Direction.N)

    def test_sector_type_enum_round_trip(self) -> None:
        self.assertIs(SectorType(SectorType.RIFT.value), SectorType.RIFT)

    def test_position_equality(self) -> None:
        self.assertEqual(Position(2, 3), Position(2, 3))

    def test_actual_map_contains(self) -> None:
        actual_map = SrsActualMap(
            width=2,
            height=2,
            cells=(
                (SrsCell(SrsTerrainType.FLOOR), SrsCell(SrsTerrainType.DEBRIS)),
                (SrsCell(SrsTerrainType.NEBULA), SrsCell(SrsTerrainType.ASTEROID_FIELD)),
            ),
        )
        self.assertTrue(actual_map.contains(Position(1, 1)))
        self.assertFalse(actual_map.contains(Position(2, 1)))

    def test_actual_map_cell_at(self) -> None:
        cell = SrsCell(SrsTerrainType.RIFT_DISTORTION)
        actual_map = SrsActualMap(width=1, height=1, cells=((cell,),))
        self.assertIs(actual_map.cell_at(Position(0, 0)), cell)

    def test_actual_map_cell_at_rejects_out_of_bounds(self) -> None:
        actual_map = SrsActualMap(width=1, height=1, cells=((SrsCell(SrsTerrainType.FLOOR),),))
        with self.assertRaisesRegex(IndexError, "position out of bounds"):
            actual_map.cell_at(Position(-1, 0))

    def test_sector_descriptor_rift_requires_blocked_edges(self) -> None:
        descriptor = SectorDescriptor(
            sector_id="R-1",
            sector_type=SectorType.RIFT,
            sector_seed=1,
            entry_edge=Direction.N,
        )
        with self.assertRaisesRegex(SrsModelError, "requires at least one blocked edge"):
            validate_sector_descriptor(descriptor)

    def test_sector_descriptor_non_rift_rejects_blocked_edges(self) -> None:
        descriptor = SectorDescriptor(
            sector_id="N-1",
            sector_type=SectorType.NORMAL,
            sector_seed=1,
            entry_edge=Direction.N,
            blocked_edges=frozenset({Direction.E}),
        )
        with self.assertRaisesRegex(SrsModelError, "only RIFT sector may have blocked edges"):
            validate_sector_descriptor(descriptor)

    def test_sector_descriptor_entry_edge_must_not_be_blocked(self) -> None:
        descriptor = SectorDescriptor(
            sector_id="R-2",
            sector_type=SectorType.RIFT,
            sector_seed=1,
            entry_edge=Direction.W,
            blocked_edges=frozenset({Direction.W}),
        )
        with self.assertRaisesRegex(SrsModelError, "entry_edge must not be blocked"):
            validate_sector_descriptor(descriptor)

    def test_game_state_freezes_objects_mapping(self) -> None:
        position = Position(0, 0)
        state = SrsGameState(
            descriptor=SectorDescriptor(
                sector_id="N-1",
                sector_type=SectorType.NORMAL,
                sector_seed=1,
                entry_edge=Direction.N,
            ),
            actual_map=SrsActualMap(
                width=1,
                height=1,
                cells=((SrsCell(SrsTerrainType.FLOOR, object_id="star-1"),),),
            ),
            known_state=SrsKnownState(),
            persistent_state=SrsPersistentState(
                generated_map_id="N-1:1",
                generation_schema_version=1,
                generation_seed=1,
                sector_type=SectorType.NORMAL,
                blocked_edges=frozenset(),
            ),
            player_position=position,
            objects={
                "star-1": SrsObjectState(
                    object_id="star-1",
                    object_type=SrsObjectType.STAR,
                    position=position,
                )
            },
        )

        with self.assertRaises(TypeError):
            state.objects["planet-1"] = SrsObjectState(
                object_id="planet-1",
                object_type=SrsObjectType.PLANET,
                position=Position(0, 0),
            )

    def test_game_state_rejects_object_key_mismatch(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "objects mapping keys must match"):
            SrsGameState(
                descriptor=SectorDescriptor(
                    sector_id="N-1",
                    sector_type=SectorType.NORMAL,
                    sector_seed=1,
                    entry_edge=Direction.N,
                ),
                actual_map=SrsActualMap(
                    width=1,
                    height=1,
                    cells=((SrsCell(SrsTerrainType.FLOOR, object_id="star-1"),),),
                ),
                known_state=SrsKnownState(),
                persistent_state=SrsPersistentState(
                    generated_map_id="N-1:1",
                    generation_schema_version=1,
                    generation_seed=1,
                    sector_type=SectorType.NORMAL,
                    blocked_edges=frozenset(),
                ),
                player_position=Position(0, 0),
                objects={
                    "bad-key": SrsObjectState(
                        object_id="star-1",
                        object_type=SrsObjectType.STAR,
                        position=Position(0, 0),
                    )
                },
            )

    def test_game_state_rejects_map_object_mismatch(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "actual_map object_id values must match"):
            SrsGameState(
                descriptor=SectorDescriptor(
                    sector_id="N-1",
                    sector_type=SectorType.NORMAL,
                    sector_seed=1,
                    entry_edge=Direction.N,
                ),
                actual_map=SrsActualMap(
                    width=1,
                    height=1,
                    cells=((SrsCell(SrsTerrainType.FLOOR, object_id="star-1"),),),
                ),
                known_state=SrsKnownState(),
                persistent_state=SrsPersistentState(
                    generated_map_id="N-1:1",
                    generation_schema_version=1,
                    generation_seed=1,
                    sector_type=SectorType.NORMAL,
                    blocked_edges=frozenset(),
                ),
                player_position=Position(0, 0),
                objects={},
            )

    def test_player_combat_state_uses_issue_1196_fixed_values(self) -> None:
        player = SrsPlayerCombatState()

        self.assertEqual(player.durability, 100)
        self.assertEqual(player.defense, 0)
        self.assertEqual(player.movement_power, 4)
        self.assertEqual(player.photon_torpedo_ammo, 6)
        self.assertEqual(player.photon_torpedo_ammo_capacity, 6)
        self.assertEqual(player.energy, 6)
        self.assertEqual(player.energy_capacity, 6)
        self.assertEqual(player.energy_recovery, 1)

    def test_weapon_profiles_use_issue_1196_fixed_values(self) -> None:
        weapons = default_weapon_profiles()

        self.assertEqual(weapons[SrsWeaponType.PHOTON_TORPEDO].damage, 3)
        self.assertEqual(weapons[SrsWeaponType.PHOTON_TORPEDO].ammo_cost, 1)
        self.assertEqual(weapons[SrsWeaponType.PHOTON_TORPEDO].range, 3)
        self.assertEqual(weapons[SrsWeaponType.PHASER].damage, 1)
        self.assertEqual(weapons[SrsWeaponType.PHASER].energy_cost, 1)
        self.assertEqual(weapons[SrsWeaponType.PHASER].range, 2)
        self.assertIsNone(weapons[SrsWeaponType.ENEMY_WEAPON].damage)
        self.assertEqual(weapons[SrsWeaponType.ENEMY_WEAPON].range, 2)

    def test_create_enemy_combat_state_uses_tier_fixed_values(self) -> None:
        expectations = {
            SrsEnemyTier.TIER1: (3, 6, 3),
            SrsEnemyTier.TIER2: (5, 7, 3),
            SrsEnemyTier.TIER3: (8, 8, 3),
            SrsEnemyTier.TIER4: (12, 10, 3),
        }

        for tier, expected in expectations.items():
            with self.subTest(tier=tier.value):
                enemy = create_enemy_combat_state(
                    enemy_id=f"{tier.value.lower()}-1",
                    tier=tier,
                    position=Position(0, 0),
                )
                self.assertEqual((enemy.durability, enemy.attack_damage, enemy.movement_power), expected)

    def test_combat_state_enemy_presence_reflects_enemy_list(self) -> None:
        empty = SrsCombatState()
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(0, 0),
        )
        occupied = SrsCombatState(enemies={"enemy-1": enemy}, player_attack_target_id="enemy-1")

        self.assertFalse(empty.enemy_presence)
        self.assertTrue(occupied.enemy_presence)
        self.assertTrue(occupied.target_available)

    def test_game_state_rejects_out_of_bounds_combat_enemy(self) -> None:
        enemy = create_enemy_combat_state(
            enemy_id="enemy-1",
            tier=SrsEnemyTier.TIER1,
            position=Position(9, 9),
        )

        with self.assertRaisesRegex(SrsModelError, "combat enemy position must be within actual_map bounds"):
            SrsGameState(
                descriptor=SectorDescriptor(
                    sector_id="N-1",
                    sector_type=SectorType.NORMAL,
                    sector_seed=1,
                    entry_edge=Direction.N,
                ),
                actual_map=SrsActualMap(
                    width=1,
                    height=1,
                    cells=((SrsCell(SrsTerrainType.FLOOR),),),
                ),
                known_state=SrsKnownState(),
                persistent_state=SrsPersistentState(
                    generated_map_id="N-1:1",
                    generation_schema_version=1,
                    generation_seed=1,
                    sector_type=SectorType.NORMAL,
                    blocked_edges=frozenset(),
                ),
                player_position=Position(0, 0),
                combat_state=SrsCombatState(
                    phase=SrsCombatPhase.PLAYER_MOVEMENT,
                    enemies={"enemy-1": enemy},
                    player_attack_target_id="enemy-1",
                ),
                objects={},
            )

    def test_object_state_freezes_metadata(self) -> None:
        resource_cache = SrsObjectState(
            object_id="resource-cache-1",
            object_type=SrsObjectType.RESOURCE_CACHE,
            position=Position(1, 2),
            metadata={"fuel_restore": 5},
        )

        self.assertEqual(resource_cache.metadata["fuel_restore"], 5)
        with self.assertRaises(TypeError):
            resource_cache.metadata["fuel_restore"] = 4

    def test_known_state_freezes_known_cells(self) -> None:
        state = SrsKnownState(
            discovered_cells={Position(0, 0)},
            known_cells={Position(0, 0): SrsCell(SrsTerrainType.FLOOR)},
        )

        with self.assertRaises(TypeError):
            state.known_cells[Position(1, 1)] = SrsCell(SrsTerrainType.NEBULA)

    def test_known_state_freezes_visited_cells(self) -> None:
        state = SrsKnownState(visited_cells={Position(0, 0)})

        with self.assertRaises(AttributeError):
            state.visited_cells.add(Position(1, 1))

    def test_known_state_rejects_known_cell_outside_discovered_cells(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "known_cells keys must be a subset"):
            SrsKnownState(
                discovered_cells={Position(0, 0)},
                known_cells={Position(1, 1): SrsCell(SrsTerrainType.FLOOR)},
            )

    def test_srs_command_rejects_empty_move_route(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "MOVE_ROUTE requires a non-empty route"):
            SrsCommand(command_type="MOVE_ROUTE")

    def test_srs_command_rejects_move_to_without_target(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "MOVE_TO requires a target"):
            SrsCommand(command_type="MOVE_TO")

    def test_srs_command_rejects_interact_without_target_object_id(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "INTERACT requires a target_object_id"):
            SrsCommand(command_type="INTERACT")

    def test_srs_command_rejects_warp_exit_without_exit_direction(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "WARP_EXIT requires an exit_direction"):
            SrsCommand(command_type="WARP_EXIT")

    def test_srs_command_normalizes_route_to_tuple(self) -> None:
        command = SrsCommand(command_type="MOVE_ROUTE", route=[Direction.N, Direction.E])
        self.assertEqual(command.route, (Direction.N, Direction.E))

    def test_srs_command_normalizes_exit_direction(self) -> None:
        command = SrsCommand(command_type="WARP_EXIT", exit_direction="N")
        self.assertEqual(command.exit_direction, Direction.N)

    def test_srs_command_normalizes_combat_action_fields(self) -> None:
        command = SrsCommand(
            command_type="COMBAT_STEP",
            player_attack_action="ATTACK",
            player_attack_weapon="PHOTON_TORPEDO",
            enemy_reactions={"enemy-1": "COUNTERATTACK"},
        )

        self.assertEqual(command.player_attack_action, SrsPlayerAttackAction.ATTACK)
        self.assertEqual(command.player_attack_weapon, SrsWeaponType.PHOTON_TORPEDO)
        self.assertEqual(command.enemy_reactions["enemy-1"], SrsEnemyReaction.COUNTERATTACK)

    def test_srs_command_rejects_attack_without_weapon(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "ATTACK requires a player_attack_weapon"):
            SrsCommand(command_type="COMBAT_STEP", player_attack_action="ATTACK")

    def test_srs_command_rejects_combat_fields_for_non_combat_command(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "combat action fields require COMBAT_STEP"):
            SrsCommand(command_type="MOVE_TO", target=Position(1, 1), player_attack_action="SKIP")

    def test_player_combat_state_allows_zero_durability(self) -> None:
        player = SrsPlayerCombatState(durability=0)

        self.assertEqual(player.durability, 0)

    def test_srs_command_rejects_invalid_exit_direction(self) -> None:
        with self.assertRaisesRegex(SrsModelError, "exit_direction must be a Direction value"):
            SrsCommand(command_type="WARP_EXIT", exit_direction="X")

    def test_derive_lrs_blocked_routes_returns_rift_edges(self) -> None:
        descriptor = SectorDescriptor(
            sector_id="rift-1",
            sector_type=SectorType.RIFT,
            sector_seed=1,
            entry_edge=Direction.S,
            blocked_edges=frozenset({Direction.N, Direction.E}),
        )

        self.assertEqual(
            derive_lrs_blocked_routes(descriptor),
            frozenset({("rift-1", Direction.N), ("rift-1", Direction.E)}),
        )

    def test_derive_lrs_blocked_routes_ignores_non_rift_sectors(self) -> None:
        descriptor = SectorDescriptor(
            sector_id="normal-1",
            sector_type=SectorType.NORMAL,
            sector_seed=1,
            entry_edge=Direction.S,
        )

        self.assertEqual(derive_lrs_blocked_routes(descriptor), frozenset())

    def test_srs_command_result_freezes_events(self) -> None:
        state = SrsGameState(
            descriptor=SectorDescriptor(
                sector_id="N-1",
                sector_type=SectorType.NORMAL,
                sector_seed=1,
                entry_edge=Direction.N,
            ),
            actual_map=SrsActualMap(width=1, height=1, cells=((SrsCell(SrsTerrainType.FLOOR),),)),
            known_state=SrsKnownState(),
            persistent_state=SrsPersistentState(
                generated_map_id="N-1:1",
                generation_schema_version=1,
                generation_seed=1,
                sector_type=SectorType.NORMAL,
                blocked_edges=frozenset(),
            ),
            player_position=Position(0, 0),
            objects={},
        )
        result = SrsCommandResult(
            state=state,
            events=[],
        )
        self.assertEqual(result.events, ())


if __name__ == "__main__":
    unittest.main()
