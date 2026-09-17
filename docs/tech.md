# tech.md

**Role:** 技術スタック・API 連携（依存クレート、環境変数、XRPL/Flare RPC の接続面）。

## 1. 言語・ランタイム

- 言語: Rust
- Edition: 2024
- ツールチェーン: `rust-toolchain.toml` で `channel = "stable"`（components: `rustfmt`, `clippy`）。MSRV 相当は `Cargo.toml` の `rust-version = "1.94.1"`（alloy 2.4.2 の MSRV に追従。実ビルドは stable 強制のため参考値）。
- 非同期ランタイム: Tokio（`Cargo.toml` は `1`、`Cargo.lock` 解決 `1.53.1`、`features = ["macros", "rt-multi-thread", "sync", "time", "net", "io-util"]`。`full` は使わない）
- 対象OS: リリース対象は [`.github/workflows/cd.yml`](../.github/workflows/cd.yml)（README Requirements が引用）。現確認は macOS arm64。

## 2. 主要ライブラリ

### UI / CLI

- `ratatui`（`Cargo.toml` `0.30`、`Cargo.lock` `0.30.2`; `serde`, `macros`）
- `crossterm`（`Cargo.toml` `0.29`、`Cargo.lock` `0.29.0`; `serde`, `event-stream`）
- `strum`（`Cargo.toml` `0.28`、`Cargo.lock` `0.28.0`; `derive` — `Action` 表示など）
- `signal-hook`（`Cargo.toml` `0.4`、`Cargo.lock` `0.4.4` — `SIGTSTP` 処理）
- `clap`（`Cargo.toml` `4`、`Cargo.lock` `4.6.6`; `derive`, `cargo`, `wrap_help`, `unicode`, `string`）
- `ratatui-image`（`Cargo.toml` `11`、`crossterm`, `image-defaults`）— Kitty / Sixel / iTerm2 / halfblocks image preview。
- `image`（`Cargo.toml` `0.25`、PNG/JPEG/GIF/WebP）— NFT image decode。

### XRPL / 通信
- `reqwest`（`Cargo.toml` `0.13`、`Cargo.lock` 直接依存 `0.13.5`; `features = ["json", "stream"]` — JSON-RPC と bounded NFT metadata/image streaming）。`xrpl-rust` 経路では **`reqwest 0.12.x` がロックに併存**しうる（解像は `Cargo.lock` を正とする）。
- `xrpl-rust`（`Cargo.toml` `1.1`、`Cargo.lock` `1.3.0`）
- `kobe-xrpl = 3.4` / `kobe-primitives = 3.4` — BIP39 secp256k1 mnemonic → BIP44 `m/44'/144'/0'/0/0` account derivation。
- `tokio-tungstenite = (xrpl-rust 経由)`
- `url = 2`
- `base64 = 0.23` — XRPLF dUNL blob / validator manifest decode（`base64::engine::general_purpose::STANDARD`）
- `hex = 0.4` — seed / domain / validator key の hex encode/decode
- `time = 0.3`（`default-features = false, features = ["alloc"]`）— Ripple epoch → `format_ripple_time_utc` の暦変換
- Payment 署名はクレート公開 API の `wallet::Wallet` + `transaction::sign`（`models::transactions::payment::Payment`）を利用し、送信時は `binarycodec::encode` した `tx_blob` に変換する。既存の XRPL family seed と BIP39 mnemonic（secp256k1 only）は `SigningCredential` に統合する。
### Flare / EVM

- `alloy`（`Cargo.toml` `2`; `features = ["essentials"]` — HTTP provider + local signer。`full` は使わない）
  - `ContractRegistry` 経由で `FtsoV2` / `AssetManagerFXRP` アドレスを解決。
  - FTSOv2: `getFeedById(bytes21)`（`src/flare.rs` → `fetch_ftso_prices`）。
  - FXRP Direct Mint: `getCoreVault()` / `getDirectMintingExecutorFeeUBA()`（`fetch_fxrp_direct_mint_info`）。
  - Flare wallet read: `eth_getBalance` + `IAssetManager.fAsset()` → `IERC20.balanceOf` / `decimals`（`fetch_flare_wallet_balance`）。
  - RPC 解決: `FLARE_RPC_URL` env > `[flare] network` preset（`flare` / `songbird` / `coston2`）。実装: `app.rs::resolve_flare_rpc_url`。

### 設定・シリアライズ

- `config`（`Cargo.toml` `0.15`、`Cargo.lock` `0.15.25`; default-features off, `toml`/`convert-case` — `ron`/`async` なし）
- `directories`（`Cargo.toml` `6`、`Cargo.lock` `6.0.0` — `ProjectDirs`）
- `serde`（`Cargo.toml` `1`、`Cargo.lock` `1.0.229`、`derive`）
- `serde_json`（`Cargo.toml` `1`、`Cargo.lock` `1.0.151`）
- `json5`（`Cargo.toml` `1`、`Cargo.lock` 直接依存 `1.3.1` — リポジトリ直下の埋め込み `config.json5` のパース）

