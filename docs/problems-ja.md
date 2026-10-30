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

### ~~P-005: `config.rs` が 1 ファイルのまま~~ → 解決（2026-10-28）

- `src/config.rs`（約 1530 行）を `src/config/mod.rs`（Config・ロード/マージ・env・パス・Flare セクション・複数領域にまたがるテスト）、`config/defaults.rs`（フォールバック定数・serde 既定値・`LedgerConfig` / `FlareFassetsConfig` の `Default`）、`config/keybindings.rs`、`config/styles.rs` に分割。純粋な移動のみで `crate::config::*` のパスは不変。

### P-007: Xahau `NetworkID` の simulate 受理は未検証

- `network_id > 1024` のネットワーク（Xahau など）では write 時に `NetworkID` を tx へ注入する（simulate は `ensure_network` が保存した id、署名は `signing::with_network_id` のスレッドローカル scope）。
- Xahau の `simulate` が `NetworkID` 付き tx を受理するかは **実機未検証**（単体テスト TC-352 は注入条件のみ）。Xahau への write は live 確認が済むまで未保証。

### P-008: FXRP mint 完了処理の公開 executor 依存

- mainnet の read-only スキャン（2026-10-24）では公開 executor の存在が確認できた（約 50 日間で plain 905 件 + Smart Account 1000 件超の mint、XRPL validation から 95–177 秒後、サンプル 8 件）。ただし手数料の下限は未計測で、Flare への write は mainnet で一度も送信していない。
- 誰も支払いを拾わない場合、XRP は Core Vault で待機し、自力で完了させるには EVM 鍵 + 約 20 FLR が必要（ROADMAP F4-d、security.md 2026-10-24）。

### P-009: 二言語ドキュメントのずれ

- 自動チェックはない。編集時は `X.md` と `X-ja.md` の ID 集合（TC/FR/NFR/S/R/I/P/F）を比較する（maintenance-ja.md 参照）。

### P-010: testnet の書き込みがまれに期限切れ（`LastLedgerSequence`）

- 症状: 2026-10-28 の testnet 書き込み約 10 件のうち 2 件（`-x send`・`-x offer-create`）が `expired: not validated by LastLedgerSequence` で終わった。`rp -t <hash>` では tx は存在するが `validated: false`。再実行ですぐ validated になった。続けて 12 件送信した追試ではすべて validated。
- 原因は未確定: Fee は `simulate` の値、`LastLedgerSequence` は `ledger_index + 20`（P-006）。simulate 後に手数料が跳ねると queue に残ったまま期限切れになる可能性がある。再現できないため手数料ロジックは変更していない。
- 別の仮説: 2 件とも `-x faucet` で資金供給した直後の最初の書き込みだった（`LastLedgerSequence` の基準が閉じた直後の ledger）。12 件の追試も faucet 直後の 1 件目が validated だったため弱まったが、否定はされていない。
- 再発時（特に mainnet）の次の手: 変更する前に、失敗した hash と並べて `-x info` の `OpenLedgerFee` を記録する。

### ~~P-011: RPC エラーが文字列プロトコルになっている~~ → 解決（2026-10-30）

- `parse::NodeError` がノード自身のエラー応答（`result.error`）で、`[XRPL-<kind>] <message>` はその `Display` に過ぎない。呼び出し側は、整形済み文字列への `starts_with("[XRPL-")` や部分文字列一致ではなく、型（`node_error`、`is_not_found`、`is_retryable_rpc_error`）に問い合わせる。トランスポートエラーが「not found」と誤判定されることもなくなった。
- **挙動変更（バグ修正）。** 旧分類器は、ノードの `error` *トークン*（`tooBusy`、`entryNotFound` など）を整形後のメッセージから探していたが、そこには散文の `error_message`（"The server is too busy to help you now."）しか入っていない。そのため `tooBusy`、`noNetwork`、`noCurrent`、`noClosed`、`failedToForward`、`amendmentBlocked` は、実ノードに対して一度も再試行されなかった（散文がトークンを繰り返す場合にだけ一致した。旧テストのフィクスチャがそうだった）。また `entryNotFound` / `lgrNotFound` / `actMalformed` は `NOT_FOUND` とラベルされるのに、そう扱われていなかった。どちらもトークンに従うようになった。表示テキストは変わらない。24 通りのエラー形で確認: 変わったのはこの 9 件のノードエラーと、以前は「not found」と読まれていたトランスポート文字列 1 件だけ。

