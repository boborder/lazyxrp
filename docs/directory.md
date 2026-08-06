# directory.md

## 1. ディレクトリ構成

```txt
lazyxrp/
├── Cargo.toml
├── rust-toolchain.toml
├── Cargo.lock
├── build.rs
├── README.md
├── config.json5
├── .env.example
├── install.sh
├── .mise.toml
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
│   │   ├── backoff.rs
│   │   ├── client.rs
│   │   ├── cli_exec.rs
│   │   ├── dunl.rs
│   │   ├── format.rs
│   │   ├── json_util.rs
│   │   ├── parse.rs
│   │   ├── nft_image.rs          # NFT metadata/image fetch and limits
│   │   ├── poll.rs
│   │   ├── toml.rs
│   │   ├── types.rs
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
│       │   ├── fxrp_direct_mint.rs
│       │   ├── ledger_objects.rs
│       │   ├── oracle.rs
│       │   ├── path_find.rs
│       │   ├── server.rs
│       │   ├── server_detail.rs
│       │   ├── server_dunl.rs
│       │   ├── server_metrics.rs
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
└── docs/
    ├── README.md          # ドキュメント導線と一覧
    ├── tx-detail.md       # TX 詳細オーバーレイ
    ├── graphify.md        # graphify ナレッジグラフの使い方
    ├── RELEASE.md         # リリース / auto-tag
    ├── external/          # 外部システムのスナップショット（FAssets 等）
    ├── architecture/
    │   ├── c4-context.md
    │   └── c4-containers.md
    ├── agent/             # AGENTS.md からリンクする運用規約
    ├── requirements.md
    ├── design.md
    ├── tech.md
    ├── test.md
    ├── tasks.md
    ├── directory.md
    ├── references.md
    ├── security.md
    ├── problems.md
    └── benchmark.md
```

## 2. ルート直下ファイル

