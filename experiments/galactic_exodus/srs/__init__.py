"""Galactic Exodus Phase 2 SRS prototype."""

__all__ = [
    "SrsFixtureError",
    "SrsFixtureRunResult",
    "fixture_result_to_jsonable",
    "render_display_map",
    "render_known_map",
    "render_known_map_spaced",
    "run_fixture",
    "run_fixture_data",
]


def __getattr__(name: str):
    if name in {"render_display_map", "render_known_map", "render_known_map_spaced"}:
        from importlib import import_module

        render_module = import_module("experiments.galactic_exodus.srs.render")

        return getattr(render_module, name)
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
