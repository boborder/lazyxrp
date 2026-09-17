# directory.md

**Role:** ディレクトリ構成・命名規則（`src/` モジュール責務と `docs/` 階層の索引）。

## 1. ディレクトリ構成

```txt
lazyxrp/
├── Cargo.toml
├── rust-toolchain.toml
├── Cargo.lock
├── build.rs
├── README.md
├── DESIGN.md              # UI/UX SSOT (keys, layout, theme, modals)
├── ROADMAP.md             # milestones / backlog SSOT
├── config.json5
├── .env.example
├── install.sh
├── mise.toml
├── AGENTS.md
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
│   │   ├── poll.rs
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
├── docs/
    ├── architecture.md    # システム・行動設計（TX detail §5.2.1 含む）
    ├── requirements.md    # FR/NFR（何をするか）
    ├── tech.md
    ├── test.md            # TC カタログ（requirements / architecture へトレース）
    ├── directory.md
    ├── references.md
    ├── security.md
    ├── problems.md
└── .agents/skills/         # domain skills (xrpl-rust, flare-*)
```


## 2. ルート直下ファイル

- `Cargo.toml`: クレート定義と依存関係、features 設定。
- `rust-toolchain.toml`: **CI とローカルで同じ** Rust チャンネル（現状 `stable`）。`.github/workflows/ci.yml` の `dtolnay/rust-toolchain@v1` は入力 `toolchain` にこのファイルの `channel` と同じ文字列を渡す（v1 で必須；`rustfmt` / `clippy` は該当ジョブで `components` 指定）。
- `config.json5`: キーバインド等の組み込みデフォルト（`src/config.rs` の `include_str!` 対象）。ユーザー設定 `config.toml` とは別物。
- `Cargo.lock`: 依存の固定バージョン（**バイナリ向けにコミット**。CI は `cargo … --locked`）。
- `build.rs`: ビルド時の補助処理。
- `README.md`: 利用者向けの概要と起動手順。
- `DESIGN.md`: UI/UX SSOT（キー・レイアウト・テーマ）。
- `ROADMAP.md`: マイルストーンとバックログ（進捗 SSOT）。
- `.env.example`: `XRPL_*` 環境変数の例（任意。一覧は `docs/tech.md` と実装を参照）。
- `install.sh`: インタラクティブインストーラ（必須は `curl` **または** `wget`）。プロンプトとメッセージは英語。`--help` で CLI 一覧（`--method cargo|binary`、`--install-rust` / `--no-install-rust`、`--install-mise` / `--no-install-mise`、`-q`）。`CI=1` は `-q` 相当。PATH 未設定時は shell profile へ追記可。リリースアーカイブに `rp` があればそれを入れ、無ければ `rp` → `lazyxrp` symlink。**手動アンインストール**は `--uninstall-help`（`lazyxrp --self-uninstall`、`INSTALL_DIR/rp` 削除など）。
- `mise.toml`: [mise](https://mise.jdx.dev/) タスク（例: `install`、`tags`（一覧）、`tag-push`（緊急時の手動タグフォールバック）、`bench` / `bench-fast` / `bench-ci`（ベンチマーク））。`main` へ push して CI が緑で、かつ `Cargo.toml` の `version` が前回から上がっていれば [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) の `auto-tag` が `v<version>` を打ち、[`.github/workflows/cd.yml`](../.github/workflows/cd.yml) がリリースする。CD 成功後に `cd.yml` の `trigger-benchmark` ジョブが [`.github/workflows/benchmark.yml`](../.github/workflows/benchmark.yml) を `--ref v<version>` で dispatch する（GITHUB_TOKEN dispatch は `workflow_run` を発火させないため。手動 `workflow_dispatch` も可）。
- `AGENTS.md`: プロジェクト運用ルールと実行契約（禁止事項・読み順）。graphify の構造情報も参照。

## 3. `src/` 配下の責務

- `lib.rs`: `lazyxrp` / `rp` 両バイナリが共有するライブラリ。`run()`（TUI+CLI）と `run_rp()`（lookup 専用）。
- `main.rs`: `lazyxrp` バイナリの薄いエントリ（`lazyxrp::run()`）。
- `bin/rp.rs`: `rp` バイナリの薄いエントリ（`lazyxrp::run_rp()`）。`cargo install` / リリース tarball で常に付く。
- `uninstall.rs`: `lazyxrp --self-uninstall` — 実行中バイナリ・同階層の `{name}.bak`・同階層の `rp`（実体/symlink）、`Config` で解決した config/data ディレクトリの削除（`cargo uninstall` は呼ばない）。
- `app.rs`: TUI アプリ本体。イベントループ、コンポーネント管理、バックグラウンド処理起動を担当。
- `xrpl/`: XRPL 連携一式。`mod.rs` は再エクスポートのみ。`address.rs`（classic/X-Address 解決・ネットワーク一致検査）、`client.rs`（`RpcClient` façade・JSON-RPC / HTTPS dUNL fetch・`tx` lookup）、`dunl.rs`（XRPLF dUNL JSON・manifest ST）、`format.rs`（金額・path・ripple time 整形、`xrp_to_drops` / `hex_to_ascii` / `path_find_*`）、`parse.rs`（JSON-RPC レスポンスパーサ・book helper）、`util.rs`（JSON パスヘルパ + 再接続バックオフ）、`types.rs`（行データ型・`BookPair`・`PollContext` / `PollCommand`）、`poll.rs`（定期ポーリング・ウォレット送信パス）、`ws.rs`（WebSocket）、`cli_exec.rs`（非 TUI の `execute_cli_command` / `execute_rp_lookup`）、`toml.rs`（`xrp-ledger.toml` パーサ）。
- `cli.rs`: コマンドライン引数とサブコマンド定義（`Cli`）および `rp` 用 `RpCli`。
- `action.rs`: アプリ内部で流す `Action`。
- `config.rs`: 既定値 + 設定ファイルのロードとマージ、`XRPL_*` 環境変数（シード・RPC/WS・ネットワーク）の反映。
- `components/mod.rs`: UI コンポーネント共通トレイトとサブモジュール統合。
- `tui.rs`: TUI 基盤（描画・イベント・端末管理）の共通処理。
- `logging.rs`: ログ初期化処理。
- `errors.rs`: `color-eyre` の panic/eyre hook 導入（TUI 終了クリーンアップ付き）。
- `network.rs`: `Network` 列挙型（mainnet / testnet / devnet / xahau / xahau-test）とエンドポイント定義、`next_network()` セッション切替。
- `signing.rs`: `SigningCredential`（family seed / BIP39 mnemonic 管理）、`prompt_production_confirmation`、Payment 向け `create_and_sign_payment`（submit用blob生成） / `build_payment_tx_json_for_simulate`。

## 4. `src/components/panels/` 配下の責務

- `server.rs`: サーバー状態表示パネル。
- `account.rs`: アカウント情報表示パネル。
- `book.rs`: オーダーブック表示パネル。
- `path_find.rs`: `ripple_path_find` ルート一覧（送信額・ホップ・経路、安い順）。
- `amm.rs`: AMM プール詳細パネル。
- `oracle.rs` / `flare_ftso.rs` / `fxrp_direct_mint.rs` / `flare_wallet.rs` / `combined_oracle.rs`: XRPL oracle 集約・Flare FTSOv2・FXRP Direct Mint 読み取り・Overview 用統合表示。

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

- `fmt.rs`: 数値書式ユーティリティ（`group_digits` / `group_digits_u64`、`fmt_xrp` / `fmt_drops`、時刻変換）。
- `fps.rs`: フレームレート表示コンポーネント。
- `help_overlay.rs`: `?` キーで開閉するキーバインドヘルプオーバーレイ。
- `selectable_table.rs`: テーブル行選択・スクロールの共通補助。
- `splash.rs`: 起動スプラッシュコンポーネント。
- `status_bar.rs`: 画面下部 1 行のステータスバー。
- `theme.rs`: 共通テーマ・色定義（`ACCENT` に加え `SECONDARY` でハッシュ列などを区別）。
- `tx_detail/`: トランザクション詳細オーバーレイ（`TxDetailState` + `render_tx_detail`）— 全 XRPL トランザクション型をパースしてポップアップ表示。
  - `mod.rs`: 状態管理 (`TxDetailState`) とレンダリング (`render_tx_detail`, `detail_lines_for`)。
  - `format.rs`: 共通フォーマット関数（`fmt_xrpl_amount`, `push_common_lines`, `format_value`, `fmt_currency`）。URI hex は `xrpl::hex_to_ascii`。
  - `parsers.rs`: 29 種類の XRPL トランザクション型をパースする `*_detail_lines` 関数群。
- `widgets.rs`: 共通 UI ヘルパー（`titled_block`、`titled_block_with_count`、`tx_table`、`spinner`）。履歴テーブルは更新時に構築し、`render_selectable_table` で再利用する。

## 7. `docs/` 配下の責務

ドキュメント階層（ルート [`DESIGN.md`](../DESIGN.md) は `docs/` 外）:

| File | Layer | Contents |
|------|-------|----------|
| `requirements.md` | What | FR/NFR ids |
| `architecture.md` | How (system) | Network, config precedence, data flow, behavioral contracts |
| `../DESIGN.md` | Look & feel | Keys, layout splits, theme, modals, loading |
| `test.md` | Verify | TC catalog → traces to requirements + architecture |
| [`ROADMAP.md`](../ROADMAP.md) | When | Milestones, cross-cutting backlog |
| `tech.md` | Stack | Dependencies, versions, dev commands |
| `directory.md` | Index | This file |
| `references.md` | Links | External references |
| `security.md` | Audit | S-xxx / R-xxx, threat model |
| `problems.md` | Debt | P-xxx known issues |

Index: [`AGENTS.md`](../AGENTS.md).

## 8. Build / test commands and entry points

| Command | Purpose |
|---------|---------|
| `cargo check` | Minimum verification after code changes |
| `cargo fmt` | Format code |
| `cargo test` | Run all tests |
| `cargo build --release` | Release build |
| `./install.sh` or `mise run install` | Install binary |
| `mise run bench` | Full benchmark suite (~10 min) |
| `mise run bench-fast` | Quick benchmarks |
| `mise run bench-ci` | CI-equivalent benchmark (`benchmark.sh --json --fast --perf-only`) |

| Entry | File | Description |
|-------|------|-------------|
| `main()` | `src/main.rs` | Thin `lazyxrp` binary → `lazyxrp::run()`. |
| `main()` | `src/bin/rp.rs` | Thin `rp` binary → `lazyxrp::run_rp()` (lookup-only). |
| `run()` / `run_rp()` | `src/lib.rs` | Shared entry: TUI/CLI vs tx/account lookup. |
| `Cli` / `RpCli` | `src/cli.rs` | Clap-derived CLIs. `Cli`: TUI flags + `-x` script mode. |
| `App::run()` | `src/app.rs` | TUI main loop: event handling → action processing → dirty-flagged render (`needs_draw`). |
| `execute_cli_command()` / `execute_rp_lookup()` | `src/xrpl/cli_exec.rs` | Non-TUI CLI dispatchers. |
| `Config::new()` | `src/config.rs` | Config loading: built-in defaults → user config.toml → env vars. |
| `build.rs` | `build.rs` | Build-time metadata (vergen-gix for commit hash, date). |
