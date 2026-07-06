# Galactic Exodus specs

このディレクトリは、Galactic Exodus の仕様正本を置く場所である。

## 目的

Galactic Exodus では、issue comment、設計メモ、fixtures、snapshots、README、tests がそれぞれ異なる役割を持つ。
今後は、ゲームルールや実装判断の正本を `experiments/galactic_exodus/docs/specs/` に集約し、実装 issue は対応する spec file を `Source of truth` として参照する。

## 役割分担

| 場所 | 役割 |
|---|---|
| issue | 議論・決定ログ。仕様を確定してよいが、issueだけで完了扱いにしない。 |
| `experiments/galactic_exodus/docs/specs/` | 仕様正本。実装 issue が参照する source of truth。 |
| `docs/design/` | 表示案・比較サンプル・設計メモ。ゲームルール正本としては扱わない。 |
| fixtures / snapshots | 実装済み挙動の regression 固定。正本仕様と矛盾した場合は正本仕様を優先する。 |
| README | 実行方法・ユーザー向け説明。詳細仕様は experiments/galactic_exodus/docs/specs へリンクする。 |
| tests | 正本仕様または固定済み挙動の検証。 |

## 運用ルール

### 1. issue は議論・決定ログ

issue comment で仕様を確定してよい。ただし、確定した仕様は対応する spec file へ反映する。

### 2. experiments/galactic_exodus/docs/specs が正本

実装 issue では、原則として次の形で正本を明記する。

```text
Source of truth:
  experiments/galactic_exodus/docs/specs/<spec-file>.md

Decision issue:
  #xxxx

Implementation PR:
  #yyyy
```

### 3. docs/design は正本ではない

`docs/design/galactic_exodus_display_samples.md` のような文書は、表示案や比較サンプルとして扱う。
ゲームルールを再決定しない。

### 4. fixtures / snapshots は regression 固定

fixtures / snapshots / phase2_reference は、実装済み挙動の回帰固定である。
正本仕様と矛盾した場合は、正本仕様を優先し、fixture / snapshot 更新 issue を作る。

### 5. README は実行方法を優先する

README はユーザー向けの起動方法・現状機能説明を優先する。
詳細なルールや仕様は、このディレクトリの spec file へリンクする。

### 6. 実装 issue 作成前に spec file を確認する

新しい実装 issue を作る前に、次を確認する。

```text
- 対応する spec file があるか
- ない場合、先に spec file を作るか
- issue上の決定だけに依存していないか
- fixtures / tests が古い仕様を固定していないか
```

### 7. 仕様確定 issue の close 条件

仕様確定 issue は、次のどちらかを満たしてから close する。

```text
A. spec file に反映済み
B. spec file 反映用の後続 issue が作成済みで、そのリンクが本文またはコメントにある
```

## 初期の正本ファイル候補

#1259 時点では、この README のみを追加し、既存仕様本文の完全移植は後続 issue に分ける。

| Spec file | 主な入力 issue | 状態 |
|---|---|---|
| `srs_warp.md` | #1088, #1254, #1255 | #1262 で移植予定 |
| `srs_map_generation.md` | #1085, #1086, #1088 | #1263 / #1264 と連動 |
| `srs_movement.md` | #1083, #1089 | #1267 で移植予定 |
| `srs_encounter.md` | #1178, #1194 | 後続候補 |
| `srs_combat.md` | #1178, #1194 | 後続候補 |
| `integrated_cli.md` | #1241, #1242, #1243, #1244, #1245 | #1268 と連動 |
| `display.md` | #1076, #1214, #1218, #1230-#1235 | 後続候補 |
| `balance.md` | #1178, #1194, #1257 | 後続候補 |

## 現時点の移植優先度

#1260 の棚卸し結果に基づき、まず次を優先する。

```text
1. #1088 WARP仕様 -> srs_warp.md
2. #1088 terrain-count / generation profile -> srs_map_generation.md または実装follow-up
3. #1241 integrated CLI / EXIT -> integrated_cli.md
4. #1083/#1089 SRS movement / exploration -> srs_movement.md
5. #1178/#1194 encounter / combat / balance -> srs_encounter.md, srs_combat.md, balance.md
6. #1076 display baseline -> display.md
```

## 注意

この README は、仕様本文そのものではなく、正本配置と運用ルールの入口である。
各ゲームルールは個別の spec file に記録する。
