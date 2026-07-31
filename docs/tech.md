# tech.md

## 1. 言語・ランタイム

- 言語: Rust
- Edition: 2024
- ツールチェーン: `rust-toolchain.toml` で `channel = "stable"`（components: `rustfmt`, `clippy`）。MSRV 相当は `Cargo.toml` の `rust-version = "1.91"`（依存クレート・エディションに追従する下限）。
- 非同期ランタイム: Tokio（`Cargo.toml` は `1`、`Cargo.lock` 解決 `1.52.3`、`features = ["full"]`）
- 対象OS（現確認）: macOS arm64

## 2. 主要ライブラリ

### UI / CLI

- `ratatui`（`Cargo.toml` `0.30`、`Cargo.lock` `0.30.0`; `serde`, `macros`）
- `crossterm`（`Cargo.toml` `0.29`、`Cargo.lock` `0.29.0`; `serde`, `event-stream`）
- `strum`（`Cargo.toml` `0.28`、`Cargo.lock` `0.28.0`; `derive` — `Action` 表示など）
- `signal-hook`（`Cargo.toml` `0.4`、`Cargo.lock` `0.4.4` — `SIGTSTP` 処理）
- `clap`（`Cargo.toml` `4`、`Cargo.lock` `4.6.1`; `derive`, `cargo`, `wrap_help`, `unicode`, `string`, `unstable-styles`）

### XRPL / 通信

- `xrpl-rust = 1.1`
- `reqwest`（`Cargo.toml` `0.13`、`Cargo.lock` 直接依存 `0.13.3`; `features = ["json"]` — `account_tx` 等で JSON-RPC）。`xrpl-rust` 経路では **`reqwest 0.12.x` がロックに併存**しうる（解像は `Cargo.lock` を正とする）。
- `tokio-tungstenite = (xrpl-rust 経由)`
- `url = 2`
- Phase 3 の Payment 署名はクレート公開 API の `wallet::Wallet` + `transaction::sign`（`models::transactions::payment::Payment`）を利用し、送信時は `binarycodec::encode` した `tx_blob` を `submit` に渡す（旧 `xrpl::crypto::Keypair` 系は 1.1 ではない）。

### Flare / EVM

- `alloy`（`Cargo.toml` `1`; `features = ["full"]`）
  - `ContractRegistry` 経由で `FtsoV2` アドレスを解決し、`getFeedById(bytes21)` で FTSOv2 フィードを取得。
  - Oracle タブ統合では Flare mainnet RPC を既定に使用する（フォールバックなし）。

### 設定・シリアライズ

- `config`（`Cargo.toml` `0.15`、`Cargo.lock` `0.15.22`）
- `directories`（`Cargo.toml` `6`、`Cargo.lock` `6.0.0` — `ProjectDirs`）
- `serde`（`Cargo.toml` `1`、`Cargo.lock` `1.0.228`、`derive`）
- `serde_json`（`Cargo.toml` `1`、`Cargo.lock` `1.0.149`）
- `json5`（`Cargo.toml` `1`、`Cargo.lock` 直接依存 `1.3.1` — リポジトリ直下の埋め込み `config.json5` のパース。`config` クレート経由などトランジティブに **`json5 0.4.x` が併存**しうる）

### 監視性・障害対応

- `tracing`（`Cargo.toml` `0.1`、`Cargo.lock` `0.1.44`）
- `tracing-subscriber`（`Cargo.toml` `0.3`、`Cargo.lock` `0.3.23`; `env-filter`, `serde`）
- `tracing-error`（`Cargo.toml` `0.2`、`Cargo.lock` `0.2.1`）
- `color-eyre`（`Cargo.toml` `0.6`、`Cargo.lock` `0.6.5`）
- `better-panic`（`Cargo.toml` `0.3`、`Cargo.lock` `0.3.0`）
- `human-panic`（`Cargo.toml` `2`、`Cargo.lock` `2.0.8`）

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
  - `anyhow`（`Cargo.toml` `1`、`Cargo.lock` `1.0.102`）
  - `vergen-gix`（`Cargo.toml` `9`、`Cargo.lock` `9.1.0`; `build`, `cargo`）
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
| `FLARE_RPC_URL` | Flare FTSOv2 RPC URL（既定: mainnet） |
| `FLARE_FEEDS` | Oracle タブ用 Flare フィード一覧（カンマ区切り） |
| `FLARE_FEED` | 旧互換の単一フィード指定（`FLARE_FEEDS` 優先） |
| `FLARE_EVM_KEY` | FXRP C3 `executeDirectMinting` 用 Flare EVM 鍵（`[flare.fassets] execute=true` 時のみ。既定 env 名は設定で変更可） |
| `LAZYXRP_CONFIG` | 設定ディレクトリの明示オーバーライド（`..` は拒否） |
| `LAZYXRP_DATA` | データディレクトリの明示オーバーライド |
| `LAZYXRP_LOG_LEVEL` | ファイルログの既定フィルタ（`tracing-subscriber` の `EnvFilter`） |

