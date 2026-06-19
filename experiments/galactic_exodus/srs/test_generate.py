from __future__ import annotations

import json
import unittest
from collections import Counter
from pathlib import Path
from typing import Any

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.generate import EDGE_POSITIONS, SrsGenerationError, create_sector
from experiments.galactic_exodus.srs.model import (
    Direction,
    Position,
    SectorDescriptor,
    SectorType,
    SrsGameState,
    SrsModelError,
)


REPO_ROOT = Path(__file__).resolve().parents[3]
SRS_DIR = REPO_ROOT / "experiments" / "galactic_exodus" / "srs"
FIXTURES_DIR = SRS_DIR / "fixtures"


def summarize_state(state: SrsGameState) -> dict[str, Any]:
    terrain_counts = Counter(
        cell.terrain.value
        for row in state.actual_map.cells
        for cell in row
    )
    warp_flags = {
        f"{position.x},{position.y}": sorted(cell.warp_flags, key=lambda direction: direction.value)
        for position, cell in _iter_cells(state)
        if cell.warp_flags
    }
    object_counts = Counter(object_state.object_type.value for object_state in state.objects.values())
    object_positions = {
        object_id: [object_state.position.x, object_state.position.y]
        for object_id, object_state in sorted(state.objects.items())
    }
    return {
        "width": state.actual_map.width,
        "height": state.actual_map.height,
        "player_position": [state.player_position.x, state.player_position.y],
        "warp_flags": {
            key: [direction.value for direction in flags]
            for key, flags in warp_flags.items()
        },
        "terrain_counts": dict(sorted(terrain_counts.items())),
        "object_counts": dict(sorted(object_counts.items())),
        "object_positions": object_positions,
        "blocked_warp_positions": [
            f"{position.x},{position.y}"
            for direction, position in sorted(
                ((direction, EDGE_POSITIONS[direction]) for direction in state.descriptor.blocked_edges),
                key=lambda item: item[0].value,
            )
        ],
        "known_discovered_count": len(state.known_state.discovered_cells),
    }


def _iter_cells(state: SrsGameState):
    for y, row in enumerate(state.actual_map.cells):
        for x, cell in enumerate(row):
            yield Position(x, y), cell


class SrsGenerateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_same_seed_same_map(self) -> None:
        descriptor = SectorDescriptor("normal-1", SectorType.NORMAL, 1001, Direction.S)
        first = create_sector(descriptor, contracts=self.contracts)
        second = create_sector(descriptor, contracts=self.contracts)

        self.assertEqual(summarize_state(first), summarize_state(second))

    def test_different_seed_changes_summary(self) -> None:
        first = create_sector(
            SectorDescriptor("normal-1", SectorType.NORMAL, 1001, Direction.S),
            contracts=self.contracts,
        )
        second = create_sector(
            SectorDescriptor("normal-1", SectorType.NORMAL, 1002, Direction.S),
            contracts=self.contracts,
        )

        self.assertNotEqual(
            summarize_state(first)["object_positions"],
            summarize_state(second)["object_positions"],
        )

    def test_rift_requires_blocked_edges(self) -> None:
        descriptor = SectorDescriptor("rift-1", SectorType.RIFT, 4001, Direction.S)
        with self.assertRaises(SrsModelError):
            create_sector(descriptor, contracts=self.contracts)

    def test_non_rift_rejects_blocked_edges(self) -> None:
        descriptor = SectorDescriptor(
            "normal-1",
            SectorType.NORMAL,
            1001,
            Direction.S,
            blocked_edges=frozenset({Direction.N}),
        )
        with self.assertRaises(SrsModelError):
            create_sector(descriptor, contracts=self.contracts)

    def test_entry_edge_blocked_is_rejected(self) -> None:
        descriptor = SectorDescriptor(
            "rift-1",
            SectorType.RIFT,
            4001,
            Direction.N,
            blocked_edges=frozenset({Direction.N}),
        )
        with self.assertRaises(SrsModelError):
            create_sector(descriptor, contracts=self.contracts)

    def test_rejects_non_9x9_size(self) -> None:
        descriptor = SectorDescriptor("normal-1", SectorType.NORMAL, 1001, Direction.S)
        with self.assertRaisesRegex(SrsGenerationError, "only 9x9"):
            create_sector(descriptor, width=11, contracts=self.contracts)

    def test_blocked_edge_has_no_warp_flags(self) -> None:
        state = create_sector(
            SectorDescriptor(
                "rift-1",
                SectorType.RIFT,
                4001,
                Direction.S,
                blocked_edges=frozenset({Direction.N}),
            ),
            contracts=self.contracts,
        )

        self.assertNotIn("4,0", summarize_state(state)["warp_flags"])

    def test_non_blocked_edges_have_warp_candidates(self) -> None:
        state = create_sector(
            SectorDescriptor(
                "rift-1",
                SectorType.RIFT,
                4001,
                Direction.S,
                blocked_edges=frozenset({Direction.N}),
            ),
            contracts=self.contracts,
        )

        self.assertEqual(
            summarize_state(state)["warp_flags"],
            {
                "0,4": ["W"],
                "4,8": ["S"],
                "8,4": ["E"],
            },
        )

    def test_star_exactly_one(self) -> None:
        state = create_sector(
            SectorDescriptor("normal-1", SectorType.NORMAL, 1001, Direction.S),
            contracts=self.contracts,
        )

        self.assertEqual(summarize_state(state)["object_counts"]["STAR"], 1)

    def test_planet_count_for_9x9_is_two(self) -> None:
        state = create_sector(
            SectorDescriptor("normal-1", SectorType.NORMAL, 1001, Direction.S),
            contracts=self.contracts,
        )

        self.assertEqual(summarize_state(state)["object_counts"]["PLANET"], 2)

    def test_normal_has_one_salvage(self) -> None:
        state = create_sector(
            SectorDescriptor("normal-1", SectorType.NORMAL, 1001, Direction.S),
            contracts=self.contracts,
        )

        self.assertEqual(summarize_state(state)["object_counts"]["SALVAGE"], 1)

    def test_base_has_one_station(self) -> None:
        state = create_sector(
            SectorDescriptor("base-1", SectorType.BASE, 2001, Direction.S),
            contracts=self.contracts,
        )

        self.assertEqual(summarize_state(state)["object_counts"]["STATION"], 1)

    def test_resource_has_one_resource_cache(self) -> None:
        state = create_sector(
            SectorDescriptor("resource-1", SectorType.RESOURCE, 3001, Direction.S),
            contracts=self.contracts,
        )

        self.assertEqual(summarize_state(state)["object_counts"]["RESOURCE_CACHE"], 1)

    def test_rift_has_one_salvage(self) -> None:
        state = create_sector(
            SectorDescriptor(
                "rift-1",
                SectorType.RIFT,
                4001,
                Direction.S,
                blocked_edges=frozenset({Direction.N}),
            ),
            contracts=self.contracts,
        )

        self.assertEqual(summarize_state(state)["object_counts"]["SALVAGE"], 1)

    def test_actual_map_and_known_state_are_separated(self) -> None:
        state = create_sector(
            SectorDescriptor("normal-1", SectorType.NORMAL, 1001, Direction.S),
            contracts=self.contracts,
        )

        self.assertTrue(any(cell.object_id is not None for _, cell in _iter_cells(state)))
        self.assertEqual(state.known_state.discovered_cells, frozenset())

    def test_persistent_state_records_generation_identity(self) -> None:
        descriptor = SectorDescriptor(
            "rift-1",
            SectorType.RIFT,
            4001,
            Direction.S,
            blocked_edges=frozenset({Direction.N}),
        )
        state = create_sector(descriptor, contracts=self.contracts)

        self.assertEqual(state.persistent_state.generated_map_id, "rift-1:4001")
        self.assertEqual(state.persistent_state.generation_seed, 4001)
        self.assertEqual(state.persistent_state.blocked_edges, frozenset({Direction.N}))
        self.assertEqual(state.persistent_state.warp_flags[Position(8, 4)], frozenset({Direction.E}))
        self.assertEqual(
            set(state.persistent_state.celestial_body_positions),
            {"star-1", "planet-1", "planet-2"},
        )

    def test_fixture_normal_minimal_9x9(self) -> None:
        self.assert_fixture("normal_minimal_9x9.json")

    def test_fixture_base_minimal_9x9(self) -> None:
        self.assert_fixture("base_minimal_9x9.json")

    def test_fixture_resource_minimal_9x9(self) -> None:
        self.assert_fixture("resource_minimal_9x9.json")

    def test_fixture_rift_n_blocked_9x9(self) -> None:
        self.assert_fixture("rift_n_blocked_9x9.json")

    def assert_fixture(self, filename: str) -> None:
        payload = json.loads((FIXTURES_DIR / filename).read_text(encoding="utf-8"))
        descriptor_json = payload["descriptor"]
        descriptor = SectorDescriptor(
            sector_id=descriptor_json["sector_id"],
            sector_type=SectorType(descriptor_json["sector_type"]),
            sector_seed=descriptor_json["sector_seed"],
            entry_edge=Direction(descriptor_json["entry_edge"]),
            blocked_edges=frozenset(Direction(edge) for edge in descriptor_json["blocked_edges"]),
        )
        state = create_sector(descriptor, contracts=self.contracts)
        summary = summarize_state(state)
        expected = payload["expected"]

        for key, value in expected.items():
            self.assertEqual(summary[key], value)


if __name__ == "__main__":
    unittest.main()