### 監視性・障害対応

- `tracing`（`Cargo.toml` `0.1`、`Cargo.lock` `0.1.44`）
- `tracing-subscriber`（`Cargo.toml` `0.3`、`Cargo.lock` `0.3.23`; `env-filter`）
- `tracing-error`（`Cargo.toml` `0.2`、`Cargo.lock` `0.2.1`）
- `color-eyre`（`Cargo.toml` `0.6`、`Cargo.lock` `0.6.5`）

### セキュリティ

- `secrecy = 0.10.x`（`features = ["serde"]`）
  - 署名資格情報（family seed / BIP39 mnemonic）を `SecretString`（= `SecretBox<str>`）でラップし、`Debug`/`Display` で秘密値をマスク。drop 時にゼロ化する。
  - 読み込み優先順: CLI (`--seed` / `--mnemonic`) > 対応する env var (`XRPL_SEED` / `XRPL_MNEMONIC`) > `config.toml [xrpl.signing]` の `seed` / `mnemonic`。
  - `config.toml` に秘密値を書く場合は平文ディスク保存のリスクがあるため、env var を推奨。

### 依存解決メモ（リンク安定化）

- `critical-section = 1.2.0`（`features = ["std"]`）
  - `embassy-sync` 系経由で必要になる critical section 実装を `std` で提供する目的。

## 3. ビルド設定

- ビルドスクリプト: `build.rs`
- build-dependencies:
  - `anyhow`（`Cargo.toml` `1`、`Cargo.lock` `1.0.104`）
  - `vergen-gix`（`Cargo.toml` `9`、`Cargo.lock` `9.1.0`; `build`）
- Release profile:
  - `codegen-units = 1`
  - `lto = "fat"`
  - `opt-level = "s"`
  - `strip = true`
  - `panic` は unwind（color-eyre のパニックレポート）

## 4. 環境変数

| 変数 | 内容 |
|---|---|
| `XRPL_NETWORK` | ネットワークプリセット（`mainnet` / `testnet` / `devnet` / `xahau` / `xahau-test`） |
| `XRPL_RPC_SERVER` | カスタム RPC エンドポイント（ネットワークプリセットを上書き） |
| `XRPL_WS_SERVER` | カスタム WS エンドポイント（ネットワークプリセットを上書き） |
| `XRPL_SEED` | XRPL family seed（署名用） |
| `XRPL_MNEMONIC` | BIP39 mnemonic（secp256k1、署名用） |
| `FLARE_RPC_URL` | Flare RPC URL（`[flare] network` preset より優先） |
| `FLARE_FEEDS` | Overview / Market 用 Flare フィード一覧（カンマ区切り） |
| `FLARE_FEED` | 旧互換の単一フィード指定（`FLARE_FEEDS` 未設定時のみ） |
| `FLARE_EVM_KEY` | FXRP C3 `executeDirectMinting` 用 Flare EVM 鍵（`[flare.fassets] execute=true` 時のみ。既定 env 名は設定で変更可） |
| `LAZYXRP_CONFIG` | 設定ディレクトリの明示オーバーライド（`..` は拒否） |
| `LAZYXRP_DATA` | データディレクトリの明示オーバーライド |
| `LAZYXRP_LOG_LEVEL` | ファイルログの既定フィルタ（`tracing-subscriber` の `EnvFilter`） |

起動順は `Config::new()` → `logging::init(config.resolved_data_dir())`。`Config::new()` は `XRPL_SEED` / `XRPL_MNEMONIC` に続けて `XRPL_NETWORK` / `XRPL_RPC_SERVER` / `XRPL_WS_SERVER` を読み、設定ファイルの同項目より優先して `xrpl` に反映する。両方の署名資格情報が設定された場合は起動時にエラーにする。

```
--network CLI > XRPL_NETWORK > config.toml [xrpl] network > デフォルト (mainnet)
--server  CLI > XRPL_RPC_SERVER > config.toml rpc_server  > Network::rpc_url()
FLARE_RPC_URL env > config.toml [flare] network preset > Flare mainnet default
FLARE_FEEDS env > FLARE_FEED env > DEFAULT_FLARE_FEEDS (`FXRP/USD`, `FLR/USD`, `BTC/USD`, `ETH/USD`)
```

`config.toml` の Flare キー（詳細は [`architecture.md`](architecture.md) §6）:

| Key | Values | Notes |
|-----|--------|-------|
| `[flare] network` | `flare` / `songbird` / `coston2` | FTSO / FXRP / wallet read の RPC preset |
| `[flare] display` | `full` / `compact` / `off` | Overview / Market の Flare パネル密度 |
| `[flare.wallet] address` | `0x…` EVM | Overview 読み取りパネル（invalid hex は起動失敗） |
| `[flare.fassets] execute` | `true` / `false` | C3 `executeDirectMinting` ゲート（既定 `false`） |

