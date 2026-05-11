# design.md

## 1. アーキテクチャ概要

- C4（システム文脈・コンテナ）: [architecture/c4-context.md](architecture/c4-context.md), [architecture/c4-containers.md](architecture/c4-containers.md)
- 実行モードは `watch`（TUI監視）と CLI モード（`info`、`account`、`book`、`summary`、`nfts`、`lines`、`amm`、`txhistory`、`account-status`、`send` など、`src/cli.rs` の `Cmd` に定義されたサブコマンド）。
- エントリーポイントは `src/main.rs`。`watch` 時は `create_app(...)` で `App` を構築し、その他は `xrpl::execute_cli_command(...)` を実行する。
- TUI は `ratatui` + `crossterm`、非同期処理は `tokio` を使用する。
- XRPL 通信は `src/xrpl/` 配下に分割（`client.rs`、`ws.rs`、`poll.rs` 等）し、`mod.rs` が公開 API を再エクスポートする。

## 2. モジュール責務

### `src/main.rs`

- CLI 引数を解釈して起動モードを分岐する。
- 共通初期化（エラー/ロギング）を行う。

### `src/app.rs`

- `App` が TUI の実行ライフサイクル全体を管理する。
- `Action` チャネルでコンポーネントとバックグラウンドタスクを疎結合化する。
- `start_ws_task(...)` と `start_poll_task(...)` を起動し、終了時は `CancellationToken` で停止する。

### `src/xrpl/`（サブモジュール）

- `client.rs`: `RpcClient` が RPC リクエスト（`server_info`, `fee`, `account_info`, `book_offers`, `account_objects`、`submit` 向け `submit_signed_tx` 等）と JSON 応答パース、`xrp_to_drops` を提供する。
- `ws.rs`: `start_ws_task(...)` が WS 購読ループを実行し、受信イベントを `Action` へ変換する。
- `poll.rs`: `start_poll_task(...)` がポーリングループを実行し、定期/手動更新コマンドを処理する（AccountSet/Payment の署名送信経路を含む）。
- `cli_exec.rs`: `execute_cli_command(...)` が非 TUI の CLI 出力を担当する（`Send` などは `crate::signing` と連携）。
- `types.rs`: パネル用行型・`BookPair`・`PollContext` / `PollCommand` 等。

### `src/components/*`

- 画面表示を `panels`（基底パネル）、`tabs`（統合タブ画面）、`shared`（共通部品）の3層に分離。
- `Component` トレイトで統一し、`Action` を受けて内部状態を更新し、`draw` で描画を行う。
- `widgets.rs` が `titled_block`（共通枠スタイル）とスピナフレームのユーティリティを提供する。
- `StatusBar` は接続状態・最終更新経過時間・監視アカウント・リフレッシュ中スピナ・エラーを1行に表示する。
- `WalletPanel` は seed から導出したアドレス概要と直近トランザクションを表示する。ウォレット枠にフォーカスした状態で **`t`** でモーダルを開き **AccountSet** と **Payment（XRP）** を選んでからフォームに入る。型選択は **Tab / jk / 矢印**。AccountSet は **Tab / `[` `]`** で行移動、**`e`** 編集、**`s`** 送信。Payment はモーダル内に送信プレビュー（緑＝送信可能、橙＝未入力/不正）を出し、**Tab / `[` `]`** と **Enter**（未編集で編集開始／編集中は次フィールド）で行を動かし、**`s`** は未編集時または **^S** で送信キューへ入れる。送信前に宛先・正の数として amount を検証し、成功時はモーダルを閉じてウォレット下段に緑のメッセージ、失敗は赤を表示する。AccountSet はモーダル内で **SetFlag / ClearFlag（`parse_account_set_flag_choice` と一致するラベルのみ有効）**、domain ASCII、tick size、transfer rate を編集し、送信成功時もモーダルを閉じる。Payment は送信先（classic または X-address）と XRP 数量を入力し、`PollCommand::PaymentSubmit` 経由で `create_and_sign_payment` → `submit`。メインネット書き込みは CLI `--yes` が必要。seed 未設定・無効時の表示は従来どおり。Watch 起動時は **`main` で一度だけ構築した `Config`（`XRPL_SEED` マージ済み）を `App::new` に渡す** — `SigningConfig::prime_seed_source` が env を消したあとに `Config::new()` を再実行するとシードが復元できない。

