# requirements-ja.md

[English](requirements.md)

**Role:** 要件定義・ゴール設計（FR/NFR の SSOT）。

本書はプロダクト**要件**（what）を扱う。振る舞い設計は [`architecture-ja.md`](architecture-ja.md)、操作・見た目は [`DESIGN.md`](../DESIGN.md)、計画は [`ROADMAP-ja.md`](../ROADMAP-ja.md) を参照。

## 1. 目的

- XRPL の状態を TUI で継続監視し、取引判断に必要な情報を素早く確認できるようにする。
- 個人運用で使える実用性と、公開プロジェクトとして再利用しやすい構成を両立する。

## 2. 対象ユーザー

- 主対象: 開発者本人（自分の運用で日常利用）
- 副対象: 公開リポジトリを使う外部ユーザー

## 3. 機能要件（Functional Requirements）

### FR-01: 監視モード

- `lazyxrp`（引数なし）で TUI を起動し、XRPL 情報を継続表示すること。
- 既定では設定ファイルのアカウントを監視し、`--account` で上書きできること。

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
- CLI コマンド `tx-history <address> [--limit N]` でも出力できること。

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

### FR-17: Flare Smart Account（proof-based instruction flow）

- XRPL アドレス（署名アカウント、なければ watch アカウント）の Flare パーソナルアカウントを TUI のみで確認・操作できること。
- 読み取り（Flare RPC、読み取り専用）: `MasterAccountController`（FlareContractRegistry 解決）の `getPersonalAccount` / `getNonce` / `getXrplProviderWallets` / `getInstructionFee(1)`（FXRP transfer）/ `getInstructionFee(2)`（FXRP redeem、いずれも XRP drops）、パーソナルアカウントの FXRP `balanceOf`（トークンの最小単位）、FXRP トークンの `decimals()`（18 超は拒否）、デプロイ済みか（code 有無）。FXRP / Flare パネル（FXRP Direct Mint）に表示し（残高は `decimals()` で整形）、`[flare] display = off` と Overview タブ限定は FTSO / FXRP と同じ扱いとすること。
- 書き込み: Wallet composer `a` で `0x01` FXRP transfer（value = FXRP の最小単位。composer は 10 進の FXRP 額を、読み取った `decimals()` を使い整数演算のみで最小単位に変換する。小数桁数超過・`decimals` 未取得は拒否、受取先 = 0x EVM アドレス）と `0x02` FXRP redeem（value = lots）を XRPL `Payment`（operator 宛、`DestinationTag` なし、`Amount` = `getInstructionFee`（XRP drops）、32-byte Memo）で送れること。実行（FDC 証明 + `executeInstruction`）は operator が Flare 上で行う。
- operator は Flare RPC の単一応答のため、`[flare.smart_accounts] expected_operator`（運用者が別経路で確認）と一致しない場合は署名を拒否すること（未設定も拒否）。mainnet は `--yes` 必須、simulate 前に署名・送信しないこと。
- C2（FXRP Mint composer）の受取先は、署名アカウント自身の personal account を既定値にすること（読み取りが署名アカウントのもので 300 秒以内のときのみ。別アカウント・古い読み取りは空欄）。composer は受取先を全桁表示し、personal account は Flare RPC の応答である旨と explorer での照合を促すこと。XRPL 署名のみで「XRP → 自分の personal account の FXRP」を完結できること（他の Flare アドレスを要求しない）。
- 実装済み（CLI のみ）: vault instruction（`-x sa-vault`）、memo 命令 `0xE0` / `0xE1` / `0xE2` / `0xD0` / `0xD1`（`-x sa-memo`）、カスタム instruction `0xFE`（`-x sa-call`）、FDC proof 取得 + `executeDirectMinting[WithData]`（`-x fxrp-execute`、Flare EVM 鍵が必要）。`sa-memo` は検証後に公開 executor の finalize を最大 5 分待ち（`--no-wait` で省略）、未完了なら自分で finalize する手順を示すこと。待機は支払いの validated 後なので、Flare 読み取りの失敗でもコマンドを失敗させない（stderr に `sa-status` の再確認手順、exit 0）。executor が固定されている場合は `0xE0` / `0xE1` / `0xE2` で待たず、固定された executor からの finalize を案内する。`-x sa-status <tx>` は `isTransactionIdUsed` の読み取りのみ（plain の recipient memo mint は対象外、Coston2 で確認）。`sa-call`（`0xFE`）は userOp バイトを持つ executor しか finalize できないため、公開 executor を待たず、その旨と手順を示すこと。
- finalize は Flare 上の第三者（executor）に依存する。公開 executor が mainnet で動いている（2026-10-24 の読み取り調査）ことは、lazyxrp が送る少額・低手数料の支払いが拾われることを保証しない（executor の手数料下限は未測定）。拾われない場合に XRP は Core Vault に残り、自分で finalize するには EVM 鍵・FLR（mainnet の attestation 約 20 FLR + gas）が要る。
- Smart Account の状態は永続化しない。Smart Account に関するものは何も保存しない。表示する情報（personal account、nonce、operator wallets、instruction fee、FXRP 残高、vault ポジション）は Overview の poll ごとに Flare から読み取り、メモリ上にのみ保持する（`SmartAccountInfo`。300 秒より古い、または別ネットワークのスナップショットは拒否し、`verify_*_terms` は署名直前に view を再読込する。TC-363）。`getPersonalAccount(xrplAddress)` は決定的なので覚えておく必要がなく、保存したコピーは古くなる（残高・nonce・operator・手数料は変わる）うえ、これらのガードを迂回してしまう。永続化する価値があるのは、S-019 を閉じるための、ユーザーが検証した `[flare.smart_accounts] expected_personal_account` の pin（`expected_operator` と同様）だけだが、未実装（ROADMAP F5-b、P3）。lazyxrp が永続化するのはユーザーが書いた設定ファイルとログだけで、ウォレット・Smart Account・履歴のキャッシュは持たない。
- 対象外（ROADMAP）: `0xFF`（memo 内 userOp。公開 executor が中継できる唯一のカスタム instruction で、XRPL 署名のみの用途では優先度が上がる）、`sa-memo` / `sa-call` の TUI 化。

