# Galactic Exodus Phase 1B integrated playtest report

## 1. Scope and versions

This report integrates Phase 1B1 manual evaluation (#1067) and Phase 1B2 deterministic policy evaluation (#1068) for the Python prototype before TBX implementation.

- Base branch: `integration/882-galactic-exodus`
- GameLog schema: version 3
- Manual data: 10 sessions, requested seeds 1..10
- Automated data: 2 deterministic policies x requested seeds 1..1000 = 2000 runs
- Turn limit: 256
- Fuel: `initial_fuel=max_fuel=16`
- Supply: reusable immediate B refuel to maximum; each R coordinate supplies at most +5 once
- Observation: cumulative 3x3 disclosure at start and after successful movement; rifts are learned only by failed traversal
- Rift generation: after #1073/#1074, every rift edge touches at least one plain `.` sector

The automated policies are deliberately simple and are not optimal play. Their loss rates are comparative baselines, not direct estimates of human success.

## 2. Reproduction commands

```bash
python -m unittest discover -s experiments/galactic_exodus -p 'test_*.py'

python experiments/galactic_exodus/evaluate_policies.py \
  --seed-start 1 \
  --seed-end 1000 \
  --max-turns 256 \
  --output experiments/galactic_exodus/results/prototype_policy_runs.csv \
  --summary experiments/galactic_exodus/results/prototype_policy_summary.json

python experiments/galactic_exodus/validate_phase1b_results.py \
  --manual experiments/galactic_exodus/results/prototype_manual_sessions.csv \
  --runs experiments/galactic_exodus/results/prototype_policy_runs.csv \
  --summary experiments/galactic_exodus/results/prototype_policy_summary.json \
  --findings experiments/galactic_exodus/results/prototype_findings.csv

cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

For deterministic-output confirmation, run the policy evaluator a second time to temporary files and compare both outputs with `diff -u`.

## 3. Manual play results

### Objective results

- Sessions: 10
- Wins: 9
- Fuel-loss defeats: 1 (seed 6)
- Turn counts: 14..32
- Remaining fuel on wins: 4..13
- B visits/refuels: 14/14 total
- R visits/refuels: 12/10 total
- Rift attempts: 21 total

### Subjective score means

| Measure | Mean / 5 |
|---|---:|
| route decision | 4.6 |
| information | 4.3 |
| fuel tension | 4.2 |
| supply choice | 4.1 |
| rift fairness | 2.3 |
| readability | 2.6 |
| defeat clarity | 4.2 |
| observation range | 5.0 |
| resource reveal | 4.8 |
| rift asymmetry | 1.4 |
| base return value | 4.4 |
| base loop risk safety | 5.0 |

The strongest positive result is that the 3x3 cumulative observation model supports route decisions. The strongest negative result is presentation: discovered rifts and consumed R are difficult to recognize.

Notable sessions:

- Seed 2: a known rift blocked return to B; the player reported that the discovered rift was not reflected on the map and that R +5 felt smaller than expected.
- Seed 3: the player overlooked R and reported that alphabetic symbols can be missed at a glance.
- Seed 6: the only defeat; consumed R could not be distinguished from unused R, and rift/B/R state was difficult to read.
- Seeds 7, 9, and 10: rift display was repeatedly identified as unclear even in winning sessions.

## 4. Automated policy results

| Metric | GOAL_GREEDY | SUPPLY_AWARE |
|---|---:|---:|
| total runs | 1000 | 1000 |
| wins | 121 (0.121) | 154 (0.154) |
| lost fuel | 677 (0.677) | 623 (0.623) |
| turn limit | 1 (0.001) | 50 (0.050) |
| no policy action | 201 (0.201) | 173 (0.173) |
| median turns | 12 | 14 |
| p90 turns | 16 | 23 |
| median remaining fuel on wins | 2 | 4 |
| p90 remaining fuel on wins | 6 | 10 |
| B visit/refuel rate | 0.009 / 0.009 | 0.116 / 0.116 |
| multiple B refuel rate | 0.001 | 0.054 |
| R visit/refuel rate | 0.427 / 0.427 | 0.653 / 0.653 |
| multiple R refuel rate | 0.087 | 0.264 |
| no-supply win rate | 0.001 | 0.000 |
| rift-attempt rate | 0.720 | 0.722 |
| reroll rate | 0.012 | 0.012 |

SUPPLY_AWARE improves win rate by 3.3 percentage points, reduces fuel losses by 5.4 points, and uses R much more often. Its higher turn-limit rate reflects deterministic supply loops in a deliberately non-searching policy and is evidence that supply changes behavior, not that the rule is invalid.

## 5. Fixture verification

The combined test suite verifies:

- start and post-move 3x3 cumulative disclosure
- rifts not disclosed before failed traversal
- B first refuel, repeated refuel, and full-arrival behavior
- R first refuel, full-arrival non-consumption, used-R revisit, and separate-R independence
- generation error and turn limit outcomes
- policy access restricted to known state
- deterministic tie-breaking and output ordering
- non-progressing rejected actions terminate as `ABORTED_NO_POLICY_ACTION`
- unknown-rift failure changes state and does not terminate as no-progress
- output reproducibility for CSV and JSON

## 6. Answers to Q1..Q10

### Q1. Is fuel 16 too strict?

**Answer: retain 16 for Phase 1.** Manual play won 9/10 with fuel-tension mean 4.2/5, indicating pressure without routine human failure. Automated win rates are only 0.121/0.154, but 17.3%-20.1% of runs stop because the intentionally myopic policy cannot make progress, so those runs do not isolate fuel capacity. Seed 6 demonstrates that fuel loss is possible and meaningful, but its notes identify consumed-R readability as a direct contributor.

### Q2. Do B/R supplies affect route choice?

**Answer: yes, materially.** Manual supply-choice mean is 4.1/5, and seeds 6-9 explicitly discuss supply-related rerouting or value. SUPPLY_AWARE raises R refuel rate from 0.427 to 0.653 and win rate from 0.121 to 0.154. Supply must remain part of the macro decision model.

### Q3. Are three R sectors too many or too few?

**Answer: retain three.** Manual sessions visited R 12 times and refueled 10 times without reporting map-wide scarcity. Automated runs refuel at R in 42.7%/65.3% of runs and use multiple distinct R in 8.7%/26.4%. This is enough to influence routes without making every run supply-saturated.

### Q4. Is maximum +5 valuable?

**Answer: yes, but its value and state need clearer presentation.** Seeds 6-9 report R as useful, and SUPPLY_AWARE's higher R use accompanies a 3.3-point win improvement. Seed 2 felt +5 was less than expected, while seed 3 overlooked R. Keep +5 provisionally; improve display before numeric retuning.

### Q5. Does reusable B become a fixed solution?

**Answer: no.** Manual base-return-value mean is 4.4/5 and base-loop-risk safety is 5.0/5. Automated B refuel rates are only 0.009/0.116, and repeated B refuel rates are 0.001/0.054. Seed 6 used B three times but still lost, showing that return cost and route constraints prevent B from guaranteeing success.

### Q6. Is fuel -1 for an unknown rift failure appropriate?

**Answer: retain the penalty.** Rift-fairness mean is low at 2.3/5, but notes repeatedly target the missing persistent display rather than the numeric penalty. Automated runs encounter rifts frequently (about 72%), generally with one or two attempts. The design problem is communicating learned boundaries, not evidence of excessive -1 cost.

### Q7. Does 3x3 disclosure support route decisions?

**Answer: yes.** Route-decision mean is 4.6/5, information 4.3/5, observation-range 5.0/5, and resource-reveal 4.8/5. Automated median turns are 12/14 and GOAL_GREEDY almost never reaches the turn limit. Retain the observation radius. Separately, hidden-rift asymmetry is poorly understood (1.4/5) and requires explanation and persistent visualization.

### Q8. Are unlimited B and coordinate-local one-use R natural?

**Answer: yes.** No manual note rejects the lifecycle rule; the failure case concerns distinguishing consumed R. Automated repeated B use remains limited while multiple R use occurs under supply-aware play. Keep both rules and make consumption state explicit.

### Q9. Are defeat and display understandable?

**Answer: defeat is generally understandable; display is not.** Defeat-clarity mean is 4.2/5, while readability is only 2.6/5. Seed 6 lost after confusing consumed R with available R, and multiple seeds call out rift display. Keep outcome semantics, but require distinct used-R and discovered-rift representations.

### Q10. Is TBX state/display/log volume excessive?

**Answer: engine state and schema v3 are justified; the player-facing display should be smaller.** Automated validation depends on known routes, supply counters, used-resource positions, outcomes, and rerolls. Manual scores show enough information for decisions but poor readability. Preserve diagnostic state and logs, while designing a compact HUD/SRS presentation as a Phase 2 concern.

## 7. Findings

The canonical finding records are in `prototype_findings.csv`.

- BLOCKER: 0
- ADJUSTMENT: 3 (`P1B-004`, `P1B-008`, `P1B-010`)
- PHASE_2: 1 (`P1B-012`)
- NO_CHANGE: 8

## 8. Blockers

No Phase 1B blocker was identified. The prototype can proceed to #1059 after the user confirms that the manual interpretation and severities match the play experience.

## 9. Adjustments

1. Keep R +5 provisionally, but show its amount and consumed state explicitly (`P1B-004`).
2. Explain hidden-rift asymmetry and persist discovered blocked boundaries on the map (`P1B-008`).
3. Give unused R, consumed R, and discovered rifts unambiguous symbols, legend entries, and event feedback (`P1B-010`).

These are specification/presentation adjustments, not reasons to rerun the full evaluation before #1059.

## 10. Phase 2 candidates

- Separate the complete schema-v3 diagnostic/log state from a compact player HUD.
- Design SRS and macro-map presentation for terrain-flavored objects, consumed resources, and discovered rift boundaries.
- Consider a targeted R +5 versus alternative amount comparison only after the display is corrected.

## 11. Specifications to retain

- `initial_fuel=max_fuel=16`
- three R sectors
- R maximum +5, once per coordinate, consumed only when fuel increases
- B immediate unlimited refuel to maximum
- cumulative 3x3 observation
- rifts hidden until failed traversal
- unknown-rift failed traversal costs one fuel
- GameLog schema v3 and current objective counters
- deterministic requested/effective seed and reroll behavior

## 12. Handoff to #1059

#1059 should make final specification decisions on:

- the exact macro-map symbols and legend for discovered rifts and consumed R
- whether R +5 remains explicitly provisional or is frozen for TBX Phase 1
- how hidden-rift asymmetry is explained in player-facing rules
- which schema-v3 fields appear in the normal HUD versus logs/debug output

With zero blockers, #1059 may start after user review of this report and `prototype_findings.csv`. No replay is required.
