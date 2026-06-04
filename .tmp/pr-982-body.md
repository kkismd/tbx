## 概要

Closes #982

STTR1 の command loop に docking / condition refresh を接続し、navigation や他コマンドの後でも次の `COMMAND:` までに状態が更新されるようにしました。

## 変更内容

- `examples/trek/scan.tbx` に `REFRESH_DOCKING_AND_CONDITION()` を追加し、`SHORT_RANGE_SCAN()` の refresh ロジックを共通化
- `examples/trek/command.tbx` の `RUN_COMMAND_LOOP()` で、各コマンド後に state refresh を実行
- command 1 は `SHORT_RANGE_SCAN()` 自身が refresh を行うため、post-command refresh を抑止して二重実行を回避
- `examples/trek/test_scan.tbx` に dock / undock の helper regression を追加
- `tests/tbx_lib_tests.rs` に mock input 付きの command loop regression を追加

## 確認

- `for f in examples/trek/test_*.tbx; do cargo run --quiet -- "$f" || exit 1; done`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