### FR-18: 宣言的 TX spec による書き込みと coverage 管理

- XRPL トランザクション種別を `TxSpec`（フィールド仕様・検証・unsigned `tx_json`・型付き署名）として宣言的に追加でき、1 つの共通経路（TUI composer「Transaction」`m`・`-x tx <TYPE> Field=value…`・`-x nft-mint` / `-x nft-burn`）で書き込めること。共通経路は既存の書き込みガード（network 照合・mainnet `--yes`・simulate `tesSUCCESS` 必須かつ要求への束縛・Fee 上限・submit 直列化・`--watch` 拒否）を重複実装せず再利用すること。
- NFT 5 種（NFTokenMint / Burn / CreateOffer / AcceptOffer / CancelOffer）を対象とする。URI は平文 ≤ 256 bytes を hex 化、TransferFee は 0..=50000（0 超は transferable 必須）、id は 64 hex、金額は `XRP:drops` / `CUR:issuer:value`。不可逆な操作（Burn・AcceptOffer）は composer で型付き確認（`BURN` / `ACCEPT`）を必須とすること。Assets タブ `x` で選択 NFT の Burn composer を NFTokenID 入力済みで開けること。
- 対応状況は `src/coverage.rs` を SSOT として、種別ごとの read / TUI write / CLI write / status（Existing・Spec・Planned・Unsupported）を管理し、ROADMAP.md「XRPL coverage」表および README の件数と自動テストで一致を強制すること（`lazyxrp -x coverage` が表を出力）。

### FR-13: シークレット管理（Phase 3 準備基盤）

- 署名用資格情報は次の優先順位で読み込めること。
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
- Phase 3（書き込み系 TX）: **実装済み**。TUI コンポーザーと CLI で Payment / AccountSet / SetRegularKey / OfferCreate / OfferCancel / TrustSet / FXRP Mint / FXRP Execute / AMMDeposit（two-asset）/ AMMWithdraw（全額 or LP 量）/ AccountDelete（TUI は `DELETE` 入力の確認必須）に対応。EscrowCreate / EscrowFinish / EscrowCancel は TUI コンポーザーのみ（時間条件のみ、crypto-condition 非対応）。残りは本番動作確認とエッジケース対応。
- Phase 3b（宣言的 TX spec、FR-18）: **NFT 5 種を実装**。共通経路（composer `m` / `-x tx`）。以降の family（Check / Ticket / DepositPreauth / SignerList / PaymentChannel / AMM）は `txspec/` に 1 ファイル + `FAMILIES` に 1 行で追加（`architecture-ja.md` §2.1）。

## 7. スコープ注記: 未対応・計画中

本節はポインタのみで、新しい要件は含まない。正は ROADMAP。

- 書き込みできない XRPL トランザクション種別は次のとおり。Planned は Vault* ファミリー、Unsupported は `xrpl-rust` に型付きモデルがないもの、XLS-66 Loan*、witness attestations、ConfidentialMPT*。ROADMAP「XRPL coverage」を参照（表の SSOT は `ROADMAP.md`、`lazyxrp -x coverage` で生成）。
- 見送り中の Flare 項目: `0xFF`、`sa-memo` / `sa-call` の TUI 化、FCC/PMW、OFT、独自 executor bot。[`ROADMAP-ja.md`](../ROADMAP-ja.md) の Flare セクションを参照。