- `Cargo.toml`: クレート定義と依存関係、features 設定。
- `rust-toolchain.toml`: **CI とローカルで同じ** Rust チャンネル（現状 `stable`）。`.github/workflows/ci.yml` の `dtolnay/rust-toolchain@v1` は入力 `toolchain` にこのファイルの `channel` と同じ文字列を渡す（v1 で必須；`rustfmt` / `clippy` は該当ジョブで `components` 指定）。
- `config.json5`: キーバインド等の組み込みデフォルト（`src/config.rs` の `include_str!` 対象）。ユーザー設定 `config.toml` とは別物。
- `Cargo.lock`: 依存の固定バージョン（**バイナリ向けにコミット**。CI は `cargo … --locked`）。
- `build.rs`: ビルド時の補助処理。
- `README.md`: 利用者向けの概要と起動手順。
- `.env.example`: `XRPL_*` 環境変数の例（任意。一覧は `docs/tech.md` と実装を参照）。
- `install.sh`: インタラクティブインストーラ（必須は `curl` **または** `wget`）。プロンプトとメッセージは英語。`--help` で CLI 一覧（`--method cargo|binary`、`--install-rust` / `--no-install-rust`、`--install-mise` / `--no-install-mise`、`-q`）。`CI=1` は `-q` 相当。PATH 未設定時は shell profile へ追記可。リリースアーカイブに `rp` があればそれを入れ、無ければ `rp` → `lazyxrp` symlink。**手動アンインストール**は `--uninstall-help`（`lazyxrp --self-uninstall`、`INSTALL_DIR/rp` 削除など）。
- `.mise.toml`: [mise](https://mise.jdx.dev/) タスク（例: `install`、`tags`（一覧）、`tag-push`（緊急時の手動タグフォールバック）、`bench` / `bench-fast`（ベンチマーク））。`main` へ push して CI が緑で、かつ `Cargo.toml` の `version` が前回から上がっていれば [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) の `auto-tag` が `v<version>` を打ち、[`.github/workflows/cd.yml`](../.github/workflows/cd.yml) がリリースする。CD 成功後に [`.github/workflows/benchmark.yml`](../.github/workflows/benchmark.yml) が連鎖する（手動 `workflow_dispatch` も可）。
- `AGENTS.md`: プロジェクト運用ルールと実行契約（禁止事項・クイックリファレンスを含む）。graphify の構造情報も参照。

## 3. `src/` 配下の責務

- `lib.rs`: `lazyxrp` / `rp` 両バイナリが共有するライブラリ。`run()`（TUI+CLI）と `run_rp()`（lookup 専用）。
- `main.rs`: `lazyxrp` バイナリの薄いエントリ（`lazyxrp::run()`）。
- `bin/rp.rs`: `rp` バイナリの薄いエントリ（`lazyxrp::run_rp()`）。`cargo install` / リリース tarball で常に付く。
- `uninstall.rs`: `lazyxrp --self-uninstall` — 実行中バイナリ・同階層の `{name}.bak`・同階層の `rp`（実体/symlink）、`Config` で解決した config/data ディレクトリの削除（`cargo uninstall` は呼ばない）。
- `app.rs`: TUI アプリ本体。イベントループ、コンポーネント管理、バックグラウンド処理起動を担当。
- `xrpl/`: XRPL 連携一式。`mod.rs` は再エクスポートのみ。`address.rs`（classic/X-Address 解決・ネットワーク一致検査）、`client.rs`（`RpcClient` façade・JSON-RPC / HTTPS dUNL fetch・`tx` lookup）、`dunl.rs`（XRPLF dUNL JSON・manifest ST）、`format.rs`（金額・path・ripple time 整形、`xrp_to_drops` / `path_find_*`）、`parse.rs`（JSON-RPC レスポンスパーサ・book helper）、`json_util.rs`（JSON パスヘルパ）、`types.rs`（行データ型・`BookPair`・`PollContext` / `PollCommand`）、`poll.rs`（定期ポーリング・ウォレット送信パス）、`ws.rs`（WebSocket）、`cli_exec.rs`（非 TUI の `execute_cli_command` / `execute_rp_lookup`）、`toml.rs`（`xrp-ledger.toml` パーサ）、`backoff.rs`（再接続間隔）。
- `cli.rs`: コマンドライン引数とサブコマンド定義（`Cli`）および `rp` 用 `RpCli`。
- `action.rs`: アプリ内部で流す `Action` 定義。
- `config.rs`: 既定値 + 設定ファイルのロードとマージ、`XRPL_*` 環境変数（シード・RPC/WS・ネットワーク）の反映。
- `components/mod.rs`: UI コンポーネント共通トレイトとサブモジュール統合。
- `tui.rs`: TUI 基盤（描画・イベント・端末管理）の共通処理。
- `logging.rs`: ログ初期化処理。
- `errors.rs`: エラー型と関連ユーティリティ。
- `network.rs`: `Network` 列挙型（mainnet / testnet / devnet）とエンドポイント定義。
- `signing.rs`: `SigningConfig`（署名シード管理）、`prompt_mainnet_confirmation`、Payment 向け `create_and_sign_payment`（submit用blob生成） / `create_unsigned_payment_json`。

## 4. `src/components/panels/` 配下の責務

- `server.rs`: サーバー状態表示パネル。
- `account.rs`: アカウント情報表示パネル。
- `book.rs`: オーダーブック表示パネル。
- `path_find.rs`: `ripple_path_find` ルート一覧（送信額・ホップ・経路、安い順）。
- `amm.rs`: AMM プール詳細パネル。
- `oracle.rs` / `flare_ftso.rs` / `fxrp_direct_mint.rs` / `combined_oracle.rs`: XRPL oracle 集約・Flare FTSOv2・FXRP Direct Mint 読み取り・Overview 用統合表示。

- `trust_lines.rs`: TrustLine 一覧パネル（Table + Scrollbar、残高で色分け）。
- `tx_history.rs`: TX 履歴パネル（Table + Scrollbar、tesSUCCESS で色分け）。
- `wallet.rs`: seed 由来アカウントのサマリ + composer（取引一覧は `tx_history.rs`）。
- `ledger_objects.rs`: `account_objects` の一覧表示（各パネルが種別でフィルタ）。

## 5. `src/components/tabs/` 配下の責務

- `overview.rs`: Tab 0 — Server + Combined Oracle/FTSO/FXRP。
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
  - `format.rs`: 共通フォーマット関数（`fmt_xrpl_amount`, `push_common_lines`, `format_value`, `hex_to_ascii`, `fmt_currency`）。
  - `parsers.rs`: 29 種類の XRPL トランザクション型をパースする `*_detail_lines` 関数群。
- `widgets.rs`: 共通 UI ヘルパー（`titled_block`、`tx_table_row`、`render_tx_scroll_table`、`spinner`）。

## 7. `docs/` 配下の責務

- `README.md`: 各ドキュメントへの導線と一覧。エージェント向けルールはルート `AGENTS.md` に集約済み。
- `architecture/`: C4 モデル（`c4-context.md`, `c4-containers.md`）。高レベル境界とコンテナ分解。
- `requirements.md`: 要件定義（機能/非機能）。
- `design.md`: アーキテクチャ設計とデータフロー。
- `tech.md`: 技術スタックと依存バージョン。
- `test.md`: テスト方針、TC-ID 付きケースリスト、TDD ロードマップ、実行コマンド。
- `tasks.md`: 現在のタスク状態と優先度。
- `directory.md`: このファイル。構成と責務の索引。
- `references.md`: 参照先リンクや補助情報の一覧。
- `security.md`: セキュリティ設計と脅威モデル、対策の一覧。
- `problems.md`: 既知の問題と対処方針。
- `benchmark.md`: ベンチマークスuiteの使い方、計測項目、CI統合方法。
