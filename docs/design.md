# design.md

Product and UX specification for lazyxrp. **Do not duplicate** module graphs, channel tables, or invariants here — use the SSOT links below.

| Topic | SSOT |
|-------|------|
| Modules, channels, startup/submit flows | [`agent/ARCHITECTURE.md`](agent/ARCHITECTURE.md) |
| Rules I-1〜I-11 (simulate, mainnet, poll interval, …) | [`agent/INVARIANTS.md`](agent/INVARIANTS.md) |
| Directory layout | [`directory.md`](directory.md) |
| TX detail overlay pipeline | [`tx-detail.md`](tx-detail.md) |
| Security audit (S-xxx) | [`security.md`](security.md) |
| Implementation risks (R-xxx) | [`agent/RISK_REGISTER.md`](agent/RISK_REGISTER.md) |
| Doc index & graphify | [`README.md`](README.md), [`graphify.md`](graphify.md) |

C4 diagrams: [architecture/c4-context.md](architecture/c4-context.md), [architecture/c4-containers.md](architecture/c4-containers.md).

## 1. 起動モードとスタック（要約）

- 実行モード: `watch`（TUI）、CLI サブコマンド、`--self-uninstall`（`logging::init` 前に終了）。
- エントリ: `src/main.rs` — `watch` → `App::new` → `run`、CLI → `xrpl::execute_cli_command(...)`.
- スタック: `ratatui` + `crossterm`、非同期 `tokio`、XRPL は `src/xrpl/`（詳細は ARCHITECTURE）。

## 2. ウォレット UI（`WalletPanel`）

`WalletPanel`（`src/components/panels/wallet.rs`）の操作仕様。構造は [`agent/ARCHITECTURE.md`](agent/ARCHITECTURE.md) の Account タブを参照。

- seed から導出したアドレス概要（Account, Balance, Sequence, Flags チップ, RegularKey, Domain デコード）と直近トランザクション表（**Hash / Dir / Type / Ledger / Result**、列ごとに色分け: ハッシュ `SECONDARY`/ターコイズ、方向 ▼赤／▲緑／·灰、種別アクセント、台帳ミュート、結果成功/失敗色）。
- ウォレット枠フォーカスかつ作成モーダル閉: **j/k・▲▼** で行選択・スクロールバー連動。**Enter** → TX 詳細オーバーレイ（[`tx-detail.md`](tx-detail.md)）。オーバーレイ中 **j/k・▲▼** スクロール、**Enter / Esc** で閉じる。
- **`t`**: コンポーザー — **AccountSet** または **Payment（XRP / IOU）**。型選択は **Tab / jk / 矢印**（モーダル中は一覧 j/k 無効）。
- **AccountSet**: **Tab / `[` `]`** 行移動、**`e`** 編集、**`s`** 送信。SetFlag/ClearFlag（`parse_account_set_flag_choice` ラベルのみ）、domain ASCII、tick size、transfer rate。
- **Payment**: プレビュー（緑=送信可、橙=未入力/不正）、**`i`** XRP⇔IOU、IOU 時 4 行（Destination, Currency, Issuer, Amount）、**Tab / `[` `]`** + **Enter**、**`s`** / **^S** でキュー。送信前検証（宛先・正の amount、IOU は currency 非空・issuer `r` 始まり）。
- 全 TX 送信: **simulate → sign → submit**（[`agent/INVARIANTS.md`](agent/INVARIANTS.md) I-3）。メインネット書き込みは CLI **`--yes`**（I-2）。
- Watch 起動: **`main` で一度だけ構築した `Config`（`XRPL_SEED` マージ済み）を `App::new` に渡す**。`prime_seed_source` 後に `Config::new()` を再実行しない。

## 3. ランタイム・データフロー

チャネル一覧・watch 起動シーケンス・WS→poll トリガ・送信パイプラインは [`agent/ARCHITECTURE.md`](agent/ARCHITECTURE.md)（Main Execution Flows）に集約。

製品視点の要約:

- 入力: キー → `Event` → `Action`、XRPL RPC/WS → `Action`。
- `PollCommand` で手動更新（アカウント / 板 / `account_objects` / 署名送信など）を poll タスクへ。
- 出力: コンポーネント `update` → `ratatui` 描画。

## 4. 設定値と起動パラメータ

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
  - `oracles`, `oracle_base_asset`, `oracle_quote_asset`（`get_aggregate_price` 用）
  - `currency` は表示名、`currency_code` は `book_offers` / price RPC に渡す 160-bit 通貨コード（未設定時は `"USD"`）
  - `issuer` は未設定時に mainnet Bitstamp USD issuer `rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B` にフォールバックする

## 5. Phase 1: 読み取り拡張（FR-08〜FR-11）

