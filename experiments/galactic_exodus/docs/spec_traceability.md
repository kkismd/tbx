# Galactic Exodus spec traceability audit

Issue: #1260
Base branch: `integration/882-galactic-exodus`
Date: 2026-07-06

This document is an audit matrix from issue decisions to repository reflection. It does not move or rewrite the full specification text. The purpose is to make it visible which issue decisions are already reflected in code, tests, fixtures, or docs, and which items still need a dedicated spec source-of-truth file or follow-up issue.

## Scope

Primary scope:

```text
#1078 and later Galactic Exodus issues
```

Reference scope:

```text
Pre-#1078 parent or baseline issues that later Phase 2 work depends on.
```

Important reference issues:

```text
#882   overall Galactic Exodus plan / integration branch
#902   Phase 0 fixed map and rift route model
#1040  Phase 0 initial recommended values
#1049  Phase 1 implementation tracker
#1059  Phase 1C TBX migration specification decision
#1073  Phase 1B rift edge constraint
#1076  Phase 2 display baseline
```

Non-scope for this audit:

```text
- gameplay implementation changes
- balance changes
- fixture or snapshot regeneration
- full migration of issue text into specs
- final docs/specs placement rule; see #1259
```

## Classification rules

Issue classification:

| Class | Meaning |
|---|---|
| A | Specification decision issue |
| B | Implementation issue |
| C | Investigation / management / evaluation issue |
| D | Obsolete / not planned / consolidated issue |
| E | Outside this audit |

Reflection status:

| Status | Meaning |
|---|---|
| `implemented` | Reflected in the necessary code / tests / fixtures / docs for the current prototype scope. |
| `partial` | Reflected in some places, but either source-of-truth docs are missing or some implementation surface is intentionally deferred. |
| `missing` | Decision exists in an issue, but no repository reflection was found in this audit. |
| `obsolete` | Replaced by a later issue or consolidated into a newer source. |
| `needs_decision` | It is unclear whether the item is a specification decision or an implementation note. |

## Issue inventory

This inventory focuses on Galactic Exodus issues at and after #1078 plus dependent baselines. It intentionally marks pure implementation issues as implementation, not as specification sources.

