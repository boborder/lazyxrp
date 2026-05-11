# tech.md

## 1. 言語・ランタイム

- 言語: Rust
- Edition: 2024
- 非同期ランタイム: Tokio (`tokio = 1.40.0`, `features = ["full"]`)
- 対象OS（現確認）: macOS arm64

## 2. 主要ライブラリ

### UI / CLI

- `ratatui = 0.30.0`（`serde`, `macros`）
- `crossterm = 0.28.1`（`serde`, `event-stream`）
- `clap = 4.5.20`（`derive`, `cargo`, `wrap_help`, `unicode`, `string`, `unstable-styles`）

### XRPL / 通信

- `xrpl-rust = 1.1`
- `reqwest = 0.12`（`features = ["json"]` — `account_tx` 等で直接 JSON-RPC を投げる用途）
- `tokio-tungstenite = (xrpl-rust 経由)`
- `url = 2`
- Phase 3 の Payment 署名はクレート公開 API の `wallet::Wallet` + `transaction::sign`（`models::transactions::payment::Payment`）を利用し、送信時は `binarycodec::encode` した `tx_blob` を `submit` に渡す（旧 `xrpl::crypto::Keypair` 系は 1.1 ではない）。

### 設定・シリアライズ

- `config = 0.14.0`
- `serde = 1.0.211`（`derive`）
- `serde_json = 1.0.132`
- `json5 = 0.4.1`

### 監視性・障害対応

- `tracing = 0.1.40`
- `tracing-subscriber = 0.3.18`（`env-filter`, `serde`）
- `tracing-error = 0.2.0`
- `color-eyre = 0.6.3`
- `better-panic = 0.3.0`
- `human-panic = 2.0.2`

### セキュリティ

- `secrecy = 0.10.x`（`features = ["serde"]`）
  - 署名シードを `SecretString`（= `SecretBox<str>`）でラップし、`Debug`/`Display` でアドレスをマスク。drop 時にシードをゼロ化する。
  - シード読み込み優先順: `XRPL_SEED` env var > `config.toml [xrpl.signing] seed`。
  - `config.toml` にシードを書く場合は平文ディスク保存のリスクあり。env var を推奨。

### 依存解決メモ（リンク安定化）

- `critical-section = 1.2.0`（`features = ["std"]`）
  - `embassy-sync` 系経由で必要になる critical section 実装を `std` で提供する目的。

## 3. ビルド設定

- ビルドスクリプト: `build.rs`
- build-dependencies:
  - `anyhow = 1.0.90`
  - `vergen-gix = 9.1.0`（`build`, `cargo`）
- Release profile:
  - `codegen-units = 1`
  - `lto = true`
  - `opt-level = "s"`
  - `strip = true`

## 4. 環境変数

| 変数 | 内容 |
|---|---|
| `XRPL_NETWORK` | ネットワークプリセット（`mainnet` / `testnet` / `devnet`） |
| `XRPL_RPC_SERVER` | カスタム RPC エンドポイント（ネットワークプリセットを上書き） |
| `XRPL_WS_SERVER` | カスタム WS エンドポイント（ネットワークプリセットを上書き） |
| `XRPL_SEED` | 署名用シード（Phase 3 書き込み系 TX 準備） |

**ネットワーク・エンドポイントの優先順位:**

```
--network CLI > XRPL_NETWORK > config.toml [xrpl] network > デフォルト (mainnet)
--server  CLI > XRPL_RPC_SERVER > config.toml rpc_server  > Network::rpc_url()
```

## 5. 開発コマンド

- 依存解決: `cargo build`
- コンパイルチェック: `cargo check`
- 実行（watch）: `cargo run --bin lazyxrp -- watch --account <r-address>`
- seed 指定実行: `cargo run --bin lazyxrp -- watch --account <r-address> --seed sXXX...`
- CLI実行（例）:
  - `cargo run --bin lazyxrp -- info`
  - `cargo run --bin lazyxrp -- account <r-address>`
  - `cargo run --bin lazyxrp -- summary --account <r-address>`
  - `cargo run --bin lazyxrp -- book --base XRP --quote USD --issuer <r-issuer> --limit 5`
  - `cargo run --bin lazyxrp -- nfts <r-address>`
  - `cargo run --bin lazyxrp -- lines <r-address>`
  - `cargo run --bin lazyxrp -- amm --asset1 XRP --asset2 USD --issuer2 <r-issuer>`
  - `cargo run --bin lazyxrp -- txhistory <r-address> --limit 20`
- ネットワーク指定例:
  - `cargo run --bin lazyxrp -- --network testnet watch --account <r-address>`
  - `XRPL_NETWORK=testnet cargo run --bin lazyxrp -- info`

## 6. 技術的制約・注意点

- `tokio::spawn` 周辺で Rust の既知制限（issue #100013）に当たるケースがある。
- 現行実装では XRPL バックグラウンドタスク起動経路を調整してビルド安定化を優先している。
- macOS で一部依存に「newer macOS version でビルドされた object」のリンク警告が出る場合があるが、致命エラーは別途切り分ける。
