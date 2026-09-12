# problems.md

**Role:** 注意点・落とし穴（P-xxx 既知問題と運用メモ）。

## 既知の問題

### P-002: `critical-section` 未解決シンボルによるリンク失敗

- 症状: `Undefined symbols for architecture arm64: __critical_section_1_0_acquire/release`
- 原因: `embassy-sync` などの依存経路で `critical-section` が必要だが、`std` 実装が有効でない構成だとリンク失敗する
- 対応: `Cargo.toml` で `critical-section = { version = "1.2.0", features = ["std"] }` を追加
- 注意: ソース内に直接の利用がなくても、この依存は `std` 実装の有効化に必要。未使用依存として削除しない
- 検証: `cargo check` / `cargo build` を連続実行して再発有無を確認する

### P-003: EscrowCreate composer/poll still unwired

- 症状: EscrowCreate の signing / poll コメントは残るが composer UI と poll 配線が未完了
- 対応: 配線時は `finalize_simulate_sign_submit` を再利用し、型固有バリデーションは caller に残す

### P-004: Submit validation split across layers

- 症状: Payment 検証が wallet UI / `poll.rs` / `signing.rs` に分散し、UI が valid でも submit が invalid になり得る
- 対応: 単一の validation 関数を wallet と poll で共有する（未実施）

### P-005: `config.rs` remains one file

- 症状: `src/config.rs` は約 1200 行（defaults / file / env / keybindings / styles / Flare network+display+wallet / paths / test helpers）
- 対応: 約 1500 行を超えたら `config/defaults.rs`, `config/keybindings.rs`, `config/styles.rs` に分割

## 運用メモ

- macOS の `ld: ... built for newer 'macOS' version ...` は、現状は警告扱い
- ビルド安定化の完了条件は、`cargo check` に加えて `cargo build --bins` が成功すること（リンクエラーは `check` だけでは検出できない）

## 解消済み（履歴）

- ~~P-001: Rust の既知制限 (`issue #100013`) による lifetime bound エラー~~ → `start_poll_task` / `start_ws_task` の構成整理で解消
- ~~W-001: `SplashScreen` コンポーネント未使用警告~~ → `App` の `splash` フィールドに組み込み済み
- ~~W-002: Phase 3 関連の未使用警告~~ → `create_payment_tx` 削除済み。`create_and_sign_payment` の `#[allow(dead_code)]` と “Phase 3” コメントも除去済み（本番経路: `poll.rs` / `cli_exec.rs`、2026-09-12）
