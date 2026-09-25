# M31 静的実行イメージ評価

## 評価基点

評価開始時の `main` は **`d385ecc6c68ad26e5fab24e6dc29697a136ada49`**（2026-09-25）だった。

- #1840 / PR #2007: runtime instruction graph から `StaticImage` への lowering
- #1841 / PR #2008: `StaticImage` 用 `ReferenceVm`
- #1995: return position、call 時 data stack depth、call 時 control-value stack depth を保持する復帰契約
- #2010: 実sourceのcompile、host / PoC実行、静的統計をつなぐ調査用 harness（評価開始時 CLOSED）

対象は `crates/tbx-next` の既存 test-only harness が同じsource compile結果から得るlogical imageである。対象sourceは `prime.tbx`、`mandelbrot.tbx`、`grades.tbx`。host / PoC実行一致を必須確認するのは `prime.tbx` のみとした。

## 測定方法と対象外

`static_image_harness` を使い、embedded standard libraryを先にcompileし、その後にsourceをcompileした。lowering owner順はtemporary execution unit、published codeで、entryは `CodePosition(0)`。命令variant件数、relocation件数、UTF-8 fixed text bytes / slots、global slots、array lengthsは #2010 の `TestImageStatistics` から取得した。

`prime.tbx` はhost VMとReferenceVmのstdout、outcome、終了時data stackを比較した。最大stack depth、stack RAM、global / arrayの最終snapshot、descriptor/linker overhead、runtime buffer、RNG state、target runtime ROM、cycle count、overflow閾値は計測していない。`guess.tbx`、STTR1、BREAK fixtureも評価対象に加えていない。

## prime の実行一致

| 観測値 | host VM | PoC ReferenceVm |
| --- | --- | --- |
| stdout | `2\n3\n5\n7\n11\n13\n17\n19\n23\n29\n` | 同一 |
| outcome | `Halted` | halted |
| 終了時 data stack | `[]` | `[]` |

既存 harness の `prime_source_matches_host_execution_and_reports_static_image` がこの一致をassertする。mandelbrot / gradesはcompile、lower、PoC実行を通し、hostとの完全比較は行っていない。

## Logical image 統計

| 入力 | logical命令数 | primitive call | compiled call | branch* | fixed text bytes | globals | arrays | array要素 | relocation | A bytes | B bytes | C bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| prime | 51 | 12 | 1 | 8 | 0 | 3 | 0 | 0 | 9 | 119 | 107 | 176 |
| mandelbrot | 230 | 55 | 5 | 20 | 0 | 27 | 0 | 0 | 25 | 518 | 463 | 806 |
| grades | 101 | 19 | 0 | 13 | 38 | 4 | 1 | 6 | 13 | 228 | 209 | 344 |

\* branch は `Jump + JumpIfZero`。array要素は各宣言長の合計。relocationは `CallCode + Jump + JumpIfZero`。

17 variantの全件数:

| LogicalInstruction | prime | mandelbrot | grades |
| --- | ---: | ---: | ---: |
| PushI16 | 10 | 35 | 24 |
| WriteText | 0 | 0 | 7 |
| LoadGlobal | 9 | 66 | 14 |
| StoreGlobal | 6 | 43 | 6 |
| LoadArray | 0 | 0 | 1 |
| StoreArray | 0 | 0 | 6 |
| CallPrimitive | 12 | 55 | 19 |
| CallCode | 1 | 5 | 0 |
| CopyCallBase | 3 | 4 | 0 |
| TruncateCallBase | 0 | 0 | 0 |
| ControlPush | 0 | 0 | 2 |
| ControlCopy | 0 | 0 | 6 |
| ControlDrop | 0 | 0 | 2 |
| Jump | 3 | 8 | 6 |
| JumpIfZero | 5 | 12 | 7 |
| Return | 1 | 1 | 0 |
| Halt | 1 | 1 | 1 |
| **合計** | **51** | **230** | **101** |

## Code size候補と算定

以下は同じlogical instruction件数へ単純な幅を掛けた比較用の仮定であり、採用するformatの決定ではない。各operand幅は候補上の固定幅とし、alignment / padding、header、linker metadata、未参照データ、runtime codeは含めない。instruction間に可変長最適化もしない。

