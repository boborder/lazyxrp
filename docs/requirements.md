# requirements.md

**Role:** 要件定義・ゴール設計（FR/NFR の SSOT）。

Product **requirements** (what). Behavioral design: [`architecture.md`](architecture.md). Interaction & visuals: [`DESIGN.md`](../DESIGN.md). Plan: [`roadmap.md`](roadmap.md).

## 1. 目的

- XRPL の状態を TUI で継続監視し、取引判断に必要な情報を素早く確認できるようにする。
- 個人運用で使える実用性と、公開プロジェクトとして再利用しやすい構成を両立する。

## 2. 対象ユーザー

- 主対象: 開発者本人（自分の運用で日常利用）
- 副対象: 公開リポジトリを使う外部ユーザー

## 3. 機能要件（Functional Requirements）

### FR-01: 監視モード

- `lazyxrp`（引数なし）で TUI を起動し、XRPL 情報を継続表示すること。
- 既定では設定ファイルのアカウントを監視し、`--account` で上書きできること（レガシー `watch` サブコマンドも deprecation 警告付きで可）。

### FR-02: サーバー情報取得

- RPC 経由で `server_info` / `fee` を定期取得し、画面に反映すること。

### FR-03: アカウント情報取得

- 監視対象アカウントの `account_info` を取得し、残高や状態を表示すること。
- 手動更新アクションで即時再取得できること。

### FR-04: 板情報取得

- 指定通貨ペアの `book_offers` を取得し、画面に反映すること。
- 手動更新アクションで即時再取得できること。

### FR-05: WebSocket サブスクライブ

- WebSocket 接続で対象アカウントのストリームを購読し、受信イベントを画面へ反映すること。

### FR-06: CLI コマンド実行

- `info` / `account` / `book` / `summary` の各コマンドを **`lazyxrp -x`**（または alias `rc`）でサポートし、TUI 非起動でも取得結果を出力できること。

### FR-07: 設定読み込み

- 設定はファイルから読み込み、未指定時は既定値を利用すること。
- 設定ファイル探索先は `$XDG_CONFIG_HOME/lazyxrp/config.toml` を優先し、未設定時は `~/.config/lazyxrp/config.toml` を利用すること。
- キーバインド（グローバル既定は `config.json5`、詳細は [`DESIGN.md`](../DESIGN.md)）と XRPL 接続情報（RPC/WS、通貨、issuer、poll 間隔）を扱えること。

## 4. 非機能要件（Non-Functional Requirements）

### NFR-01: 応答性

- 既定設定で描画と入力が途切れず、監視中に操作不能にならないこと。

### NFR-02: 信頼性

- RPC/WS の一時失敗時にプロセス全体が即時異常終了しないこと。
- 終了時にバックグラウンド処理をキャンセルし、TUI を正常終了できること。

### NFR-03: 可観測性

- 主要イベントとエラーをログ出力できること。

### NFR-04: 可搬性

- 開発環境（macOS arm64）でビルド可能であること。

## 5. 制約

- 実装言語は Rust（Edition 2024）。
- 非同期実行基盤は Tokio。
- UI ライブラリは Ratatui + Crossterm。
- XRPL 通信は `xrpl-rust` を利用する。

### FR-08: NFT 情報取得

- `account_nfts` で監視アカウントの NFT 一覧を取得し表示すること。
- TUI の Assets タブで選択中 NFT の対応画像 URI（HTTP(S) / IPFS / Arweave）を取得し、対応 terminal protocol または halfblocks fallback で preview 表示すること。
- CLI コマンド `nfts <address>` でも出力できること。

### FR-09: TrustLine 情報取得

- `account_lines` で監視アカウントの TrustLine 一覧を取得し表示すること。
- CLI コマンド `lines <address>` でも出力できること。

### FR-10: AMM プール情報取得

- `amm_info` で指定通貨ペアの AMM プール状態を取得し表示すること。
- CLI コマンド `amm --asset1 <currency> --asset2 <currency> [--issuer1 <r-addr>] [--issuer2 <r-addr>]` でも出力できること。

