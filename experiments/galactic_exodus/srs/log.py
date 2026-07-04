from __future__ import annotations

from typing import Any, Mapping, Sequence

from experiments.galactic_exodus.srs.model import SrsGameLog, SrsTurnEvent


MOVE_ACCEPTED = "MOVE_ACCEPTED"
MOVE_REJECTED = "MOVE_REJECTED"
WAIT_ACCEPTED = "WAIT_ACCEPTED"
STOPPED_BEFORE_IMPASSABLE = "STOPPED_BEFORE_IMPASSABLE"
OBSERVATION_UPDATED = "OBSERVATION_UPDATED"
INTERACT_ACCEPTED = "INTERACT_ACCEPTED"
INTERACT_REJECTED = "INTERACT_REJECTED"
OBJECT_CONSUMED = "OBJECT_CONSUMED"
STATION_ACTIVATED = "STATION_ACTIVATED"
WARP_EXIT_ACCEPTED = "WARP_EXIT_ACCEPTED"
WARP_EXIT_REJECTED = "WARP_EXIT_REJECTED"
COMBAT_TRANSITIONED = "COMBAT_TRANSITIONED"
COMBAT_REJECTED = "COMBAT_REJECTED"
ENCOUNTER_ROLLED = "ENCOUNTER_ROLLED"


def make_turn_event(
    *,
    srs_turn: int,
    event_type: str,
    payload: Mapping[str, Any],
) -> SrsTurnEvent:
    return SrsTurnEvent(
        srs_turn=srs_turn,
        event_type=event_type,
        payload=dict(payload),
    )


def build_srs_log(events: Sequence[SrsTurnEvent]) -> SrsGameLog:
    return SrsGameLog(events=tuple(events))
