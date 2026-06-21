Closes #1110
Refs #1080
Refs #1081
Refs #1082

## 概要

Phase 2B7 向けに、SRS fixture runner、known map 用の簡易 ASCII render、JSON serializable な評価 API を追加しました。

## 変更内容

- `render_known_map(state)` を追加し、known state だけを使う簡易 ASCII render を実装
- `run_fixture.py` を追加し、fixture JSON の load、初期状態構築、command 実行、expect 検証、CLI 実行を実装
- `SrsFixtureRunResult` と `fixture_result_to_jsonable(...)` を追加し、Phase 2C/D から使いやすい出力形式を整理
- 必須 fixture 9 本を追加
- `test_render.py` と `test_fixtures.py` を追加し、render と fixture runner の仕様を固定
- `__init__.py` から必要 API を lazy export し、`python -m ...run_fixture` の warning を回避

## 確認

- `python -m unittest experiments.galactic_exodus.srs.test_render`
- `python -m unittest experiments.galactic_exodus.srs.test_fixtures`
- `python -m unittest experiments.galactic_exodus.srs.test_engine_warp`
- `python -m unittest experiments.galactic_exodus.srs.test_observation`
- `python -m unittest experiments.galactic_exodus.srs.test_engine_interaction`
- `python -m unittest experiments.galactic_exodus.srs.test_engine_movement`
- `python -m unittest experiments.galactic_exodus.srs.test_log`
- `python -m unittest experiments.galactic_exodus.srs.test_model`
- `python -m unittest experiments.galactic_exodus.srs.test_generate`
- `python -m unittest experiments.galactic_exodus.srs.test_contracts`
- `python -m experiments.galactic_exodus.srs.run_fixture experiments/galactic_exodus/srs/fixtures/resource_cache_single_9x9.json`