TUI セッション内の `<Ctrl-n>` は `Network::next_network()` で XRPL ネットワークを循環（config へは永続化しない）。`xahau` は mainnet 同等の書き込みガード（`--yes`）対象。

**常駐運用:** 既定 `poll_interval_ms = 5000` はインタラクティブ向け。24/7 で公開 RPC（`xrplcluster.com` 等）を叩く場合は `15000`〜`30000` を推奨（[`architecture.md`](architecture.md) §3）。dUNL は 10 分 TTL キャッシュ、Flare registry 解決はプロセス内キャッシュ。

## 5. 開発コマンド

- 依存解決: `cargo build`
- タスクランナー: ルート `mise.toml`（`mise run <task>`）。`check` / `fmt` / `lint`（fmt+clippy が CI 基準）/ `test` / `test-serial` / `doc` / `audit`（cargo-audit は CLI フラグで RUSTSEC-2026-0235 を無視 — CI と同一、cargo-deny の無視リストは `deny.toml`）/ `update`（`cargo update` 後に check + audit 再掃討）/ `clean`（target/ と benchmark 出力を削除）/ `bench*` / `tag-push`（タグ作成前に 4 ゲート実行）。ツール実体は `[tools]`（rust は [`rust-toolchain.toml`](../rust-toolchain.toml) と同一チャンネル `stable`、`hyperfine`、`cargo-audit`、`cargo-deny`、`cargo-bloat`）。
- コンパイルチェック: `cargo check`
- ローカルインストール（任意）: ルート `./install.sh`（**HTTP 取得には `curl` または `wget` のいずれかが必須**）。英語プロンプト。`--help` でオプション確認（`--method cargo|binary`、`--install-rust` / `--no-install-rust` など）。ソースビルドはクローン済みツリーのルートから（`Cargo.toml` / `rust-toolchain.toml` と同階）。`curl | bash` だけのとき rustup で入れる既定ツールチェーンは **リポにある `rust-toolchain.toml` の `channel` を読めるかぎりそれ**で揃える（読めずに素のstdin経路なら **`stable`** フォールバック）。GitHub Releases REST は **公開 API の無認証だと環境によりレート制限**になりうるので、任意で **`GITHUB_TOKEN` / `GITHUB_API_TOKEN`**。バイナリ配置は INSTALL_DIR 上の **`*.partial.*` にコピーしてから `mv`（失敗や中断時の掃除は EXIT の `cleanup`）。**手動アンインストール**は `./install.sh --uninstall-help`（**PATH 上のバイナリから** `lazyxrp --self-uninstall` / `--yes` も可。バイナリ／`cargo uninstall` に加え、任意でユーザ設定・データ directory の削除例 Linux/macOS 別、`LAZYXRP_CONFIG` / `LAZYXRP_DATA` と `config.toml` の `data_dir` / `config_dir` 上書きの注意。README の Uninstall と同様）。TTY は対話 + アニメ; `-q` または非 TTY は非対話。ダウンロードは `curl` 優先（無ければ `wget`）でリトライ／タイムアウトあり。GitHub のタグ／コミット SHA 解決は **`jq` があれば優先**（無ければ従来の grep/sed）。`BINARY_INSTALL=1 ./install.sh -q` でプリビルト優先の例は従来どおり。または **[mise](https://mise.jdx.dev/)** `mise run install`（`mise.toml` のタスク経由; 詳細は `README.md`）
- 実行（TUI）: `cargo run --bin lazyxrp -- --account <r-address>`
- seed / mnemonic 指定実行（非推奨 — argv/history に露出。`XRPL_SEED` / `XRPL_MNEMONIC` か `config.toml` を推奨）: `cargo run --bin lazyxrp -- --account <r-address> --mnemonic "abandon ... about"`
- スクリプト CLI（例）:
  - `cargo run --bin lazyxrp -- -x info`
  - `cargo run --bin lazyxrp -- -x account <r-address>`
  - `cargo run --bin lazyxrp -- -x summary --account <r-address>`
  - `cargo run --bin lazyxrp -- -x book --base XRP --quote USD --issuer <r-issuer> --limit 5`
  - `cargo run --bin lazyxrp -- -x nfts <r-address>`
  - `cargo run --bin lazyxrp -- -x lines <r-address>`
  - `cargo run --bin lazyxrp -- -x amm --asset1 XRP --asset2 USD --issuer2 <r-issuer>`
  - `cargo run --bin lazyxrp -- -x tx-history <r-address> --limit 20`
- ネットワーク指定例:
  - `cargo run --bin lazyxrp -- --network testnet --account <r-address>`
  - `XRPL_NETWORK=testnet cargo run --bin lazyxrp -- -x info`

## 6. 技術的制約・注意点

- `tokio::spawn` 周辺で Rust の既知制限（issue #100013）に当たるケースがある。
- 現行実装では XRPL バックグラウンドタスク起動経路を調整してビルド安定化を優先している。
- macOS で一部依存に「newer macOS version でビルドされた object」のリンク警告が出る場合があるが、致命エラーは別途切り分ける。
