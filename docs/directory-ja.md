# directory-ja.md

[English](directory.md)

**役割:** ディレクトリ構成・命名規則（`src/` モジュール責務と `docs/` 階層の索引）。

## 1. ディレクトリ構成

```txt
lazyxrp/
├── Cargo.toml
├── rust-toolchain.toml
├── Cargo.lock
├── build.rs
├── README.md
├── DESIGN.md              # UI/UX SSOT (keys, layout, theme, modals)
├── ROADMAP.md             # ゴール / マイルストーン SSOT（docs/roadmap-log.md: 注意点・進路変更）
├── ROADMAP-ja.md          # ROADMAP.md の日本語版
├── CHANGELOG.md           # user-facing changes per release (Keep a Changelog)
├── SECURITY.md            # vulnerability reporting + safety model (details: docs/security.md)
├── config.json5
├── .env.example
├── install.sh
├── mise.toml
├── deny.toml
├── AGENTS.md
├── .github/workflows/     # ci.yml / cd.yml / benchmark.yml
├── src/
│   ├── lib.rs                 # shared library (lazyxrp + rp binaries)
│   ├── main.rs                # thin `lazyxrp` binary entry
│   ├── bin/
│   │   └── rp.rs              # thin `rp` lookup binary entry
│   ├── app.rs
│   ├── action.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── network.rs
│   ├── signing.rs
│   ├── flare.rs
│   ├── flare_cmd.rs           # Flare EVM write commands (`-x fxrp-execute` / `fxrp-redeem` / `fxrp-tag-*` / `sa-*`)
│   ├── fdc.rs                 # Flare Data Connector proof flow (`prove_xrp_payment`)
│   ├── coverage.rs            # XRPL TX coverage SSOT (`-x coverage`)
│   ├── test_support.rs        # test helpers (`#[cfg(test)]`)
│   ├── tui.rs
│   ├── uninstall.rs
│   ├── logging.rs
│   ├── errors.rs
│   ├── xrpl/
│   │   ├── mod.rs
│   │   ├── address.rs
│   │   ├── client.rs
│   │   ├── cli_exec.rs
│   │   ├── dunl.rs
│   │   ├── format.rs
│   │   ├── parse.rs
│   │   ├── nft_image.rs          # NFT metadata/image fetch and limits
│   │   ├── poll.rs               # poll loop + read batch; PollCommand dispatch
│   │   ├── submit.rs             # write paths: guard → simulate → sign → submit, quote
│   │   ├── txspec/               # declarative tx specs
│   │   │   ├── mod.rs            # TxSpec / FieldSpec / registry
│   │   │   ├── nft.rs            # NFT family
│   │   │   ├── channel_amm.rs    # PaymentChannel / AMM families
│   │   │   ├── checks_misc.rs    # Check / Ticket / DepositPreauth / SignerListSet / Clawback
│   │   │   ├── identity.rs       # Credential / DID / Oracle / PermissionedDomain
│   │   │   ├── mpt.rs            # MPT family
│   │   │   └── xchain.rs         # XChain bridge family
│   │   ├── toml.rs
│   │   ├── types.rs
│   │   ├── util.rs               # JSON path helpers + reconnect backoff
│   │   └── ws.rs
│   └── components/
│       ├── mod.rs
│       ├── panels/
│       │   ├── mod.rs
│       │   ├── account.rs
│       │   ├── amm.rs
│       │   ├── book.rs
│       │   ├── combined_oracle.rs
│       │   ├── flare_ftso.rs
│       │   ├── flare_wallet.rs
│       │   ├── fxrp_direct_mint.rs
│       │   ├── ledger_objects.rs
│       │   ├── oracle.rs
│       │   ├── path_find.rs
│       │   ├── server.rs
│       │   ├── server_detail.rs
│       │   ├── server_dunl.rs
│       │   ├── trust_lines.rs
│       │   ├── tx_history.rs
│       │   ├── wallet.rs
│       │   ├── wallet_composer.rs
│       │   ├── wallet_keygen.rs
│       │   └── wallet_keys.rs
│       ├── tabs/
│       │   ├── mod.rs
│       │   ├── overview.rs          # Tab 0: Server + Oracle/FTSO/FXRP
│       │   ├── account_wallet.rs    # Tab 1: Wallet/Account + TxHistory
│       │   ├── market_oracle.rs     # Tab 2: Book / Lines / AMM / FTSO
│       │   ├── assets.rs            # Tab 3: NFT + ledger objects
│       │   └── nft.rs               # used by AssetsTab
│       └── shared/
│           ├── mod.rs
│           ├── fmt.rs
│           ├── fps.rs
│           ├── help_overlay.rs
│           ├── selectable_table.rs
│           ├── splash.rs
│           ├── status_bar.rs
│           ├── theme.rs
│           ├── widgets.rs
│           └── tx_detail/
│               ├── mod.rs
│               ├── format.rs
│               └── parsers.rs
├── tests/
│   ├── cli_smoke.rs           # binary boundary smoke
│   └── testnet_e2e.rs         # opt-in live testnet/devnet writes
├── docs/
│   ├── architecture.md  # システム・行動設計（TX detail §5.2.1 含む）
│   ├── architecture-ja.md
│   ├── requirements.md  # FR/NFR（何をするか）
│   ├── requirements-ja.md
│   ├── tech.md
│   ├── tech-ja.md
│   ├── maintenance.md  # リリース・公開フロー（運用）
│   ├── maintenance-ja.md
│   ├── test.md  # TC カタログ（requirements / architecture へトレース）
│   ├── test-ja.md
│   ├── roadmap-log.md  # 進路変更記録・計画上の注意点
│   ├── roadmap-log-ja.md
│   ├── directory.md
│   ├── directory-ja.md
│   ├── references.md
│   ├── references-ja.md
│   ├── security.md
│   ├── security-ja.md
│   ├── problems.md
│   └── problems-ja.md
└── .agents/skills/         # domain skills (xrpl-rust, flare-*)
```

**Docs の命名規則:** すべての `docs/*.md` と `ROADMAP.md` は、同じディレクトリに日本語版 `*-ja.md` を持つ。エージェントが基準にするのは英語版。片方を変更する場合は同じ変更内で他方も更新し、ID 集合（TC/FR/NFR/S/R/I/P/F）を一致させなければならない。

## 2. ルート直下ファイル

- `Cargo.toml`: クレート定義と依存関係、features 設定。
- `rust-toolchain.toml`: **CI とローカルで同じ** Rust チャンネル（現状 `stable`）。`.github/workflows/ci.yml` の `dtolnay/rust-toolchain`（SHA 固定、`# v1`）は入力 `toolchain` にこのファイルの `channel` と同じ文字列を渡す（v1 で必須；`rustfmt` / `clippy` は該当ジョブで `components` 指定）。Actions の SHA 更新は `.github/dependabot.yml`。
- `config.json5`: キーバインド等の組み込みデフォルト（`src/config.rs` の `include_str!` 対象）。ユーザー設定 `config.toml` とは別物。
- `Cargo.lock`: 依存の固定バージョン（**バイナリ向けにコミット**。CI は `cargo … --locked`）。
- `build.rs`: ビルド時の補助処理。
- `README.md`: 利用者向けの概要と起動手順。
- `DESIGN.md`: UI/UX SSOT（キー・レイアウト・テーマ）。
- `ROADMAP.md` / `ROADMAP-ja.md`: ゴール階層（最終ゴール / 長期 / 中期 / 短期）とマイルストーン（進捗 SSOT）。日本語版が `ROADMAP-ja.md`。注意点・進路変更記録は `roadmap-log.md`。
- `CHANGELOG.md`: 利用者向けの変更履歴（Keep a Changelog）。`[Unreleased]` に追記し、リリース時に版番号へ確定。crates.io パッケージにも同梱。
- `SECURITY.md`: 脆弱性の報告窓口（GitHub private vulnerability reporting）と安全モデルの要約。詳細と履歴は `docs/security.md`。
- `.env.example`: `XRPL_*` 環境変数の例（任意。一覧は `docs/tech.md` と実装を参照）。
- `install.sh`: インタラクティブインストーラ（必須は `curl` **または** `wget`）。プロンプトとメッセージは英語。`--help` で CLI 一覧（`--method cargo|binary`、`--install-rust` / `--no-install-rust`、`--install-mise` / `--no-install-mise`、`-q`）。`CI=1` は `-q` 相当。PATH 未設定時は shell profile へ追記可。リリースアーカイブに `rp` があればそれを入れ、無ければ `rp` → `lazyxrp` symlink。**手動アンインストール**は `--uninstall-help`（`lazyxrp --self-uninstall`、`INSTALL_DIR/rp` 削除など）。
- `mise.toml`: [mise](https://mise.jdx.dev/) タスク（`install`、`tags`（一覧）、`tag-push`（緊急時の手動タグフォールバック）、`bench` / `bench-fast` / `bench-ci`（ベンチマーク））。公開フロー（CI の `Cargo.toml` version 検知 → auto-tag → CD 公開）の詳細は [`maintenance-ja.md`](maintenance-ja.md)。
- `AGENTS.md`: プロジェクト運用ルールと実行ルール（禁止事項・読み順）。graphify の構造情報（`graphify-out/`）は gitignore で、ローカル生成のみ。
- `deny.toml`: cargo-deny 設定（advisories / licenses / bans / sources）。CI `security-advisories` と `mise run audit` が使う。

## 3. `src/` 配下の責務

- `lib.rs`: `lazyxrp` / `rp` 両バイナリが共有するライブラリ。`run()`（TUI+CLI）と `run_rp()`（lookup 専用）。
- `coverage.rs`: XRPL TX 種別カバレッジ（`rows()` = Existing / Spec（`xrpl::txspec::registry()` 由来）/ Planned / Unsupported、read は tx_detail parser 登録から）。`markdown_table()` を `lazyxrp -x coverage` が出力し、ROADMAP.md「XRPL coverage」と README の件数を drift テスト（TC-248）が検証。
- `xrpl/txspec/`: 宣言的 TX spec（`mod.rs` = `TxSpec` / `FieldSpec` / `FAMILIES` レジストリ・検証・`prepare`。family ごとに 1 ファイル、§1 参照）。共通 submit は `submit::submit_generic_transaction`。追加手順は `architecture-ja.md` §2.1。`tests/testnet_e2e.rs` に NFT mint → burn の opt-in テスト。
- `main.rs`: `lazyxrp` バイナリの薄いエントリ（`lazyxrp::run()`）。
- `bin/rp.rs`: `rp` バイナリの薄いエントリ（`lazyxrp::run_rp()`）。`cargo install` / リリース tarball で常に付く。
- `uninstall.rs`: `lazyxrp --self-uninstall` — 実行中バイナリ・同階層の `{name}.bak`・同階層の `rp`（実体/symlink）と、`Config` で解決した config/data ディレクトリを削除する（`cargo uninstall` は呼ばない）。
- `app.rs`: TUI アプリ本体。イベントループ、コンポーネント管理、バックグラウンド処理の起動を担当する。
- `xrpl/`: XRPL 連携一式。`mod.rs` は再エクスポートのみ。`address.rs`（classic/X-Address 解決・ネットワーク一致検査）、`client.rs`（`RpcClient` façade・JSON-RPC / HTTPS dUNL fetch・`tx` lookup）、`dunl.rs`（XRPLF dUNL JSON・manifest ST）、`format.rs`（金額・path・ripple time 整形、`xrp_to_drops` / `hex_to_ascii` / `path_find_*`）、`parse.rs`（JSON-RPC レスポンスパーサ・book helper）、`util.rs`（JSON パスヘルパ + 再接続バックオフ）、`types.rs`（行データ型・`BookPair`・`PollContext` / `PollCommand`）、`poll.rs`（定期ポーリング・`PollCommand` 振り分け）、`submit.rs`（ウォレット送信パス: mainnet guard → simulate → sign → submit、`submit_*` 各種・`simulate_tx_requiring_tes_success`・Payment quote）、`ws.rs`（WebSocket）、`cli_exec.rs`（非 TUI の `execute_cli_command` / `execute_rp_lookup`）、`toml.rs`（`xrp-ledger.toml` パーサ）。
- `cli.rs`: コマンドライン引数とサブコマンド定義（`Cli`）および `rp` 用 `RpCli`。
- `action.rs`: アプリ内部で流す `Action`。
- `config.rs`: 既定値 + 設定ファイルのロードとマージ、`XRPL_*` 環境変数（シード・RPC/WS・ネットワーク）の反映。
- `components/mod.rs`: UI コンポーネント共通トレイトとサブモジュール統合。
- `tui.rs`: TUI 基盤（描画・イベント・端末管理）の共通処理。
- `logging.rs`: ログ初期化処理。
- `errors.rs`: `color-eyre` の panic/eyre hook 導入（TUI 終了クリーンアップ付き）。
- `network.rs`: `Network` 列挙型（mainnet / testnet / devnet / xahau / xahau-test）とエンドポイント定義、`next_network()` セッション切替。
- `signing.rs`: `SigningCredential`（family seed / BIP39 mnemonic 管理）、`prompt_production_confirmation`、Payment 向け `create_and_sign_payment`（submit用blob生成） / `build_payment_tx_json_for_simulate`。
- `test_support.rs`: テスト共通ヘルパ（`TempRootGuard` / `env_lock` / `env_lock_async` / `TestEnvGuard`、`#[cfg(test)]` 限定）。
- `../tests/cli_smoke.rs`: バイナリ境界スモーク（`lazyxrp` / `rp` 実行ファイルの argv・終了コード・出力をローカル JSON-RPC で検証、TC-158）。

**依存方向（import 規則）:**
- `components/` が import してよいのは `action`・`config`（`Arc<Config>`）・`network`・`xrpl` の行型 / 純粋ヘルパ（`xrpl::sanitize_text` / `xrp_to_drops` / `hex_to_ascii` / `txspec`）・`signing` の純粋な検証・エンコード関数（`validate_*` / `parse_*` / `encode_fxrp_*`）・`flare` の型と純粋ヘルパ（`format_token_units` / `classify_mint` / `check_operator` など）。`xrpl/client`・`xrpl/poll`・`xrpl/submit`・`app` は import しない（`RpcClient` を持たず、I/O は `Action` 経由）。
- `xrpl/` が import してよい内部モジュールは `action`・`network`・`signing`・`config`・`cli`・`flare`・`coverage`（`cli_exec.rs` / `poll.rs` から）。`components/` と `app` は import しない。`xrpl/types.rs` は UI / poll に依存しない leaf。
- `coverage.rs` は `components::shared::tx_detail`（parser 登録）と `xrpl::txspec`（registry）を import する逆向き依存を持つ。`xrpl/cli_exec.rs` が `coverage` を呼ぶため、`xrpl/` → `coverage` → `components` の経路が存在する（読み取り専用の登録情報のみ）。
- `config.rs` は `action` の `Action` / `Mode` を import する（keybinding デシリアライズ用）。`Mode` は `action.rs` に置く。
- `signing.rs` は `network`・`secrecy`・XRPL SDK crate と `xrpl` の純粋ヘルパ（`xrp_to_drops` / `resolve_payment_destination` / `GeneratedWalletKeys`）に依存し、`components/` と `app` を import しない。
- 新しい TX 種別の write は共有パイプライン（`xrpl/submit.rs` の resolve / simulate / sign / submit、`poll.rs` の handler）を再利用し、種別ごとに独自の simulate・署名を書かない。宣言的な型は `xrpl/txspec/`（architecture-ja.md §2.1）。
- パネルを追加したら同じ変更で `app::TAB_TITLES` を更新する（タブ index とパネル順を一致させる、security-ja.md I-9）。
- 結合が強い箇所: `action.rs`（全送受信のハブ）、`poll.rs` から呼ばれる `submit.rs` の `submit_*`（型ごとの検証 + 共有 simulate/sign/submit）、`tx_detail/parsers.rs`（登録された TX 種別、architecture-ja.md §5.2.1）。

## 4. `src/components/panels/` 配下の責務

- `server.rs`: サーバー状態表示パネル。
- `account.rs`: アカウント情報表示パネル。
- `book.rs`: オーダーブック表示パネル。
- `path_find.rs`: `ripple_path_find` ルート一覧（送信額・ホップ・経路、安い順）。
- `amm.rs`: AMM プール詳細パネル。
- `oracle.rs` / `flare_ftso.rs` / `fxrp_direct_mint.rs` / `flare_wallet.rs` / `combined_oracle.rs`: XRPL oracle 集約・Flare FTSOv2・FXRP Direct Mint 読み取り（+ Smart Account 行）・Overview 用統合表示。

- `trust_lines.rs`: TrustLine 一覧パネル（Table + Scrollbar、残高で色分け）。
- `tx_history.rs`: TX 履歴パネル（Table + Scrollbar、tesSUCCESS で色分け）。
- `wallet.rs`: seed 由来アカウントのサマリ + composer（取引一覧は `tx_history.rs`）。
- `ledger_objects.rs`: `account_objects` の一覧表示（各パネルが種別でフィルタ）。

## 5. `src/components/tabs/` 配下の責務

- `overview.rs`: Tab 0 — Server + Combined Oracle/FTSO/FXRP + Flare wallet read panel。
- `account_wallet.rs`: Tab 1 — Wallet（上）+ TxHistory（下）。seed 未設定時は Account パネル。
- `market_oracle.rs`: Tab 2 — Book / Path-Find / AMM / Trust lines / Flare FTSO / XRPL Oracle。
- `assets.rs`: Tab 3 — NFT + ledger objects（PayChannel / Escrow 含む）。
- `nft.rs`: `AssetsTab` から利用する NFT サブビュー。

## 6. `src/components/shared/` 配下の責務

- `fmt.rs`: 数値書式ユーティリティ（`group_digits` / `group_digits_u64`、`fmt_xrp`、`truncate_middle` / `short_hex`、ローカル時刻変換）。
- `fps.rs`: フレームレート表示コンポーネント。
- `help_overlay.rs`: `?` キーで開閉するキーバインドヘルプオーバーレイ。
- `selectable_table.rs`: テーブル行選択・スクロールの共通補助。
- `splash.rs`: 起動スプラッシュコンポーネント。
- `status_bar.rs`: 画面下部 1 行のステータスバー。
- `theme.rs`: 共通テーマ・色定義（`ACCENT` に加え `SECONDARY` でハッシュ列などを区別）。
- `tx_detail/`: トランザクション詳細オーバーレイ（`TxDetailState` + `render_tx_detail`）— 全 XRPL トランザクション型をパースしてポップアップ表示。
  - `mod.rs`: 状態管理 (`TxDetailState`) とレンダリング (`render_tx_detail`, `detail_lines_for`)。
  - `format.rs`: 共通フォーマット関数（`fmt_xrpl_amount_from_value`, `push_common_lines_from_value`, `format_value`）。URI hex は `xrpl::hex_to_ascii`。
  - `parsers.rs`: 49 種類の XRPL トランザクション型をパースする `*_detail_lines` 関数群。
- `widgets.rs`: 共通 UI ヘルパー（`titled_block_with_count`、`tx_table`、`spinner`、`render_loading` / `render_empty` / `render_error`、`centered_popup_rect`）。履歴テーブルは更新時に構築し、`render_selectable_table` で再利用する。

## 7. `docs/` 配下の責務

ドキュメントの索引（各 doc が答える質問）は [`AGENTS.md`](../AGENTS.md) が SSOT。ここでは複製しない。

## 8. ビルド / テストコマンドとエントリポイント

| コマンド | 目的 |
|---------|------|
| `cargo check` | コード変更後の最小検証 |
| `cargo fmt` | コード整形 |
| `cargo test` | 全テスト実行 |
| `cargo build --release` | リリースビルド |
| `./install.sh` または `mise run install` | バイナリのインストール |
| `mise run bench` | ベンチマーク一式（約 10 分） |
| `mise run bench-fast` | 簡易ベンチマーク |
| `mise run bench-ci` | CI 相当のベンチマーク（`benchmark.sh --json --fast --perf-only`） |

| エントリ | ファイル | 説明 |
|-------|------|-------------|
| `main()` | `src/main.rs` | 薄い `lazyxrp` バイナリ → `lazyxrp::run()`。 |
| `main()` | `src/bin/rp.rs` | 薄い `rp` バイナリ → `lazyxrp::run_rp()`（lookup 専用）。 |
| `run()` / `run_rp()` | `src/lib.rs` | 共有エントリ: TUI/CLI と tx/account lookup。 |
| `Cli` / `RpCli` | `src/cli.rs` | Clap 由来の CLI。`Cli`: TUI フラグ + `-x` スクリプトモード。 |
| `App::run()` | `src/app.rs` | TUI メインループ: イベント処理 → アクション処理 → dirty フラグ付き描画（`needs_draw`）。 |
| `execute_cli_command()` / `execute_rp_lookup()` | `src/xrpl/cli_exec.rs` | 非 TUI の CLI ディスパッチャ。 |
| `Config::new()` | `src/config.rs` | 設定ロード: 組み込みデフォルト → ユーザー config.toml → 環境変数。 |
| `build.rs` | `build.rs` | ビルド時メタデータ（commit hash・日付は vergen-gix）。 |