### `src/config.rs`

- 組み込みデフォルト設定（リポジトリ直下の `config.json5` を `include_str!`）を読み込む。
- 設定ファイル探索先は `$XDG_CONFIG_HOME/lazyxrp/config.toml` 優先、未設定時は `~/.config/lazyxrp/config.toml` を利用する。
- ユーザー設定ファイルをマージし、キーバインド/スタイル/XRPL 設定を最終決定する。

## 3. ランタイムフロー（watch）

1. `main` が `Cmd::Watch` を選択して `App::run()` を開始。
2. `App` がコンポーネントを初期化し、`action` / `poll` チャネルを生成。
3. `start_ws_task` が WS 購読を開始し、台帳イベントやトランザクションを `Action` として送信。
4. `start_poll_task` が定期的に RPC を呼び、サーバー/手数料/アカウント/板情報を `Action` へ送信。
5. メインループが `Event` と `Action` を処理してコンポーネント更新・描画を繰り返す。
6. 終了時に `CancellationToken` をキャンセルし、バックグラウンド処理を停止して TUI を終了。

## 4. データフロー

- 入力:
  - ユーザーキー入力（`crossterm`）→ `Event` → `Action`
  - XRPL RPC/WS レスポンス → `Action`
- 中継:
  - `tokio::sync::mpsc::UnboundedSender<Action>` でイベントを集中管理
  - `PollCommand` は手動更新要求（アカウント / 板 / **ledger objects (`account_objects`)** / **AccountSet 署名送信** / **Payment（XRP）署名送信** 等）をポーリングタスクへ通知
- 出力:
  - `Action` を基にコンポーネント状態を更新
  - `ratatui` フレームへ描画

## 5. 設定値と起動パラメータ

- CLI 引数:
  - `--yes`（メインネットでの書き込み確認スキップ；CLI `Send` および TUI の AccountSet / Payment 送信に適用）
  - `--server`（RPC URL）
  - `--ws-server`（WebSocket URL）
  - `--tick-rate`, `--frame-rate`
  - `--seed`（署名用シード、env/config を上書き）
  - `watch --account`（監視アカウント上書き）
- 設定ファイル（`Config::xrpl`）:
  - 探索順序: `$XDG_CONFIG_HOME/lazyxrp/config.toml` → `~/.config/lazyxrp/config.toml`
  - `account`, `issuer`, `currency`, `currency_code`, `offer_limit`, `poll_interval_ms`
  - `currency` は表示名、`currency_code` は `book_offers` / price RPC に渡す 160-bit 通貨コード（未設定時は `"USD"`）
  - `issuer` は未設定時に mainnet Bitstamp USD issuer `rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B` にフォールバックする

## 6. Phase 1: 読み取り拡張（FR-08〜FR-11）

### 6.1 新規データ型（`src/xrpl/types.rs` 等）

```rust
// NFT 1件の表示行
pub struct NftRow {
    pub nft_id: String,
    pub taxon: u32,
    pub serial: u32,
    pub transfer_fee: u16,
    pub uri: String,         // hex -> UTF-8変換後
    pub is_mutable: bool,    // Flags に tfMutable (0x10) が立つ dNFT
}

// TrustLine 1件の表示行
pub struct TrustLineRow {
    pub currency: String,
    pub account: String,    // issuer
    pub balance: String,
    pub limit: String,
}

// AMM プールサマリ
pub struct AmmSummary {
    pub asset1: String,
    pub asset2: String,
    pub lp_token: String,
    pub trading_fee: u16,
    pub pool1: String,
    pub pool2: String,
}

// Tx 履歴 1件の表示行
pub struct TxRow {
    pub hash: String,
    pub tx_type: String,
    pub ledger_index: u32,
    pub result: String,
}

// account_objects の 1 行（タブ側で LedgerEntryType によりフィルタ）
pub struct LedgerObjectRow {
    pub ledger_type: String,
    pub index: String,
    pub detail: String,
}
```