| Issue | Class | Role in audit | Notes |
|---:|---|---|---|
| #1076 | A | Display baseline reference | Fixed LRS/SRS/HUD/log baseline, using `docs/design/galactic_exodus_display_samples.md` as the comparison input. |
| #1078 | C | Phase 2 SRS exploration parent | Tracker / umbrella for SRS exploration model. Child issues contain concrete decisions. |
| #1079 | C | Phase 2A initial model/evaluation setup | Initial hypotheses and evaluation setup; later refined by #1085/#1086/#1087/#1088/#1089 and #1083. |
| #1080 | B | SRS prototype implementation | Implementation carrier for the Phase 2A model. |
| #1081 | C | Manual evaluation | Evaluation issue; not a stable spec source except findings referenced by follow-ups. |
| #1082 | C | Agent evaluation | Evaluation issue; not a stable spec source except findings referenced by follow-ups. |
| #1083 | A | SRS movement/exploration integration decision | Consolidates manual/agent evaluation into SRS movement/exploration rules. |
| #1085 | A | SRS element taxonomy | Defines SectorType / terrain / feature / object / actor split and required attributes. |
| #1086 | A | SRS terrain effects | Concrete movement cost, passability, observation range, and terrain/object compatibility. |
| #1087 | A | SRS object state / persistence | Object interaction and post-use state rules. |
| #1088 | A | SRS map generation and WARP | Terrain count profile, warp_flags, 2x2 FLOOR cluster rule, RIFT barrier rules. |
| #1089 | A | SRS movement command details | Movement command resolution / turn handling; should remain linked from SRS movement docs. |
| #1130 | B | SRS input tolerance / restart | Implementation follow-up from manual evaluation. |
| #1132 | B | Player/object overlap display | Implementation/display fix, not an independent spec source. |
| #1134 | B | Interaction event summary detail | Event wording/detail implementation. |
| #1136 | B | Fixture initial visible cells | Fixture/test alignment. |
| #1137 | C/D | SHARED_FUEL detail not fixed in #1082 | Managed by later fixture/regression/balance decisions. |
| #1138 | B | SRS fixture regression tests | Test coverage issue. |
| #1178 | C/A | Combat/encounter/SALVAGE management | Management issue that declares #1194 as current source for combat initial parameters and records consolidated decisions. |
| #1179 | D | Former enemy/threat model issue | Not planned / consolidated into #1194 and #1178. |
| #1180 | D | Former spawn/warp/terrain modifier issue | Not planned / consolidated into #1194 and #1178. |
| #1181 | D | Former chase-pressure issue | Not planned / consolidated into #1194 and #1178. |
| #1182 | D | Former enemy detection / warp restriction issue | Not planned / consolidated into #1194 and #1178. |
| #1183 | D | Former combat rules issue | Not planned / consolidated into #1194 and #1178; may contain obsolete distance-decay text. |
| #1184 | D | Former weapon/ammo/energy issue | Not planned / consolidated into #1194 and #1178; may contain obsolete distance-decay text. |
| #1185 | D | Former SALVAGE effect issue | Not planned / consolidated into #1194 and #1178. |
| #1186 | D | Former enemy AI / progression issue | Not planned / consolidated into #1194 and #1178. |
| #1187-#1193 | D | Former simulation decomposition | Superseded for now by #1194. |
| #1194 | A | Combat/encounter initial parameters | Current source for SRS combat, encounter chance, weapon stats, enemy tier, enemy action, and spawn composition initial model. |
| #1214 | C/A | Display sample creation | Produced `docs/design/galactic_exodus_display_samples.md`, used as #1076 input. |
| #1218 | A | SRS coordinate policy | Fixed internal 0-origin lower-left and display 1-origin lower-left. |
| #1220-#1223 | B | Coordinate policy implementation | Fixture / validator / tests / render alignment for #1218. |
| #1230 | C | Display implementation impact audit | Investigation issue for implementing #1076 display baseline. |
| #1231 | B | LRS border-light renderer | Implementation of #1076 LRS baseline. |
| #1232 | B | SRS display renderer | Implementation of #1076 SRS baseline. |
| #1233 | B | Compact HUD | Implementation of #1076 HUD baseline. |
| #1234 | B | Log/debug event wording | Implementation of #1076 wording baseline. |
| #1235 | B | Display snapshot / fixture | Regression coverage for #1076 display baseline. |
| #1241 | A | Integrated CLI command-response / EXIT decision | Source for command loop, response panel order, parser normalization, and EXIT-driven LRS movement. |
| #1242 | B | Integrated CLI shell | Implementation of #1241 command-response skeleton. |
| #1243 | B | SRS movement command connection | Implementation of #1241 movement command mapping. |
| #1244 | B | EXIT / LRS move connection | Implementation of #1241 EXIT transition. |
| #1245 | B | INTERACT command connection | Implementation of #1241 interaction command path. |
| #1250 | B | readline / stdin decode resilience | CLI robustness implementation. |
| #1252 | B | Initial SRS player display=(1,1) | Targeted implementation decision for initial integrated CLI placement. |
| #1254 | B | `srs/generate.py` warp_flags sync | Implementation correction for #1088 WARP rule. |
| #1255 | B | `integrated_play.py` minimal SRS warp_flags sync | Implementation correction for #1088 WARP rule. |
| #1257 | C | Encounter spawn / combat balance recheck | Open follow-up candidate after #1254/#1256; should consume #1178/#1194. |
| #1259 | C/A | Spec source-of-truth placement and operations | Defines future docs/specs layout and update process. |
| #1260 | C | This audit | Creates this traceability matrix. |

## Traceability matrix

