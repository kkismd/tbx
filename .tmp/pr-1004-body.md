## 概要

Closes #1004

tbx16 に colon word の metadata 解決、entry colon 開始、nested colon call、arity/local frame 管理、return frame push/pop、call depth 管理を追加しました。

## 変更内容

- `CODE_TOKEN_DOCOL` と colon metadata 解決を追加
- `run(entry_xt)` が colon word を直接開始できるように変更
- nested colon call 時に return stack へ `return IP` / `caller BP` の 2 Cell frame を原子的に push
- arity と local count に基づく `BP` / `DSP` 更新と local 領域の 0 初期化を追加
- `call_depth` と return stack 使用量の整合性チェックを追加
- entry / nested call の正常系、overflow、underflow、原子性を検証する `tbx16` テストを追加

## 確認

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
