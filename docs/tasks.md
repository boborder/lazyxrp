# tasks.md

## 1. ステータス定義

- `DONE`: 実装済みで通常利用できる
- `IN_PROGRESS`: 実装中または安定化作業中
- `TODO`: 未着手
- `BLOCKED`: 外部要因で停止中

## 2. 現在のタスク一覧

| ID | タスク | ステータス | 優先度 | 備考 |
| --- | --- | --- | --- | --- |
| T-001 | `watch` モードで TUI 起動・描画ループ実行 | DONE | P0 | `App::run()` で event/action ループが稼働 |
| T-002 | XRPL RPC ポーリング（server_info/fee/account/book） | DONE | P0 | `spawn_poll_loop` で定期取得 |
| T-003 | WS サブスクライブ（ledger/account tx 反映） | DONE | P0 | `spawn_ws_subscriptions` で購読 |
| T-004 | `info/account/book/summary` CLI コマンド | DONE | P1 | `run_cli` で実行可能 |
| T-005 | poll/WS タスク起動のビルド安定化 | DONE | P0 | `start_poll_task` / `start_ws_task` は `tokio::spawn` 起動で `cargo check` 通過済み |
| T-006 | `critical-section` 由来のリンクエラー安定化 | DONE | P0 | `critical-section` の `std` feature を導入済み、リンク安定化確認済み |
| T-007 | `SplashScreen` コンポーネントの扱い整理（利用 or 削除） | DONE | P2 | 起動スプラッシュとして実装済み |
| T-008 | 単体テストの追加（通貨変換・値抽出・整形） | DONE | P1 | `docs/test.md`（TC-ID）で追跡。`cargo test`: 71 passed、11 ignored（ローカル実行時点） |
| T-009 | 結合テストの追加（watch 起動/終了・CLI） | DONE | P1 | `xrpl::integration_live_network`（要外向き HTTPS）+ `app` の `Tui`/`process_actions` 系。オフライン CI は要検討 |
| T-010 | CI で `cargo check` / `cargo test` 自動化 | DONE | P2 | `.github/workflows/ci.yml` 実装済み（test/fmt/clippy/docs）。Rust は `rust-toolchain.toml`（`channel` 固定）で CI/CD 同期。**依存解決はリポジトリの `Cargo.lock` + `cargo … --locked`** |
| T-011 | ドキュメントの整理（docs配下の構成・リンク・責務の明確化） | DONE | P2 | `docs/*.md` の重複と参照導線を整備済み |
| T-012 | バイナリ配布対応（release ビルドと配布手順の整備） | DONE | P1 | `.github/workflows/cd.yml` でクロスコンパイル＋GitHub Releases 自動化。ビルドは `build-binaries` でアーティファクト化し、**全マトリクス成功後**に `publish-github-release` と `publish-cargo` が走る。`publish-github-release` は `permissions.contents: write`（既定 read-only の `GITHUB_TOKEN` で 403 になるのを防ぐ）。crates.io 公開も含む。Linux GNU のみ `[target.'cfg(...)']` で `openssl` `vendored`（`cross` 向け）。**cross** は `taiki-e/install-action` で **cargo-binstall** を入れたうえで `cargo binstall cross --no-confirm`（`--locked` なし）。**アプリ本体のビルド**は `cargo build --locked` / `cross build --locked`（ロックはリポジトリ管理）。Windows は MSVC で vendored OpenSSL+Perl を避ける |
| T-013 | ツール命名の統一（CLI 名・crate 名・ドキュメント表記） | DONE | P1 | `Cargo.toml` の package name を `lazyxrp` に変更。バイナリ名・CLI名と統一 |
| T-014 | XDG Base Directory 準拠の `config.toml` 読み込み対応 | DONE | P1 | `XDG_CONFIG_HOME` 優先 + `~/.config/lazyxrp/config.toml` フォールバックを実装 |
| T-015 | TUI に ASCII アート表示モードを追加 | DONE | P2 | `Home` コンポーネントでスプラッシュ＋ASCIIアート実装済み |
| T-016 | TUI の軽量アニメーション対応（点滅/ローディング等） | DONE | P2 | ブライユスピナを各パネル・StatusBar に実装済み |
| T-017 | RPC/WS 再接続に指数バックオフを追加 | DONE | P1 | poll/WS ループ両方に指数バックオフ実装済み |
| T-018 | リフレッシュコマンドのデバウンス | DONE | P2 | App で 500ms デバウンス実装済み |
| T-019 | キーバインドヘルプオーバーレイ（`?` キー） | DONE | P2 | `HelpOverlay` コンポーネント実装済み、`?`/Esc で開閉 |