### FR-11: Tx 履歴取得

- `account_tx` で監視アカウントの最新 N 件のトランザクション履歴を取得し表示すること。
- CLI コマンド `txhistory <address> [--limit N]` でも出力できること。

### FR-14: アカウント ledger objects（読み取り）

- `account_objects` で監視アカウントに紐づくオブジェクト（Check / Ticket / MPT / DID / Payment Channel / Escrow 等）を取得し、TUI の **Assets** タブ（上段: misc、下段: channel/escrow）で種別に応じて表示すること。
- 手動リフレッシュ（既定キー `o`）で即時再取得できること。

### FR-12: ネットワーク選択

- `--network mainnet|testnet|devnet|xahau|xahau-test` で接続先を切り替えられること。
- 未指定時は `mainnet` を使用すること。
- 環境変数 `XRPL_NETWORK` および `config.toml` の `[xrpl] network` でも指定できること。
- 優先順位: CLI フラグ > 環境変数 > config.toml > デフォルト（mainnet）。
- TUI セッション内で `<Ctrl-n>` によりネットワークを循環切替できること（設定ファイルへは永続化しない）。カスタム RPC 指定時はエンドポイントを維持すること。
- `xahau` は本番チェーンとして mainnet 書き込みガード（`--yes`）が適用されること。

### FR-15: Flare 表示レベル

- `[flare] display` で `full` / `compact` / `off` を選択できること（既定 `full`）。
- `off` 時は Overview / Market の Flare パネル省略および FTSO/FXRP 読み取りポーリングをスキップすること（C3 execute 用 RPC URL は維持）。
- `compact` 時は Oracle テーブルを維持し FTSO/FXRP は一行サマリー表示すること。

### FR-16: Flare ウォレット読み取りパネル

- `[flare.wallet] address` に有効な EVM アドレスを設定した場合、Overview に読み取り専用パネルを表示すること（native FLR 残高、FXRP ERC20 残高、`[flare.fassets] execute` 状態、executor 用 env キー設定有無）。
- アドレス未設定時はセットアップガイダンスを表示すること（Flare への書き込みは行わない）。
- ポーリングは Overview タブ表示中のみ行うこと。

### FR-13: シークレット管理（Phase 3 準備基盤）

- 署名用資格情報は以下の優先順位で読み込めること：
  1. CLI `--seed` / `--mnemonic`
  2. 対応する環境変数 `XRPL_SEED` / `XRPL_MNEMONIC`
  3. 設定ファイル `~/.config/lazyxrp/config.toml` の `[xrpl.signing] seed` / `mnemonic`
- BIP39 mnemonic は secp256k1、BIP44 account index 0 に対応すること。family seed の既存動作を維持すること。
- 設定ファイルに平文保存する場合のリスクをドキュメントに明記すること。
- 資格情報が設定されていない状態で書き込み系コマンドを実行した場合、明示的なエラーメッセージを返すこと。
- mainnet で資格情報を利用する書き込み系操作は、実行前に確認プロンプトを表示すること。
- 資格情報から導出したアドレスのアカウント情報を TUI の Account タブに表示すること（未設定時は案内、無効時はエラー表示）。

## 6. 開発ステータス

- Phase 1（読み取り拡張）+ Phase 2（ネットワーク抽象化 + Xahau / Flare multichain）: **実装完了**
- Phase 3（書き込み系 TX）: **実装済み** — TUI コンポーザー + CLI で Payment / AccountSet / SetRegularKey / OfferCreate / TrustSet / FXRP Mint / FXRP Execute に対応。EscrowCreate は poll/signing 完備だが UI 未実装（`docs/problems.md` P-003）。残りは本番動作確認とエッジケース対応。将来 TX 種別（AccountDelete / OfferCancel / EscrowFinish・Cancel / AMM Deposit・Withdraw）は [`roadmap.md`](roadmap.md) Cross-cutting 参照。
