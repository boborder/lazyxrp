# problems.md

[English](problems.md)

**Role:** 注意点・落とし穴（P-xxx 既知問題と運用メモ）。

## 既知の問題

### P-002: `critical-section` 未解決シンボルによるリンク失敗

- 症状: `Undefined symbols for architecture arm64: __critical_section_1_0_acquire/release`
- 原因: `embassy-sync` などの依存経路で `critical-section` が必要だが、`std` 実装が有効でない構成だとリンク失敗する
- 対応: `Cargo.toml` で `critical-section = { version = "1.2.0", features = ["std"] }` を追加
- 注意: ソース内に直接の利用がなくても、この依存は `std` 実装の有効化に必要。未使用依存として削除しない
- 検証: `cargo check` / `cargo build --bins` を連続実行して再発有無を確認する

### ~~P-003: EscrowCreate composer/poll が未接続~~ → 解決（2026-10-11）

- 実態は signing も未実装で、poll の `_ => {}` が `EscrowCreateSubmit` を知らないうちに捨てていた。signing / `submit_escrow_*` / composer（`9` Create・`0` Finish/Cancel）を追加し、poll の `PollCommand` match から catch-all を外した（新しい command の取りこぼしはコンパイルエラーになる）。TC-215–221、testnet E2E は `docs/test-ja.md`。

### P-005: `config.rs` が 1 ファイルのまま

- 症状: `src/config.rs` はテスト込みで約 1530 行（defaults / file / env / keybindings / styles / Flare network+display+fassets+wallet+smart_accounts / paths / test helpers）
- 対応: 約 1500 行を超えたら `config/defaults.rs`, `config/keybindings.rs`, `config/styles.rs` に分割

### P-007: Xahau `NetworkID` の simulate 受理は未検証

- `network_id > 1024` のネットワーク（Xahau など）では write 時に `NetworkID` を tx へ注入する（simulate は `ensure_network` が保存した id、署名は `signing::with_network_id` のスレッドローカル scope）。
- Xahau の `simulate` が `NetworkID` 付き tx を受理するかは **実機未検証**（単体テスト TC-352 は注入条件のみ）。Xahau への write は live 確認が済むまで未保証。

### P-008: FXRP mint 完了処理の公開 executor 依存

- mainnet の read-only スキャン（2026-10-24）では公開 executor の存在が確認できた（約 50 日間で plain 905 件 + Smart Account 1000 件超の mint、XRPL validation から 95–177 秒後、サンプル 8 件）。ただし手数料の下限は未計測で、Flare への write は mainnet で一度も送信していない。
- 誰も支払いを拾わない場合、XRP は Core Vault で待機し、自力で完了させるには EVM 鍵 + 約 20 FLR が必要（ROADMAP F4-d、security.md 2026-10-24）。

### P-009: 二言語ドキュメントのずれ

- 自動チェックはない。編集時は `X.md` と `X-ja.md` の ID 集合（TC/FR/NFR/S/R/I/P/F）を比較する（maintenance-ja.md 参照）。

## 運用メモ

- macOS の `ld: ... built for newer 'macOS' version ...` は、現状は警告扱い
- ビルド安定化の完了条件は、`cargo check` に加えて `cargo build --bins` が成功すること（リンクエラーは `check` だけでは検出できない）

## 解消済み（履歴）

- ~~P-001: Rust の既知制限 (`issue #100013`) による lifetime bound エラー~~ → `start_poll_task` / `start_ws_task` の構成整理で解消
- ~~W-001: `SplashScreen` コンポーネント未使用警告~~ → `App` の `splash` フィールドに組み込み済み
- ~~W-002: Phase 3 関連の未使用警告~~ → `create_payment_tx` 削除済み。`create_and_sign_payment` の `#[allow(dead_code)]` と “Phase 3” コメントも除去済み（本番経路: `poll.rs` / `cli_exec.rs`、2026-09-12）
- ~~P-004: Submit 検証が層ごとに分裂~~ → `signing::validate_payment_input` を composer（preview / submit / quote）と poll `PaymentSubmit` で共有（TC-197–198、2026-10-10）
- ~~P-006: live `simulate` が `LastLedgerSequence` を省略~~ → rippled 3.x は Fee/Sequence のみ autofill するため、testnet では全 write（TUI + CLI）が失敗していた。`parse_simulate_result` が未設定時に `ledger_index + 20` を補う（TC-200、2026-10-10）