### ~~P-012: `(sequence, fee_drops, last_ledger_sequence)` が 3 つの素の `u32` で流れている~~ → 解決（2026-10-30）

- `signing::SimulatedFill { sequence, fee_drops, last_ledger_sequence }` を `sequence_fee_ledger_from_simulate` が返し、すべての `create_and_sign_*`、`txspec` の `sign` ポインタ、CLI / submit の `sign` クロージャが受け取る。2 つを入れ替えるとコンパイルが通らない。`PaymentSpec` と `AccountSetChanges` は simulate リクエストと署名済みトランザクションで共通の Payment / AccountSet のフィールドを持ち、各フローが 1 回作って両方に渡す。変更前のコードと照合済み: フィールドごとに異なる値で、署名済み blob 31 件・simulate JSON・拒否メッセージがバイト単位で一致。`signing.rs` に `#[allow(clippy::too_many_arguments)]` は残っていない。

### P-013: 150 行を超える関数がまだ残っている

- 分割済み: `render_composer`（983 → 104 行）、wallet の `key_action`、`execute_cli_command`（483 → 123）。まだ長い: `poll.rs::drive_poll_loop`（370）と `poll_batch`（309）、`lib.rs::run_flare_command`（324）、`app.rs::drain_and_dispatch_actions`（280）、`tx_detail::detail_lines_for`（214）、`validator_detail_lines`（184）、`wallet.rs::update`（174）、`submit_fxrp_direct_mint_payment`（171）、`server.rs::draw`（164）、`submit_smart_account_instruction`（160）、`submit_payment_transaction`（159）、`account.rs::draw`（150）。
- `run_flare_command` には自動テストがない（書き込みに live の Flare ノードが要る）ため、実行手段と合わせて分割する。

### ~~P-014: `signing.rs` 以外に `too_many_arguments` の allow が 7 個残っている~~ → 解決（2026-10-30）

- クレート内に `#[allow(clippy::too_many_arguments)]` は残っていない。`poll::Backoff { secs, until }`（`record(succeeded)`、`is_active()`）が対になった `&mut` の backoff 変数を置き換える。`submit::SubmitPlan { submit_err, build, sign, on_ok }` が、`run_submit` / `run_submit_with_wallet` が位置で受け取っていた 3 つのクロージャに名前を付ける。`app::Endpoints { network, rpc_url, ws_url }` が `App::new` の受け取る解決済みの接続先、`server_detail::TomlFetch` が TOML 取得結果、`ComposerPhase::payment_edit_key(&mut self, key)` は 5 個の `&mut String` ではなく Payment フェーズ自身を操作する。
- 途中で見つけたもの: `ServerPanel::update` の 2 つ目の `Action::XrplTomlFetched ... if self.detail.visible` アームは到達不能だった（直前の `if self.detail.visible { match … }` がすべてのアームで return する）。移植せず削除した。

## 運用メモ

- macOS の `ld: ... built for newer 'macOS' version ...` は、現状は警告扱い
- ビルド安定化の完了条件は、`cargo check` に加えて `cargo build --bins` が成功すること（リンクエラーは `check` だけでは検出できない）

## 解消済み（履歴）

- ~~P-001: Rust の既知制限 (`issue #100013`) による lifetime bound エラー~~ → `start_poll_task` / `start_ws_task` の構成整理で解消
- ~~W-001: `SplashScreen` コンポーネント未使用警告~~ → `App` の `splash` フィールドに組み込み済み
- ~~W-002: Phase 3 関連の未使用警告~~ → `create_payment_tx` 削除済み。`create_and_sign_payment` の `#[allow(dead_code)]` と “Phase 3” コメントも除去済み（本番経路: `poll.rs` / `cli_exec.rs`、2026-09-12）
- ~~P-004: Submit 検証が層ごとに分裂~~ → `signing::validate_payment_input` を composer（preview / submit / quote）と poll `PaymentSubmit` で共有（TC-197–198、2026-10-10）
- ~~P-006: live `simulate` が `LastLedgerSequence` を省略~~ → rippled 3.x は Fee/Sequence のみ autofill するため、testnet では全 write（TUI + CLI）が失敗していた。`parse_simulate_result` が未設定時に `ledger_index + 20` を補う（TC-200、2026-10-10）