### 6.2 新規 RPC メソッド（`RpcClient` に追加）

```rust
impl RpcClient {
    pub async fn account_nfts(&self, account: &str) -> color_eyre::Result<Vec<NftRow>>;
    pub async fn account_lines(&self, account: &str) -> color_eyre::Result<Vec<TrustLineRow>>;
    pub async fn amm_info(&self, asset1: Currency<'static>, asset2: Currency<'static>)
        -> color_eyre::Result<AmmSummary>;
    pub async fn account_tx(&self, account: &str, limit: u32)
        -> color_eyre::Result<Vec<TxRow>>;
    pub async fn account_objects(&self, account: &str)
        -> color_eyre::Result<Vec<LedgerObjectRow>>;
    pub async fn account_overview(&self, account: &str)
        -> color_eyre::Result<(Option<AccountSummary>, Vec<TxRow>)>;
    pub async fn xrp_rlusd_price(&self, rlusd_currency: &str, rlusd_issuer: &str)
        -> color_eyre::Result<XrplRlusdPrice>;
}
```

### 6.3 新規 Action バリアント（`src/action.rs` に追加）

```rust
XrplAccountNfts(Vec<NftRow>),
XrplTrustLines(Vec<TrustLineRow>),
XrplAmmInfo(Box<AmmSummary>),
XrplTxHistory(Vec<TxRow>),
XrplWalletOverview(Option<AccountSummary>, Vec<TxRow>),
XrplRlusdPrice(XrplRlusdPrice),
XrplLedgerObjects(Vec<LedgerObjectRow>),
RefreshNfts,
RefreshLines,
RefreshTxHistory,
RefreshLedgerObjects,
FocusNext,
FocusPrev,
TabJump(usize),
```

### 6.4 新規 CLI コマンド（`src/cli.rs` の `Cmd` に追加）

```rust
Nfts { address: String },
Lines { address: String },
Amm {
    #[arg(long)] asset1: String,
    #[arg(long)] asset2: String,
    #[arg(long)] issuer1: Option<String>,
    #[arg(long)] issuer2: Option<String>,
},
TxHistory {
    address: String,
    #[arg(long, default_value_t = 10)] limit: u32,
},
```

### 6.5 TUI パネル

- Phase 1 で NFT / TrustLine / AMM / TxHistory の **TUI パネルも実装済み**。
- `src/components/panels/nft.rs`、`trust_lines.rs`、`amm.rs`、`tx_history.rs`、`wallet.rs` が対応。
- **`ledger_objects.rs`**: `account_objects` 応答の一覧・詳細ウィジェット。
- 統合タブとして `src/components/tabs/account_tx.rs`、`market.rs`、`server_overview.rs` に加え、**`account_objects.rs`** を追加（**Objects** 1 タブで上段: Check / Ticket / MPT / DID 等、下段: PayChannel と Escrow）。

### 6.6 タブ構成（5 タブ）と ledger objects

- **Overview**: Server + Wallet  
- **Account**: Account + TxHistory  
- **Market**: Book + AMM + TrustLines  
- **NFTs**: NFT 一覧  
- **Objects**: 上段 — Check / Ticket / MPToken / DepositPreauth / SignerList / **DID**（分散識別子・credential 用 ledger object）など。下段左右 — PayChannel / Escrow（同一 `account_objects` 結果を種別フィルタ）

ポーリングは `PollCommand::LedgerObjects` で `RpcClient::account_objects` を実行し、`Action::XrplLedgerObjects` で UI に渡す。手動更新キーは設定で `RefreshLedgerObjects`（既定 `o`）。

---

## 7. Phase 2: ネットワーク抽象化 + シークレット基盤（FR-12〜FR-13）

### 7.1 `Network` 列挙型（`src/network.rs` 新規）

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Mainnet,
    Testnet,
    Devnet,
}