### 5.1 新規データ型（`src/xrpl/types.rs` 等）

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
    /// "▼" outbound, "▲" inbound, "·" self-only.
    pub direction: String,
    /// Raw transaction JSON (shared via Arc).
    pub tx_json: ArcValue,
    /// Raw metadata JSON (shared via Arc).
    pub meta_json: ArcValue,
}

// account_objects の 1 行（タブ側で LedgerEntryType によりフィルタ）
pub struct LedgerObjectRow {
    pub ledger_type: String,
    pub index: String,
    pub detail: String,
}
```

### 5.2 新規 RPC メソッド（`RpcClient` に追加）

```rust
impl RpcClient {
    pub async fn account_nfts(&self, account: &str) -> color_eyre::Result<Vec<NftRow>>;
    pub async fn account_lines(&self, account: &str) -> color_eyre::Result<Vec<TrustLineRow>>;
    pub async fn amm_info(&self, asset1: Currency<'static>, asset2: Currency<'static>)
        -> color_eyre::Result<AmmSummary>;
    pub async fn account_tx(&self, account: &str, limit: u32, marker: Option<Value>)
        -> color_eyre::Result<AccountTxPage>;
    pub async fn account_objects(&self, account: &str)
        -> color_eyre::Result<Vec<LedgerObjectRow>>;
    pub async fn account_overview(&self, account: &str)
        -> color_eyre::Result<(Option<AccountSummary>, Vec<TxRow>)>;
    pub async fn xrp_rlusd_price(&self, rlusd_currency: &str, rlusd_issuer: &str)
        -> color_eyre::Result<XrplRlusdPrice>;
}
```

### 5.3 新規 Action バリアント（`src/action.rs` に追加）

```rust
XrplAccountNfts(Vec<NftRow>),
XrplTrustLines(Vec<TrustLineRow>),
XrplAmmInfo(Box<AmmSummary>),
XrplTxHistory(Vec<TxRow>),
XrplWalletOverview(Option<AccountSummary>),
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

### 5.4 新規 CLI コマンド（`src/cli.rs` の `Cmd` に追加）

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

### 5.5 TUI パネル

- Phase 1 で NFT / TrustLine / AMM / TxHistory の **TUI パネルも実装済み**。
- `src/components/panels/nft.rs`、`trust_lines.rs`、`amm.rs`、`tx_history.rs`、`wallet.rs` が対応。
- **Enter キーで詳細ポップアップ**: `TxHistoryPanel`、`WalletPanel`（Recent Transactions）、`NftTab`、`BookPanel`、`TrustLinesPanel`、`LedgerObjectsPanel` のすべてで、テーブル行選択中に `Enter` を押すと `tx_detail::render_tx_detail` の統一オーバーレイが開く。各 `Row` 構造体（`TxRow`、`NftRow`、`OfferRow`、`TrustLineRow`、`LedgerObjectRow`）は `raw_json: ArcValue` フィールドで元の JSON を保持し、`render_tx_detail` に渡してスクロール可能な詳細表示を行う。
- **`TxHistoryPanel` と `WalletPanel`（Recent Transactions）の pagination（marker）対応**: 初回ロード時にレスポンスに `marker` が含まれていれば下部に「m: more」ヒントを表示し、`m` キーで次ページ（`PollCommand::TxHistoryMore` → `account_tx` with `marker`）を読み込んで既存リストに append する。`Action::XrplTxHistory` は置き換え、`Action::XrplTxHistoryAppend` は追加分を表す。
- **トランザクション詳細オーバーレイ**: 29 型 + JSON フォールバック。パイプライン・型一覧・変更手順は [`tx-detail.md`](tx-detail.md)。
- **`TxHistoryPanel` と `WalletPanel`（Recent Transactions）のフィルタ機能**: `f` キーでフィルタ入力モードに入り、ハッシュまたはトランザクション種別の部分一致でリアルタイム絞り込みを行う。`Enter` で確定、`Esc` で解除。フィルタ適用中はタイトルに `[filter: xxx]` を表示し、ヒント行に `f: filter` を表示する。
- **`ledger_objects.rs`**: `account_objects` 応答の一覧・詳細ウィジェット。
### 5.6 タブ構成（4 タブ）と ledger objects

- **Overview**: `OverviewTab` → `ServerPanel`（左）+ `CombinedOraclePanel`（XRPL aggregate + Flare FTSOv2 + FXRP Direct Mint C1）
- **Account**: `AccountWalletTab` — `WalletPanel` + `AccountPanel` + `TxHistoryPanel`
- **Market**: `MarketOracleTab` — Book / **PathFind** / AMM / TrustLines / Flare FTSO / Oracle
- **Assets**: `AssetsTab` — `NftTab` + `LedgerObjectsPanel`（Objects / Pay channels / Escrows）

#### Path-Find（Market タブ）

