from __future__ import annotations

from pathlib import Path


class ValidationError(ValueError):
    pass


_REQUIRED_SNIPPETS = (
    "正本仕様書",
    "Python 実装は実行可能な参照実装",
    "TURN_ONLY",
    "SHARED_FUEL",
    "NEBULA observation = 3x3",
    "RESOURCE_CACHE",
    "STATION",
    "SALVAGE",
    "WARP_EXIT",
    "GameLog",
    "TBX state",
    "#1076",
    "#1165",
    "#1166",
    "#1167",
)


def validate(path: Path) -> None:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise ValidationError(f"missing spec file: {path}") from exc
    except OSError as exc:
        raise ValidationError(f"failed to read spec file {path}: {exc}") from exc

    if "TBD" in text:
        raise ValidationError("phase2_srs_spec.md must not contain TBD")

    for snippet in _REQUIRED_SNIPPETS:
        if snippet not in text:
            raise ValidationError(f"phase2_srs_spec.md is missing required text: {snippet}")