| Variant | A: opcode + operand bytecode | B: primitive短縮 bytecode | C: 16-bit threaded-like |
| --- | ---: | ---: | ---: |
| PushI16 | 3 (1 opcode + 2 immediate) | 3 | 4 (2 token + 2 immediate) |
| WriteText | 2 (1 + 1 slot) | 2 | 4 (2 + 2 slot) |
| LoadGlobal / StoreGlobal | 2 (1 + 1 slot) | 2 | 4 (2 + 2 slot) |
| LoadArray / StoreArray | 2 (1 + 1 slot) | 2 | 4 (2 + 2 slot) |
| CallPrimitive | 2 (1 opcode + 1 primitive ID) | 1 dedicated opcode | 2 (one token) |
| CallCode | 3 (1 + 2 position) | 3 | 4 (2 + 2 position) |
| CopyCallBase | 2 (1 + 1 offset) | 2 | 4 (2 + 2 offset) |
| TruncateCallBase / ControlPush / ControlCopy / ControlDrop / Return / Halt | 1 each | 1 each | 2 each (one token) |
| Jump / JumpIfZero | 3 (1 + 2 position) | 3 | 4 (2 + 2 position) |

A assumes 1-byte opcode, 8-bit primitive ID / slot / call-base offset and 16-bit code position / immediate. B uses the same widths except every primitive call has a distinct 1-byte opcode, avoiding its explicit primitive ID byte. This assumes the opcode space can accommodate those primitive forms. C represents each operation by a 16-bit token; values and references that need an operand add one 16-bit word. All candidates ignore alignment and external metadata.

The resulting code bytes are calculated per instruction as `sum(variant count × assumed bytes)`. No size is inferred from Rust type layout. The totals omit fixed text payload, which is reported separately above.

## Static / mutable data規模

| 入力 | fixed text payload | global scalar payload (2 B/cell) | array payload (2 B/element) | mutable payload概算 |
| --- | ---: | ---: | ---: | ---: |
| prime | 0 B / 0 slots | 6 B (3 slots) | 0 B | 6 B |
| mandelbrot | 0 B / 0 slots | 54 B (27 slots) | 0 B | 54 B |
| grades | 38 B / 7 slots | 8 B (4 slots) | 12 B (6 elements) | 20 B |

2 bytes per scalar / array element follows the language `Value = i16` as an evaluation assumption. The mutable payload is only the scalar and element contents; array descriptors, allocation tables, alignment, and other runtime state are excluded. The fixed text total is UTF-8 payload only and excludes descriptors or terminators.

## 未計測resourceと理由

| 項目 | 扱い | 理由 |
| --- | --- | --- |
| 最大data / control-value / return stack depth | 未計測 | harnessはpeak observerを持たない。少数source実行の観測を一般的上限と混同しない。 |
| stack RAM総量 | 未計測 | peak depthとprofile容量がなく換算できない。 |
| return frameの論理要素 | 定性的記録 | #1995によりreturn position、call時data stack depth、call時control-value stack depthが必要。最大depthを測っていないのでframe RAMは算出しない。 |
| descriptor / table、linker overhead | 未計測 | target形式が未決定で、host Rust layoutをABIとして使えない。 |
| runtime I/O buffer / RNG state | 未計測 | 対象sourceのstatic resource観測に含まれず、RNDも実行比較しない。 |
| target runtime ROM、cycle count | 未計測 | 実target runtime / encoderがない。logical imageからは導けない。 |
| resource overflow閾値 | 未計測 | candidate profile validatorはなく、本issueで新設もしない。 |

## 旧 `docs/tbx-6502-profile.md` の継承

引き続き有効な調査上の前提は、host compile後のtarget runtime、16-bit cellの大きさ、文字列を静的payloadとして分離する考え方、配列を連続したcell payloadとして数える考え方である。今回の2 B/cell換算は規模比較の仮定としてのみ使う。

旧profileはTBX Nextの仕様ではない。特に、旧文書のwrap arithmetic、targetでのtrap詳細、`Value` 等のhost identityを持ち込まないという境界、初期6502向け機能制限、indirect threading採用をNextへ自動継承しない。Next PoCの前提はsigned i16 / checked arithmetic、early bindingと `StaticImage` lowering、#1995のRETURN契約であり、物理encodingやtarget ABIは未決定のままにする。

## 次段階の判断材料

- この3入力ではA/B仮定のcode size差は12 B（prime）、55 B（mandelbrot）、19 B（grades）。primitive短縮の効果はprimitive call数に比例する。これはbyte幅仮定の差であり、実targetでの速度・ROM総量差を示さない。
- Cの2-byte token形式はこの仮定で最小にならず、operandを含むcode bytesはAより大きい。一方、dispatcherやlinkerの実装費用は比較していない。
- gradesはarray payloadが12 B、global payloadが8 B、fixed textが38 B。textとmutable dataを別領域として検討する根拠になる。
- 具体的な次の判断はexecution format、address / slot幅、stack容量、descriptor形式とlinker overhead。これらが候補選定に必要と判明した時点で、別ADRまたは測定issueに分ける。
- INPUT / RND / `USE`、STTR1、stack peak、exact total RAM、resource limits、cycle performanceは今回の結果から結論できない。

この評価は最終execution formatを選定せず、測定していない値をゼロや確定値として扱わない。
