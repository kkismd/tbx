# Galactic Exodus SRS encounter spawn balance notes

Source issue: #1257
Related: #1088, #1178, #1194, #1254, PR #1256
Base branch: `integration/882-galactic-exodus`

This document records the #1257 audit result for encounter spawn candidates and initial combat distance after the #1254 warp flag expansion. It is an analysis note, not a gameplay source of truth.

## 1. Scope

このメモは `spawn_candidate_points()` と `spawn_enemies_for_encounter()` の現行挙動を、deterministic なテストで確認した結果をまとめる。

対象は次に限定する。

- all `FLOOR` 9x9 での warp candidate 分布
- player が edge 近傍にいる場合の neighbor ring 除外
- `SectorType.RIFT` + blocked edge の candidate 除外
- fixed composition spawn の enemy id / tier / position
- representative fixture における初期 combat distance

この PR では gameplay constants、encounter chance、movement power、tier composition、damage、durability は変更しない。

## 2. Baseline after #1254

#1254 / PR #1256 により warp flag は「各辺中央4点」ではなく、外周 `FLOOR` と 2x2 `FLOOR` cluster 条件に基づく配置へ広がった。

その結果、all `FLOOR` 9x9 では外周全体が spawn candidate になりうる。candidate 数は増えたが、candidate 順序・spawn cap・tier 正規化の仕様は今回の PR では変更していない。

## 3. Candidate distribution checks

`test_encounter_balance.py` で確認した代表ケースは次の通り。

- all `FLOOR` 9x9 / player center (`Position(4, 4)`)
  - candidate count = 32
  - y_min = 9
  - y_max = 9
  - west = 7
  - east = 7
  - corners = 4
  - 全 candidate の Chebyshev 距離 = 4
- all `FLOOR` 9x9 / player edge-near (`Position(7, 4)`)
  - player の neighbor ring に入る `Position(8, 3)`, `Position(8, 4)`, `Position(8, 5)` は除外される
  - それ以外の外周 candidate は残る
  - candidate count = 29
  - 最短 Chebyshev 距離 = 2
- `SectorType.RIFT`, `blocked_edges = {N, W}`, player center
  - candidate count = 15
  - blocked edge 側の candidate は除外される
  - y_min = 8, y_max = 0, west = 0, east = 7, corners = 1
  - 残る candidate の Chebyshev 距離はすべて 4

`y_min` / `y_max` は座標ベースの集計であり、`Direction.N` / `Direction.S` とは意図的に切り分けている。west / east の件数は corner を別集計に分離しており、外周分布を二重計上しないための分析用 summary である。

## 4. Spawn result checks

固定 composition `(TIER2, TIER1, TIER1)` を all `FLOOR` 9x9 / center player に与えると、spawn 結果は次で安定する。

- `enemy-1`: `TIER1` at `Position(0, 0)`
- `enemy-2`: `TIER1` at `Position(1, 0)`
- `enemy-3`: `TIER2` at `Position(2, 0)`

つまり、

- enemy id 採番は deterministic
- tier は現行仕様通り昇順に正規化される
- position は candidate 順序に従って deterministic に選ばれる

既存 fixture `combat_encounter_spawn_cap_9x9.json` でも、最終位置は `Position(1, 0)`, `Position(2, 0)`, `Position(3, 0)` で固定される。

## 5. Distance / pressure assessment

center player の代表 9x9 ケースでは、all `FLOOR` / RIFT fixture のどちらでも spawn 直後の敵距離は Chebyshev 4 でそろっている。

これは次を意味する。

- #1254 後に candidate は増えた
- ただし player center ケースでは「外周多数 candidate = すぐ隣に spawn」にはなっていない
- player edge 近傍でも neighbor ring 除外により、少なくとも距離 1 の即時近接 spawn は抑制されている

現行の `player movement_power = 4`、`enemy movement_power = 3` 前提に対し、今回確認した representative ケースでは初期 distance pressure が直ちに破綻している証拠は見つからない。

## 6. Balance conclusion

Conclusion:

#1254 後の warp flag 増加により spawn candidate は増えたが、player neighbor ring 除外と外周 spawn により、代表 9x9 ケースでは初期距離は概ね維持されている。既存 #1194 / #1178 の初期バランス前提と即時に矛盾する evidence はない。数値調整は行わない。

## 7. Follow-up

今回の分析では follow-up issue が必要なほどの明確な balance break は確認できなかった。

non-scope を再掲する。

- `BASE_ENCOUNTER_CHANCE_PER_SRS_TURN` の変更
- `EXPECTED_SRS_TURNS` の変更
- `ENCOUNTERS_PER_LRS_STEP` の変更
- enemy / player `movement_power` の変更
- tier composition / group budget の変更
- combat damage / durability / defense / evasion の変更
- spawn candidate ordering の仕様変更
- spawn cap の仕様変更
- `integrated_play.py` への encounter / combat 接続