| Spec area | Source issue | Decision summary | Expected repo reflection | Current reflection | Status | Gap / action |
|---|---:|---|---|---|---|---|
| Phase 2 display baseline | #1076 | Use border-light LRS macro map, borderless north-to-south SRS map, compact HUD, one-line last event, debug/log split, ASCII fallback. | `docs/design/galactic_exodus_display_samples.md`, LRS/SRS renderers, HUD, event formatting, display snapshots. | `docs/design/galactic_exodus_display_samples.md`; #1231 LRS renderer; #1232 SRS renderer; #1233 compact HUD; #1234 event wording; #1235 snapshots. | `implemented` | When #1259 lands, add/mirror canonical display summary under `docs/specs/galactic_exodus/display.md`. |
| SRS coordinate contract | #1218 | Engine / fixtures / validator / raw payload use internal 0-origin lower-left; render / manual eval / HUD / docs use display 1-origin lower-left. | SRS model/tests/fixtures/render/manual docs use the correct coordinate layer. | #1220-#1223 implemented coordinate conversion and render/display alignment; #1076 references the policy. | `implemented` | Add a short canonical note in future display spec so new code does not reintroduce upper-left or 0-origin display assumptions. |
| SRS element taxonomy | #1085/#1086 | Split SectorType, terrain, feature/object/actor concepts; terrain passability/move cost/observation behavior; no generic WALL in current terrain set. | `phase2_srs_elements.md`, JSON, validator, tests, model enums, engine movement/observation. | `experiments/galactic_exodus/srs/phase2_srs_elements.md`; `phase2_srs_elements.json`; validator/tests; model enums include current SectorType/Terrain/Object/Actor. | `implemented` | The doc still mentions older `WARP_POINT` terminology in places. Flag for cleanup when #1259 spec docs are created. |
| SRS object state and interaction | #1085/#1087 | STATION adjacent interaction, reusable; RESOURCE_CACHE and SALVAGE same-cell interaction, consumed/collected then removed; STAR/PLANET static impassable; use/collect consumes one SRS turn; fuel-full no-op for station/cache. | SRS model/object states, interaction engine, fixtures/tests, event formatter. | Object types and states exist in `srs/model.py`; interaction was connected to integrated CLI by #1245; #1085 comment records #1087 decisions. | `partial` | Need a canonical spec file for object lifecycle; verify fuel-full no-op and turn consumption remain covered by tests after spec docs are added. |
| SRS WARP flags | #1088 | Deprecated `WARP_POINT`, fixed edge-center, Feature warp point, and WarpZone; each FLOOR cell may hold direction-specific `warp_flags`; edge cells get a flag if they are part of an edge-adjacent 2x2 FLOOR cluster; corners may have two directions. | `srs/generate.py`, `srs/test_generate.py`, `integrated_play.py`, `test_integrated_play.py`, render/HUD/docs. | #1254 updated `srs/generate.py`; #1255 updated minimal integrated SRS; tests were added/updated in their PRs. | `partial` | Create canonical `srs_warp.md` under #1259. Also update older docs that still say `WARP_POINT` where they are meant to describe current behavior. |
| RIFT edge / RIFT_BARRIER mapping | #1088 | RIFT blocked edges forbid the corresponding warp flag and place RIFT_BARRIER; non-blocked edges use normal 2x2 FLOOR warp flag rule; galaxy exterior directions forbid warp flags. | SRS generator, RIFT fixtures/tests, LRS EXIT validation, renderer/HUD wording. | `srs/generate.py` explicitly skips `descriptor.blocked_edges` for warp flags and applies `RIFT_BARRIER`; integrated CLI rejects blocked/out-of-bounds exits. | `partial` | `create_sector()` currently treats all non-blocked descriptor directions as open because the descriptor lacks board-bound context; document this limitation and resolve when LRS descriptor is integrated. |
| SRS terrain density / generation profile | #1088 | Replace `obstacle_density` with sector-specific terrain count ranges and limits; FLOOR is residual; passability/terrain counts depend on SectorType and map size. | Generator, generation contracts/fixtures, validator/tests, generation notes/spec. | #1088 decision exists in issue comments; current `srs/generate.py` is still a minimal all-floor-plus-barrier generator for many paths. | `partial` | Follow-up needed to either implement full terrain-count profile or explicitly mark it deferred in canonical `srs_map_generation.md`. |
| SRS movement / exploration rules | #1083/#1089 | SRS movement/exploration rules from manual/agent evaluation, including movement command resolution, observation update, cost model, and revisit persistence. | SRS engine, fixtures, regression tests, docs. | SRS engine/tests/fixtures exist; #1138 added fixture regression; `phase2_srs_elements.md` records observation and movement-related terrain effects. | `partial` | Add canonical `srs_movement.md` or include this in `srs_map_generation.md` / `integrated_cli.md`; audit #1083/#1089 exact final decision text before migrating. |
| Combat initial player/enemy stats | #1194/#1178 | Player durability 100, movement_power 4, photon torpedo ammo 6, energy 6, recovery 1; enemy movement_power 3; torpedo damage/range 3/3; phaser damage/range 1/2; enemy tier stats T1=3/6, T2=5/7, T3=8/8, T4=12/10. | `srs/model.py`, combat tests, HUD. | `srs/model.py` reflects player defaults, weapon profiles, enemy tier defaults, and enemy movement_power. | `implemented` | Add canonical `srs_combat.md` and keep #1178 as index/management rather than source of full spec. |
| Encounter rate and terrain modifier | #1194/#1178 | `T_srs_expected=4`; `E_base_per_lrs_step=0.75`; `base_encounter_chance_per_srs_turn=0.18`; NEBULA terrain modifier 0.7; other terrain 1.0. | Encounter module and tests; balance notes. | `srs/encounter.py` defines `EXPECTED_SRS_TURNS=4`, `ENCOUNTERS_PER_LRS_STEP=0.75`, `BASE_ENCOUNTER_CHANCE_PER_SRS_TURN=0.18`, and NEBULA modifier 0.7. | `implemented` | Add canonical `srs_encounter.md`; keep #1257 as recheck follow-up after WARP/spawn changes. |
| Encounter group budget / tier composition | #1178/#1194 | Danger-score budget ranges and fixed tier composition table; spawn cap keeps strongest enemies but action array is sorted weak-to-strong. | Encounter module and tests. | `srs/encounter.py` contains group costs, budget ranges, composition table, spawn cap, and tier sort order. | `implemented` | Verify fixture coverage for spawn-cap truncation and action-order sort after #1257. |
| Enemy spawn candidate points | #1178/#1194 | Spawn from passable warp points excluding player cell and eight neighbors; no spawn when enemy_presence is true; no additional spawn during combat. | Encounter module, engine turn advancement tests. | `srs/encounter.py` computes candidates from warp positions and excludes the 3x3 area around player. | `partial` | Confirm engine-side roll suppression and no-additional-spawn behavior have explicit tests; record in `srs_encounter.md`. |
| Enemy action model | #1194/#1178 | Enemy moves toward an attack position if it cannot attack; attacks if it can; enemy range equals phaser range 2; destroyed enemy does not counterattack. | Combat/engine implementation and tests. | Combat stats are in `srs/model.py`; this audit did not find a separate `srs/combat.py` file. | `partial` | Follow-up audit needed for exact enemy action implementation surface; create/update combat tests if missing. |
| SALVAGE combat/resource effects | #1178/#1194 and #1185 consolidated | SALVAGE inventory and recovery/upgrade choices exist as model concepts; exact application timing was intentionally not fixed in older subissues. | Model, interaction, combat/resource recovery tests, future base upgrade docs. | `SrsSalvageChoice` and `SrsBaseUpgrade` enums exist in `srs/model.py`; exact lifecycle/effect spec remains less visible than encounter/combat constants. | `needs_decision` | Create follow-up to decide whether current SALVAGE behavior is fixed spec or still prototype-only. |
| Integrated CLI command-response loop | #1241 | Single `COMMAND>` loop; parse/execute/render; output order `RESULT`, `LRS`, `SRS`, `HUD`, optional `LOG`; LRS/SRS are response panels, not input modes. | `integrated_play.py`, `test_integrated_play.py`, README. | #1242 added skeleton; tests assert startup sections and parser behavior; #1250 added stdin resilience. | `implemented` | Add canonical `integrated_cli.md` after #1259. |
| Integrated CLI movement commands | #1241/#1243 | `N/E/S/W` and `MOVE ...` move inside SRS only; direct direction command does not change LRS position. | `integrated_play.py`, tests. | #1243 connected SRS movement; tests verify direction command changes only SRS. | `implemented` | Include examples in `integrated_cli.md`. |
| Integrated CLI EXIT command | #1241/#1244/#1255 | Only `EXIT <dir>` changes LRS position; requires current SRS cell to have matching warp flag, board destination in bounds, no known blocked RIFT, and no blocking combat/fuel constraint. | `integrated_play.py`, `test_integrated_play.py`, LRS engine. | #1244 connected EXIT; #1255 synced minimal SRS warp flags; #1252/#1255 cover lower-left out-of-bounds rejection. | `partial` | Combat/fuel constraints are described as future/needed constraints; verify or document if they are currently non-scope in minimal CLI. |
| Integrated CLI INTERACT | #1241/#1245 | `INTERACT` executes object interaction on current SRS cell / applicable object and returns command result without changing parser model. | `integrated_play.py`, tests, SRS interaction engine. | #1245 implemented connection and minimal SRS object placement for RESOURCE_CACHE/STATION. | `implemented` | Add integrated CLI spec examples for no-target, cache, station, and salvage once canonical docs are added. |
| Initial SRS player position | #1252 | New integrated CLI game starts at internal=(0,0), display=(1,1); EXIT entry points after LRS movement remain unchanged. | `integrated_play.py`, `test_integrated_play.py`, display/HUD snapshots. | #1252 closed; tests assert `Position(0,0)` and lower-left discovered window. | `implemented` | No follow-up unless future generation replaces minimal SRS start placement. |
| Readline / stdin decode resilience | #1250 | CLI should tolerate readline absence and stdin decode errors without traceback. | `integrated_play.py`, tests. | Tests cover decode error ending session without traceback. | `implemented` | No spec action needed beyond integrated CLI operations note. |
| Spec source-of-truth operation | #1259 | Future source-of-truth should live under `docs/specs/galactic_exodus/`; issue decisions must be reflected in docs, not left only in comments. | New docs/specs layout and README. | #1259 is open at this audit time. | `partial` | This audit should feed #1259. Do not treat this file as final source-of-truth location if #1259 chooses another path. |

