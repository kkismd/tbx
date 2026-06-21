"""Galactic Exodus Phase 2 SRS prototype."""

__all__ = [
    "SrsFixtureError",
    "SrsFixtureRunResult",
    "fixture_result_to_jsonable",
    "render_known_map",
    "run_fixture",
    "run_fixture_data",
]


def __getattr__(name: str):
    if name == "render_known_map":
        from experiments.galactic_exodus.srs.render import render_known_map

        return render_known_map
    if name in {
        "SrsFixtureError",
        "SrsFixtureRunResult",
        "fixture_result_to_jsonable",
        "run_fixture",
        "run_fixture_data",
    }:
        from importlib import import_module

        run_fixture_module = import_module("experiments.galactic_exodus.srs.run_fixture")

        return getattr(run_fixture_module, name)
    raise AttributeError(name)
