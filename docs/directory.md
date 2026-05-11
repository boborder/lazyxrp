# directory.md

## 1. ディレクトリ構成

```txt
lazyxrp/
├── Cargo.toml
├── Cargo.lock
├── build.rs
├── README.md
├── .env.example
├── install.sh
├── .mise.toml
├── AGENTS.md
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── xrpl/
│   │   └── mod.rs
│   ├── cli.rs
│   ├── action.rs
│   ├── config.rs
│   ├── network.rs
│   ├── signing.rs
│   ├── components/
│   │   ├── mod.rs
│   │   ├── panels/
│   │   │   ├── mod.rs
│   │   │   ├── account.rs
│   │   │   ├── amm.rs
│   │   │   ├── book.rs
│   │   │   ├── ledger_objects.rs
│   │   │   ├── nft.rs
│   │   │   ├── server.rs
│   │   │   ├── trust_lines.rs
│   │   │   ├── tx_history.rs
│   │   │   └── wallet.rs
│   │   ├── tabs/
│   │   │   ├── mod.rs
│   │   │   ├── account_objects.rs
│   │   │   ├── account_tx.rs
│   │   │   ├── channels_escrow.rs（ファイルのみ残置; `tabs/mod.rs` 未登録。Objects は `account_objects.rs` に統合）
│   │   │   ├── market.rs
│   │   │   └── server_overview.rs
│   │   └── shared/
│   │       ├── mod.rs
│   │       ├── fmt.rs
│   │       ├── fps.rs
│   │       ├── help_overlay.rs
│   │       ├── selectable_table.rs
│   │       ├── splash.rs
│   │       ├── status_bar.rs
│   │       ├── theme.rs
│   │       └── widgets.rs
│   ├── tui.rs
│   ├── logging.rs
│   └── errors.rs
└── docs/
    ├── agents/
    │   ├── README.md
    │   ├── execution-contract.md
    │   ├── development-policy.md
    │   ├── testing.md
    │   └── project-reference.md
    ├── architecture/
    │   ├── c4-context.md
    │   └── c4-containers.md
    ├── requirements.md
    ├── design.md
    ├── tech.md
    ├── test.md
    ├── tasks.md
    ├── directory.md
    ├── reference.md
    ├── security.md
    └── problems.md
```

## 2. ルート直下ファイル

- `Cargo.toml`: クレート定義と依存関係、features 設定。
- `Cargo.lock`: 依存の固定バージョン。
- `build.rs`: ビルド時の補助処理。
- `README.md`: 利用者向けの概要と起動手順。
- `.env.example`: `XRPL_*` 環境変数の例（任意。一覧は `docs/tech.md` と実装を参照）。
- `install.sh`: インタラクティブインストーラ（必須は `curl` のみ）。プロンプトとメッセージは英語。`--help` で CLI 一覧（`--method cargo|binary`、`--install-rust` / `--no-install-rust`、`--install-mise` / `--no-install-mise`、`-q`）。TTY 時はアニメ付き対話で、cargo / mise 未導入ならインストールを提案；非 TTY / `-q` は既定の自動応答。
- `.mise.toml`: [mise](https://mise.jdx.dev/) の `install` タスク定義。
- `AGENTS.md`: プロジェクト運用ルールと実行契約の要約（禁止事項・クイックリファレンスを含む）。詳細は `docs/agents/`（入口は `docs/agents/README.md`）。

## 3. `src/` 配下の責務

- `main.rs`: 起動エントリーポイント。`watch` と CLI モードの分岐、ネットワーク/エンドポイント/シードの優先順位解決を担当。
- `app.rs`: TUI アプリ本体。イベントループ、コンポーネント管理、バックグラウンド処理起動を担当。
- `xrpl/mod.rs`: XRPL RPC/WS 通信、ポーリング、CLI コマンド実行ロジックを担当（XRP 文字列→drops などの変換もここに集約）。
- `cli.rs`: コマンドライン引数とサブコマンド定義。
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
- `amm.rs`: AMM プール詳細パネル。
- `nft.rs`: NFT 一覧パネル。
- `trust_lines.rs`: TrustLine 一覧パネル（Table + Scrollbar、残高で色分け）。
- `tx_history.rs`: TX 履歴パネル（Table + Scrollbar、tesSUCCESS で色分け）。
- `wallet.rs`: seed 由来アカウントのウォレット概要パネル。
- `ledger_objects.rs`: `account_objects` の一覧表示（各パネルが種別でフィルタ）。

## 5. `src/components/tabs/` 配下の責務

- `account_tx.rs`: Account + TxHistory の統合タブ。
- `market.rs`: Book + Amm + TrustLines の統合タブ。
- `server_overview.rs`: Server + Wallet の統合タブ。
- `account_objects.rs`: **Objects** タブ — 上段に Check / Ticket / MPT / DID 等、下段左右に Payment Channel と Escrow（同一 `account_objects` 結果をフィルタ）。

## 6. `src/components/shared/` 配下の責務

- `fmt.rs`: 数値書式ユーティリティ（XRP/drops/時刻変換）。
- `fps.rs`: フレームレート表示コンポーネント。
- `help_overlay.rs`: `?` キーで開閉するキーバインドヘルプオーバーレイ。
- `selectable_table.rs`: テーブル行選択・スクロールの共通補助。
- `splash.rs`: 起動スプラッシュコンポーネント。
- `status_bar.rs`: 画面下部 1 行のステータスバー。
- `theme.rs`: 共通テーマ・色定義。
- `widgets.rs`: 共通 UI ヘルパー（`titled_block`、`spinner`）。

## 7. `docs/` 配下の責務

- `agents/`: ルート `AGENTS.md` からリンクされるエージェント向け詳細。入口は `README.md`（各サブドキュメントへの索引）。実行契約全文、開発ポリシー、TDD 要約、プロジェクト参照。禁止事項の要約はルート `AGENTS.md` の「Prohibitions」。
- `architecture/`: C4 モデル（`c4-context.md`, `c4-containers.md`）。高レベル境界とコンテナ分解。
- `requirements.md`: 要件定義（機能/非機能）。
- `design.md`: アーキテクチャ設計とデータフロー。
- `tech.md`: 技術スタックと依存バージョン。
- `test.md`: テスト方針、TC-ID 付きケースリスト、TDD ロードマップ、実行コマンド。
- `tasks.md`: 現在のタスク状態と優先度。
- `directory.md`: このファイル。構成と責務の索引。
- `reference.md`: 参照先リンクや補助情報の一覧。
- `security.md`: セキュリティ設計と脅威モデル、対策の一覧。
- `problems.md`: 既知の問題と対処方針。
