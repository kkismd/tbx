from __future__ import annotations

import json
import unittest
from pathlib import Path

from experiments.galactic_exodus.srs.contracts import load_default_contracts
from experiments.galactic_exodus.srs.evaluate_policies import (
    EvaluationCase,
    EvaluationCaseError,
    InitialRevealMode,
    RevisitMode,
    build_default_evaluation_cases,
)
from experiments.galactic_exodus.srs.model import CostMode, Direction, SectorDescriptor, SectorType, SrsGameState


REPO_ROOT = Path(__file__).resolve().parents[3]


class EvaluationCaseTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contracts = load_default_contracts(REPO_ROOT)

    def test_metadata_is_stable_and_jsonable(self) -> None:
        case = EvaluationCase(
            case_id="normal-turn-only-first-visit",
            sector_id="normal-1001",
            sector_type=SectorType.NORMAL,
            sector_seed=1001,
            entry_edge=Direction.S,
            blocked_edges=frozenset(),
            selected_exit_edge=Direction.N,
            cost_mode=CostMode.TURN_ONLY,
            initial_fuel=0,
            max_fuel=0,
            initial_reveal_mode=InitialRevealMode.LOCAL_MOVEMENT,
            revisit_mode=RevisitMode.FIRST_VISIT,
        )

        metadata = dict(case.metadata())

        self.assertEqual(
            metadata,
            {
                "case_id": "normal-turn-only-first-visit",
                "sector_id": "normal-1001",
                "sector_type": "NORMAL",
                "sector_seed": 1001,
                "entry_edge": "S",
                "blocked_edges": [],
                "selected_exit_edge": "N",
                "cost_mode": "TURN_ONLY",
                "initial_fuel": 0,
                "max_fuel": 0,
                "initial_reveal_mode": "LOCAL_MOVEMENT",
                "revisit_mode": "FIRST_VISIT",
            },
        )
        self.assertNotIn(str(REPO_ROOT), json.dumps(metadata))

    def test_rejects_selected_exit_edge_that_is_blocked(self) -> None:
        with self.assertRaisesRegex(EvaluationCaseError, "selected_exit_edge"):
            EvaluationCase(
                case_id="rift-invalid-selected-exit",
                sector_id="rift-4001",
                sector_type=SectorType.RIFT,
                sector_seed=4001,
                entry_edge=Direction.S,
                blocked_edges=frozenset({Direction.N}),
                selected_exit_edge=Direction.N,
                cost_mode=CostMode.TURN_ONLY,
                initial_fuel=0,
                max_fuel=0,
                initial_reveal_mode=InitialRevealMode.NONE,
                revisit_mode=RevisitMode.FIRST_VISIT,
            )

    def test_build_sector_descriptor(self) -> None:
        case = build_default_evaluation_cases()[0]

        descriptor = case.build_sector_descriptor()

        self.assertEqual(
            descriptor,
            SectorDescriptor(
                sector_id="normal-1001",
                sector_type=SectorType.NORMAL,
                sector_seed=1001,
                entry_edge=Direction.S,
                blocked_edges=frozenset(),
            ),
        )

    def test_build_initial_state(self) -> None:
        case = build_default_evaluation_cases()[2]

        state = case.build_initial_state(contracts=self.contracts)

        self.assertIsInstance(state, SrsGameState)
        self.assertEqual(state.descriptor.sector_id, "resource-3001")
        self.assertEqual(state.fuel, 2)
        self.assertEqual(state.max_fuel, 9)
        self.assertEqual(len(state.known_state.discovered_cells), 81)

    def test_nebula_local_case_uses_3x3_observation(self) -> None:
        case = next(
            candidate
            for candidate in build_default_evaluation_cases()
            if candidate.case_id == "nebula-local-3x3-first-visit"
        )

        state = case.build_initial_state(contracts=self.contracts)

        self.assertEqual(state.actual_map.cell_at(state.player_position).terrain.value, "NEBULA")
        self.assertEqual(len(state.known_state.discovered_cells), 9)

    def test_revisit_case_restores_deterministic_persistent_state(self) -> None:
        case = next(
            candidate
            for candidate in build_default_evaluation_cases()
            if candidate.case_id == "resource-cache-revisit"
        )

        state = case.build_initial_state(contracts=self.contracts)

        self.assertEqual(state.fuel, 2)
        self.assertEqual(state.max_fuel, 9)
        self.assertEqual(state.known_state.visited_cells, frozenset())
        self.assertEqual(state.persistent_state.consumed_object_ids, frozenset({"resource-cache-1"}))
        self.assertTrue(state.objects["resource-cache-1"].consumed)


class DefaultEvaluationCasesTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.cases = build_default_evaluation_cases()

    def test_default_cases_are_not_empty(self) -> None:
        self.assertTrue(self.cases)

    def test_default_cases_include_turn_only_and_shared_fuel(self) -> None:
        cost_modes = {case.cost_mode for case in self.cases}

        self.assertIn(CostMode.TURN_ONLY, cost_modes)
        self.assertIn(CostMode.SHARED_FUEL, cost_modes)

    def test_default_cases_cover_required_sector_categories(self) -> None:
        sector_types = {case.sector_type for case in self.cases}

        self.assertTrue(
            {SectorType.NORMAL, SectorType.RESOURCE, SectorType.BASE, SectorType.RIFT}.issubset(sector_types)
        )

    def test_default_cases_cover_first_visit_revisit_blocked_edge_and_multiple_exits(self) -> None:
        revisit_modes = {case.revisit_mode for case in self.cases}

        self.assertIn(RevisitMode.FIRST_VISIT, revisit_modes)
        self.assertTrue(any(case.revisit_mode is not RevisitMode.FIRST_VISIT for case in self.cases))
        self.assertTrue(any(case.blocked_edges for case in self.cases))
        self.assertTrue(any(len(case.open_exit_edges()) > 1 for case in self.cases))

    def test_default_case_ids_are_unique_and_stable(self) -> None:
        case_ids = [case.case_id for case in self.cases]

        self.assertEqual(len(case_ids), len(set(case_ids)))
        self.assertEqual(case_ids, [case.metadata()["case_id"] for case in self.cases])


if __name__ == "__main__":
    unittest.main()