| T-020 | `Network` 列挙型導入とエンドポイント定義 | DONE | P0 | `src/network.rs` 新規。mainnet/testnet/devnet の URL を集約 |
| T-021 | `--network` CLI フラグと優先順位ロジック | DONE | P0 | `CLI > env > config > default(mainnet)`。`resolve_network/rpc/ws` 関数 |
| T-022 | `XrplConfig` に `network` フィールド追加 | DONE | P0 | `config.toml` の `[xrpl] network` をサポート。`XRPL_NETWORK` env var も対応 |
| T-023 | StatusBar にネットワークインジケータ追加 | DONE | P1 | mainnet 時は赤・REVERSED で強調表示。testnet/devnet は黄色 |
| T-024 | `account_nfts` RPC + `nfts` CLI コマンド | DONE | P1 | `NftRow`・`XrplAccountNfts` Action・`run_cli` 対応 |
| T-025 | `account_lines` RPC + `lines` CLI コマンド | DONE | P1 | `TrustLineRow`・`XrplTrustLines` Action・`run_cli` 対応 |
| T-026 | `amm_info` RPC + `amm` CLI コマンド | DONE | P1 | `AmmSummary`・`XrplAmmInfo` Action・`run_cli` 対応 |
| T-027 | `account_tx` RPC + `txhistory` CLI コマンド | DONE | P1 | `TxRow`・`XrplTxHistory` Action・`run_cli` 対応 |
| T-028 | `SigningConfig` 導入（`XRPL_SEED` / config.toml） | DONE | P2 | `secrecy 0.10` でシードをメモリ上マスク。`src/signing.rs` 新規 |
| T-029 | mainnet 書き込み確認プロンプト基盤 | DONE | P2 | `prompt_mainnet_confirmation`。`--yes` 相当でスキップ可能 |
| T-030 | `docs/*.md` の重複と参照導線整備 | DONE | P2 | T-011 の完了。変更履歴と現行仕様の整合性確認済み |
| T-031 | コンポーネント再編（panels/tabs/shared 分離） | DONE | P2 | `src/components/` を3層に再構成。命名も統一 |
| T-032 | WalletPanel 実装（seed 由来アカウント表示） | DONE | P1 | seed 未設定/無効/有効の3状態表示。`--seed` CLI 引数追加 |
| T-033 | タブ統合（5タブ構成） | DONE | P1 | Overview / Account / Market / NFTs / Objects。数字キー `1`–`5` でジャンプ |
| T-034 | `account_objects` ポーリングと Objects タブ（misc + PayChan + Escrow） | DONE | P1 | `PollCommand::LedgerObjects`、`LedgerObjectRow`、TC-071〜073（`docs/test.md`） |

## 3. 直近マイルストーン

### M1: ビルド安定化（最優先）

- 目標:
  - `cargo check` が警告のみで完了する状態
  - 既知のリンクエラーが再発しない状態
- 完了条件:
  - ローカルで連続3回 `cargo check` 成功

### M2: 監視機能の回帰防止

- 目標:
  - `watch` の起動・更新・終了の最小回帰テストを用意
- 完了条件:
  - `docs/test.md` に定義した高優先テストを最低限実装

### M4: Phase 1 — 読み取り拡張

- 目標:
  - `nfts` / `lines` / `amm` / `txhistory` CLI コマンドが testnet で動作する
- 完了条件:
  - `cargo test` が全ケースで通る
  - 各コマンドが testnet 実アカウントで正常出力する

### M5: Phase 2 — ネットワーク抽象化

- 目標:
  - `--network` フラグで mainnet/testnet/devnet を切り替えられる
  - StatusBar にネットワーク名を常時表示
  - `XRPL_SEED` 環境変数 / config.toml `[xrpl.signing]` でシードを読み込める
- 完了条件:
  - `cargo test` が全ケースで通る
  - `--network testnet` 指定時に testnet エンドポイントへ接続することを確認

### M3: 公開向け整備

- 目標:
  - README と AGENTS の初版完成
  - 主要コマンドの利用手順が再現可能
  - 配布物と設定ファイル運用（XDG準拠）が利用者に明確
- 完了条件:
  - 新規利用者が `cargo run -- watch --account <address>` まで実行できる