`PathFindPanel`（`src/components/panels/path_find.rs`）は、設定済みの book ペアに対する **self-payment プレビュー**（`ripple_path_find`）を表示する。

- ポーリング: `poll.rs` が `book_pair.path_find_destination_amount_preview()` を宛先 amount に使い、watch アドレス同士で path find を実行。
- サマリー: 受取目標（例 `1 USD`）とルート件数（**cheapest send first**）。
- テーブル列: `#` / `You send`（通貨単位付き）/ `Hops`（`direct` / `N hop(s)`）/ `Route`（中間アカウント列）。
- **Enter**: 選択ルートの生 JSON を TX 詳細オーバーレイで表示。

ポーリングは `PollCommand::LedgerObjects` で `RpcClient::account_objects` を実行し、`Action::XrplLedgerObjects` で UI に渡す。手動更新キーは設定で `RefreshLedgerObjects`（既定 `o`）。

---

## 6. Phase 2: ネットワーク抽象化 + シークレット基盤（FR-12〜FR-13）

### 6.1 `Network` 列挙型（`src/network.rs` 新規）

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

### 6.2 優先順位ロジック

```
CLI --network > XRPL_NETWORK env > config.toml [xrpl] network > default (mainnet)
```

- `--server` / `--ws-server` の明示指定は引き続き最優先（Custom エンドポイント指定指定用）。

### 6.3 Config 層の変更（`src/config.rs`）

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

### 6.4 `src/main.rs` の分岐追加

```rust
// resolve_connection_config(ネットワーク解決关数)
// 優先順位ロジックを単一箇所に集約
fn resolve_rpc(cli: &Cli, config: &Config) -> String;
fn resolve_ws(cli: &Cli, config: &Config) -> String;
fn resolve_network(cli: &Cli, config: &Config) -> Network;
```

### 6.5 StatusBar のネットワークインジケータ

`StatusBar` の右端にネットワーク名を常時表示する。
mainnet 時は警告色（赤系）で強調する。

```
[ … 状態内容 … ] [MAINNET]
```

### 6.6 シーレット基盤（FR-13）

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
- Payment 署名は xrpl-rust 1.1 の `wallet::Wallet` と `transaction::sign`（`models::transactions::payment::Payment`）を利用し、`binarycodec::encode` で `submit` 用の `tx_blob` に変換する。IOU Payment は `Amount` フィールドに `{currency, issuer, value}` オブジェクトを使用し、`create_and_sign_payment` が `iou_currency`/`iou_issuer` Option パラメータにより分岐する。`sEd...`（Ed25519 family seed）は xrpl-rust の `decode_seed` が secp 経路に落ちるケースがあるため、lazyxrp では `signing::wallet_from_family_seed` で Ed25519 プレフィックス専用デコードを先に行う。
- **全 TX 送信は simulate フロー**：最小限の unsigned `tx_json` を構築→`simulate_tx` 実行→`engine_result == "tesSUCCESS"` 確認→サーバーが自動入力した `Sequence`/`Fee`/`ledger_index` を抽出→署名・エンコード→`submit`。これにより、手動での Sequence 管理や Fee 見積もりが不要になる。
- **対応 TX 種別**：`Payment`（XRP + IOU）、`AccountSet`（SetFlag/ClearFlag/Domain/TickSize/TransferRate）、`SetRegularKey`（バックエンド準備済、UI はキー生成待ち）、`EscrowCreate`（バックエンド準備済）、`OfferCreate`（バックエンド準備済、compact spec `XRP:drops` / `CUR:issuer:value`）。`signing.rs` に各 `create_and_sign_*` 関数完備。`poll.rs` に `submit_*_transaction` 完備。

---

## 7. 既知の設計上注意点

実装リスク・負債の一覧は [`agent/DESIGN_ISSUES.md`](agent/DESIGN_ISSUES.md) と [`agent/RISK_REGISTER.md`](agent/RISK_REGISTER.md)。運用上のメモ:

- `start_poll_task` は `tokio::spawn` でポーリング async タスクを起動する。引数は `PollContext` 構造体で受け取る。
- WS 起点の更新は過剰ポーリングを避けるため `MIN_POLL_INTERVAL` で間引き、通常の定期ポーリングは設定値 `poll_interval_ms` を尊重する。
- `splash.rs` は起動スプラッシュとして使用され、`App` の `splash` フィールドに組み込まれている。`Mode::Splash` で表示制御。`Action::Tick` ごとに ASCII 行ウェーブ・ロゴ領域 80% 幅での折り返し・接続行ドット・quit ヒント括弧の 4 系統のアニメを更新する。接続先の表示はマージ後の `config.xrpl.rpc_server`（未設定時は `xrplcluster.com` の既定ホスト）で、`XRPL_RPC_SERVER` は `Config::new()` で `[xrpl] rpc_server` より優先して取り込まれる。