impl Network {
    pub fn rpc_url(&self) -> &str { ... }
    pub fn ws_url(&self)  -> &str { ... }
    pub fn display_name(&self) -> &str { ... } // TUI StatusBar 用
    pub fn is_mainnet(&self) -> bool { matches!(self, Self::Mainnet) }
}
```

| Network | RPC | WS |
|---|---|---|
| mainnet | `https://xrplcluster.com` | `wss://xrplcluster.com` |
| testnet | `https://s.altnet.rippletest.net:51234` | `wss://s.altnet.rippletest.net:51233` |
| devnet  | `https://s.devnet.rippletest.net:51234` | `wss://s.devnet.rippletest.net:51233` |

### 7.2 優先順位ロジック

```
CLI --network > XRPL_NETWORK env > config.toml [xrpl] network > default (mainnet)
```

- `--server` / `--ws-server` の明示指定は引き続き最優先（Custom エンドポイント指定指定用）。

### 7.3 Config 層の変更（`src/config.rs`）

`XrplConfig` に `network` フィールド追加:

```rust
#[serde(default)]
pub network: Network,   // Default::default() = Network::Mainnet
```

`config.toml` での記述例:

```toml
[xrpl]
network = "mainnet"   # mainnet | testnet | devnet
# rpc_server と ws_server は network の設定を上書きするカスタム用途向け
```

### 7.4 `src/main.rs` の分岐追加

```rust
// resolve_connection_config(ネットワーク解決关数)
// 優先順位ロジックを単一箇所に集約
fn resolve_rpc(cli: &Cli, config: &Config) -> String;
fn resolve_ws(cli: &Cli, config: &Config) -> String;
fn resolve_network(cli: &Cli, config: &Config) -> Network;
```

### 7.5 StatusBar のネットワークインジケータ

`StatusBar` の右端にネットワーク名を常時表示する。
mainnet 時は警告色（赤系）で強調する。

```
[ … 状態内容 … ] [MAINNET]
```

### 7.6 シーレット基盤（FR-13）

**読み込み順位**:

```
1. CLI --seed
2. 環境変数 XRPL_SEED
3. 設定ファイル ~/.config/lazyxrp/config.toml
   [xrpl.signing]
   seed = "sXXX..."
```

**デザイン決定**:

- シードはメモリ上で `SecretString`（いくつかの候補: [`secrecy`](https://crates.io/crates/secrecy) クレート）として保持し、`Debug`/`Display` でマスクする。
- `SigningConfig` 構造体を導入し、`App` や `run_cli` に渡す。
- mainnet 書き込み系操作実行前に `prompt_mainnet_confirmation(operation: &str, network: &Network, skip_prompt: bool) -> bool` で確認プロンプト（`skip_prompt` は `--yes` 相当）。
- `--yes` フラグで確認スキップ可能にする（CI/スクリプト用途）。
- Payment 署名は xrpl-rust 1.1 の `wallet::Wallet` と `transaction::sign`（`models::transactions::payment::Payment`）を利用し、`binarycodec::encode` で `submit` 用の `tx_blob` に変換する。`sEd...`（Ed25519 family seed）は xrpl-rust の `decode_seed` が secp 経路に落ちるケースがあるため、lazyxrp では `signing::wallet_from_family_seed` で Ed25519 プレフィックス専用デコードを先に行う。

---

## 8. 既知の設計上注意点

- `start_poll_task` は `tokio::spawn` でポーリング async タスクを起動する。引数は `PollContext` 構造体で受け取る。
- WS 起点の更新は過剰ポーリングを避けるため `MIN_POLL_INTERVAL` で間引き、通常の定期ポーリングは設定値 `poll_interval_ms` を尊重する。
- `splash.rs` は起動スプラッシュとして使用され、`App` の `splash` フィールドに組み込まれている。`Mode::Splash` で表示制御。`Action::Tick` ごとに ASCII 行ウェーブ・ロゴ領域 80% 幅での折り返し・接続行ドット・quit ヒント括弧の 4 系統のアニメを更新する。接続先の表示はマージ後の `config.xrpl.rpc_server`（未設定時は `xrplcluster.com` の既定ホスト）で、`XRPL_RPC_SERVER` は `Config::new()` で `[xrpl] rpc_server` より優先して取り込まれる。
