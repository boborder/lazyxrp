# problems.md

## 既知の問題

### P-002: `critical-section` 未解決シンボルによるリンク失敗

- 症状: `Undefined symbols for architecture arm64: __critical_section_1_0_acquire/release`
- 原因: `embassy-sync` などの依存経路で `critical-section` が必要だが、`std` 実装が有効でない構成だとリンク失敗する
- 対応: `Cargo.toml` で `critical-section = { version = "1.2.0", features = ["std"] }` を追加
- 検証: `cargo check` / `cargo build` を連続実行して再発有無を確認する

### W-002: Phase 3 関連の未使用警告

- 症状: `create_payment_tx` や一時的なローカル変数が未使用、`#[allow(dead_code)]` 付きヘルパが残る、など
- 影響: 動作には影響しないことが多い（署名・送信経路の途中実装）
- 対応: 書き込み系 CLI/TUI を本番相当まで繋いだタイミングで整理する

## 運用メモ

- macOS の `ld: ... built for newer 'macOS' version ...` は、現状は警告扱い
- ビルド安定化の完了条件は、ローカル連続実行で `cargo check` が成功すること

## 解消済み（履歴）

- ~~P-001: Rust の既知制限 (`issue #100013`) による lifetime bound エラー~~ → `start_poll_task` / `start_ws_task` の構成整理で解消
- ~~W-001: `SplashScreen` コンポーネント未使用警告~~ → `App` の `splash` フィールドに組み込み済み
