# problems.md

## 1. 既知の問題

### P-001: ~~Rust の既知制限 (`issue #100013`) による lifetime bound エラー~~ → 解消済み

- 症状:
  - 以前は `tokio::spawn(...)` に渡す非同期タスクで `lifetime bound not satisfied` が発生する場合があった
- 影響範囲:
  - `watch` モードのタスク起動経路（特に `start_poll_task` / `start_ws_task`）
- 現在の対応:
  - `start_poll_task` は `tokio::spawn` で直接起動する構成に整理済み
  - `start_ws_task` はタスク関数を分離して起動
- 検証:
  - `cargo check` で再発しないことを確認する

### P-002: `critical-section` 未解決シンボルによるリンク失敗

- 症状:
  - `Undefined symbols for architecture arm64: __critical_section_1_0_acquire/release`
- 原因:
  - 依存経路（`embassy-sync` など）で `critical-section` が必要だが、`std` 実装が有効でない構成だとリンク失敗する
- 現在の対応:
  - `Cargo.toml` で `critical-section = { version = "1.2.0", features = ["std"] }` を追加
- 検証:
  - `cargo check` / `cargo build` を連続実行して再発有無を確認する

## 2. 既知の警告

### W-001: ~~`SplashScreen` コンポーネント未使用警告~~ → 解消済み

- `SplashScreen`（旧 `Home`）は `App` の `splash` フィールドに起動スプラッシュとして組み込み済み。警告は発生しない。

### W-002: Phase 3 関連の未使用警告（残りうるもの）

- 症状（例）:
  - `create_payment_tx` や一時的なローカル変数が未使用、`#[allow(dead_code)]` 付きヘルパが残る、など
- 影響:
  - 動作には影響しないことが多い（署名・送信経路の途中実装）
- 対応:
  - 書き込み系 CLI/TUI を本番相当まで繋いだタイミングで整理する

## 3. 運用メモ

- macOS の `ld: ... built for newer 'macOS' version ...` は、現状は警告扱い
- 実障害の優先度は `P-001` と `P-002` が上位
- ビルド安定化の完了条件は、ローカル連続実行で `cargo check` が成功すること