起動順は `Config::new()` → `logging::init(config.resolved_data_dir())`。`Config::new()` は `XRPL_SEED` に続けて `XRPL_NETWORK` / `XRPL_RPC_SERVER` / `XRPL_WS_SERVER` を読み、設定ファイルの同項目より優先して `xrpl` に反映する（スプラッシュの接続先表示もこのマージ後の値を使う）。Flare の `FLARE_RPC_URL` / `FLARE_FEEDS` は `App::run()` で読み取られ、Oracle タブの FTSOv2 ポーリングに適用される。ログファイルはマージ後の `data_dir`（設定ファイルトップレベルキー、または空なら環境変数・XDG・フォールバック）配下に作成される。設定ファイル自体の探索パスはブートストラップの `config_dir()`（`LAZYXRP_CONFIG` / XDG 等）で決まる。

```
--network CLI > XRPL_NETWORK > config.toml [xrpl] network > デフォルト (mainnet)
--server  CLI > XRPL_RPC_SERVER > config.toml rpc_server  > Network::rpc_url()
```

## 5. 開発コマンド

- 依存解決: `cargo build`
- コンパイルチェック: `cargo check`
- ローカルインストール（任意）: ルート `./install.sh`（**必須は `curl` のみ**）。英語プロンプト。`--help` でオプション確認（`--method cargo|binary`、`--install-rust` / `--no-install-rust` など）。ソースビルドはクローン済みツリーのルートから（`Cargo.toml` / `rust-toolchain.toml` と同階）。`curl | bash` だけのとき rustup で入れる既定ツールチェーンは **リポにある `rust-toolchain.toml` の `channel` を読めるかぎりそれ**で揃える（読めずに素のstdin経路なら **`stable`** フォールバック）。GitHub Releases REST は **公開 API の無認証だと環境によりレート制限**になりうるので、任意で **`GITHUB_TOKEN` / `GITHUB_API_TOKEN`**。バイナリ配置は INSTALL_DIR 上の **`*.partial.*` にコピーしてから `mv`（失敗や中断時の掃除は EXIT の `cleanup`）。**手動アンインストール**は `./install.sh --uninstall-help`（**PATH 上のバイナリから** `lazyxrp --self-uninstall` / `--yes` も可。バイナリ／`cargo uninstall` に加え、任意でユーザ設定・データ directory の削除例 Linux/macOS 別、`LAZYXRP_CONFIG` / `LAZYXRP_DATA` と `config.toml` の `data_dir` / `config_dir` 上書きの注意。README の Uninstall と同様）。TTY は対話 + アニメ; `-q` または非 TTY は非対話。ダウンロードは `curl` にリトライ／タイムアウトあり。GitHub のタグ／コミット SHA 解決は **`jq` があれば優先**（無ければ従来の grep/sed）。`BINARY_INSTALL=1 ./install.sh -q` でプリビルト優先の例は従来どおり。または **[mise](https://mise.jdx.dev/)** `mise run install`（`.mise.toml` のタスク経由; 詳細は `README.md`）
- 実行（watch）: `cargo run --bin lazyxrp -- watch --account <r-address>`
- seed 指定実行（非推奨 — argv/history に露出。`XRPL_SEED` か `config.toml` を推奨）: `cargo run --bin lazyxrp -- watch --account <r-address> --seed sXXX...`
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
