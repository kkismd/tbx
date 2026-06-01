# STTR1 Klingon Attack Tracker Notes

`issue #917` で整理した、`#893` (Klingon retaliation / attack tracker) の棚卸しメモ。

## `#893` で完了した範囲

- deterministic な Klingon attack helper
  - `KLINGON_ATTACK_AMOUNT()`
  - `KLINGON_ATTACK_AMOUNT_FOR_INDEX_AND_ROLL()`
- `KLINGON_ATTACK()` core の safe boundary
  - docked 時は Starbase protection を表示して return
  - `KLINGONS_HERE <= 0` なら no-op
  - dead / unused Klingon slot を安全に無視
- phaser command 後の retaliation 接続
  - `FIRE_PHASERS_WITH_UNITS_AND_ROLL()`
  - `MAYBE_KLINGON_ATTACK_AFTER_PLAYER_ACTION_WITH_ROLL()`
- shield underflow defeat hook
  - retaliation で `SHIELDS` を減算
  - `SHIELDS < 0` で `GAME_OVER = TRUE`
  - 現状の defeat message は `"THE ENTERPRISE HAS BEEN DESTROYED"`

この時点で、最小戦闘ループは次まで接続済み。

```text
command 3 = FIRE PHASERS
  ↓
Klingon に hit / destroy
  ↓
生存 Klingon がいれば retaliation
  ↓
SHIELDS 減少
  ↓
SHIELDS < 0 なら GAME_OVER
```

## `#893` に残さないもの

以下は `#893` の完了条件には含めず、後続 issue へ分離する。

### Damage system foundation

- `@DAMAGE[1..8]` の意味づけ整理
- device damage 発生条件
- damage control report command
- repair / dock repair
- phaser computer penalty の実処理

### Shield / retaliation expansion

- shield control command
- shield energy transfer
- docked 時の shield / energy 回復境界
- navigation 後の Klingon attack
- photon torpedo 後の Klingon attack
- shield command 後の Klingon attack
- shield collapse 後の表示整理
- Starbase protection message の精密化

### Full endgame / score

- defeat report の原典寄せ
- victory report
- final score
- mission summary
- `GAME_OVER` 後の quit / endgame flow 完成

## `command 3` の確認観点

`integration/470-sttr1-porting` 上では、`command 3 = FIRE PHASERS` の確認を次の 3 層に分ける。

1. dispatch smoke
   - `examples/trek/test_command.tbx`
   - `DISPATCH_COMMAND(3)` が通常起動経路から `PHASER_CONTROL()` を呼んでも落ちないこと
   - stdin なし実行では `READ_NUMBER_OR(0)` が `0` になり safe return する
2. valid fire path
   - `examples/trek/test_phaser.tbx`
   - `TRY_FIRE_PHASERS_UNITS()` と `FIRE_PHASERS_WITH_UNITS_AND_ROLL()` で command 3 の実処理を確認する
3. retaliation / defeat path
   - `examples/trek/test_phaser.tbx`
   - retaliation 発生条件、last Klingon destroy の no retaliation、shield underflow の `GAME_OVER` を確認する

この分割により、入出力付き command path と deterministic combat core を分離したまま、通常起動の確認観点を維持する。

## `#893` の完了判断

`#893` は「Klingon retaliation / attack の最小戦闘ループを成立させる tracker」としては完了扱いでよい。

根拠:

- deterministic helper test がある
- `KLINGON_ATTACK()` core の safe boundary がある
- phaser 後 retaliation が接続済み
- shield underflow defeat hook が接続済み
- command `3` の dispatch smoke と combat regression の両方がある

未完了なのは、damage system / shield control / full endgame の次段階であり、`#893` とは別 tracker に切り出すほうが粒度がよい。

## 次に切る issue 候補

1. STTR1: damage system foundation を実装する
2. STTR1: shield control と retaliation 接続拡張を実装する
3. STTR1: full endgame / score を実装する