## Gaps and follow-up candidates

| Priority | Gap | Suggested follow-up |
|---:|---|---|
| 1 | #1088 WARP decisions are now mostly implemented, but no canonical `srs_warp.md` exists and older docs still mention `WARP_POINT`. | Create `docs/specs/galactic_exodus/srs_warp.md` after #1259; update references from `WARP_POINT` to current `warp_flags` where appropriate. |
| 2 | #1088 terrain-count generation profile is not clearly reflected in the minimal generator. | Decide whether full terrain-count profile is deferred or implement it; record in `srs_map_generation.md`. |
| 3 | `create_sector()` cannot know galaxy exterior directions; it treats non-blocked descriptor directions as open. | Add LRS board-bound context to the SRS descriptor path or document the limitation until full LRS/SRS generation integration. |
| 4 | Combat constants are reflected, but enemy action flow and destroyed-enemy counterattack behavior need a clearer implementation/test trace. | Audit engine/combat tests and create a focused follow-up if coverage is missing. |
| 5 | SALVAGE effect/timing remains less fixed than combat and encounter constants. | Create a decision issue or fold into `srs_combat.md` / `balance.md` after #1259. |
| 6 | #1083/#1089 final SRS movement/exploration decisions need a direct canonical doc. | Create `srs_movement.md` or fold into `integrated_cli.md` plus SRS engine spec. |
| 7 | Integrated CLI EXIT spec mentions fuel/combat constraints, but minimal CLI may not implement all of them. | In `integrated_cli.md`, mark current constraints as implemented vs deferred. |

## Audit notes

- Code search alone was not used as the source of truth because missing specs are invisible to code search.
- #1179-#1186 are not considered active source issues; #1178 explicitly points to #1194 as the current source for combat initial parameters.
- #1254/#1255 are implementation corrections caused by #1088 decisions remaining only in issue comments for too long. Their existence is evidence that #1088 needs a canonical spec file.
- This file is intentionally a traceability matrix. It should not become the final full specification corpus.
