# architecture.md — システム・挙動設計

[English](architecture.md)

**役割:** 詳細設計・スキーマ設計（ネットワーク・設定・データフロー・パネル契約の SSOT）。UI キー／レイアウトは [`DESIGN.md`](../DESIGN.md)。

[`requirements-ja.md`](requirements-ja.md) を起点とする。テストは [`test-ja.md`](test-ja.md) が本書および FR/NFR id にトレースする。

**ここに無いもの:** キーバインド、配色、レイアウト分割、スクロール／モーダル操作。SSOT はルートの [`DESIGN.md`](../DESIGN.md)。

| トピック | SSOT |
|-------|------|
| 見た目、キー、フォーカス、テーマ | [`DESIGN.md`](../DESIGN.md) |
| FR/NFR id | [`requirements-ja.md`](requirements-ja.md) |
| モジュール、チャネル、起動／submit フロー | `src/app.rs`, `src/xrpl/poll.rs`, `src/xrpl/submit.rs`, [`directory-ja.md`](directory-ja.md) §3 |
| ルール I-1〜I-11 | [`security-ja.md`](security-ja.md)（ガードは `src/`） |
| ディレクトリ構成 | [`directory-ja.md`](directory-ja.md) |
| TX 詳細パーサのレジストリ | 後述 §5.2.1、`src/components/shared/tx_detail/` |
| セキュリティ監査 S-xxx / リスク R-xxx | [`security-ja.md`](security-ja.md) |
| ドキュメント索引 | [`AGENTS.md`](../AGENTS.md) |
| Graphify | [`graphify-out/GRAPH_REPORT.md`](../graphify-out/GRAPH_REPORT.md) |

## システム境界

lazyxrp はローカルで動く**単一の Rust バイナリ**である。読み取りには公開 **XRP Ledger** エンドポイント（JSON-RPC / WebSocket）に接続し、設定時は署名付き submit も行う。任意で **Flare** RPC 読み取り（FTSO、FXRP AssetManager）と、明示的に有効化した場合のみ C3 execute を行う。

## ランタイム構造（単一プロセス）

| 領域 | 役割 | 主なコード |
|------|------|--------------|
| Watch (TUI) | ダッシュボード、パネル、キーボード操作 | `app.rs`, `tui.rs`, `components/` |
| CLI ランナー | 非対話サブコマンド | `cli.rs`, `xrpl/cli_exec.rs` |
| XRPL 連携 | RPC、WebSocket、poll タスク → `Action`、書き込み経路 | `xrpl/client.rs`, `ws.rs`, `poll.rs`, `submit.rs` |
| 設定・署名 | マージ済み `config.toml`、認証情報 | `config.rs`, `signing.rs` |
| Flare | FTSO / FXRP 読み取り（＋任意の execute） | `flare.rs`, `poll.rs`, `submit.rs` |

モジュール配線の要約: [`directory-ja.md`](directory-ja.md) §3。タブ分割とキー: [`DESIGN.md`](../DESIGN.md)。

## 1. 起動モードとスタック（要約）

| 入口 | 用途 | 例 |
|-------|---------|---------|
| `lazyxrp` | **TUI**（既定） | `lazyxrp --account <ADDR>` |
| `lazyxrp -x <subcmd>` | **スクリプト CLI** | `lazyxrp -x info` |
| `rc <subcmd>` | 同上（シェル alias） | `alias rc='lazyxrp -x'` |
| `rp` | lookup 専用バイナリ | `rp -t <txid\|ADDR>` |
| `lazyxrp --self-uninstall` | アンインストール | `logging::init` 前に終了 |

- エントリ: `src/lib.rs`（TUI → `App::new` → `run`、`-x` CLI → `xrpl::execute_cli_command(...)`）。
- 裸サブコマンド（`-x` なし）はエラーで `lazyxrp -x <name>` を案内。`watch` サブコマンドは削除（TUI は引数なし `lazyxrp`）。CLI の書き込み（`send`・`offer-create`・`offer-cancel`・`amm-deposit`・`amm-withdraw`・`account-delete`）も **network 照合 → simulate → sign → submit → validated 待ち**（Sequence/Fee は simulate 由来。LastLedgerSequence は simulate が返せばその値、無ければ `ledger_index + 20` を `parse_simulate_result` が補完）。validated 待ち（`wait_validated`）: 1 秒ごとに `tx` を確認し、最終結果が `tes*` 以外なら Err、validated ledger が LLS を過ぎても未取込なら Err（expired）、5 分（`WAIT_LIMIT`）経っても決着しなければ Err（ノード停止・`validated_ledger` 欠落への上限）。`--no-wait` で submit 直後に戻る。`offers` は `account_offers` 読み取り。`faucet [ADDR]` は testnet/devnet の公開 faucet へ `destination` を POST（ADDR 省略時は `generate_wallet_keys_local` でローカル生成し seed は表示のみ）。応答の `classicAddress` が依頼先と違えば Err。
- Spec'd 書き込み（`-x tx <TYPE> Field=value…`・`-x nft-mint`・`-x nft-burn`、§2.1）も同じ network 照合 → simulate → sign → submit → validated 待ち。`--watch` は拒否（`Cmd::signs`）。`-x coverage` は XRPL TX 種別カバレッジ表（ROADMAP-ja.md「XRPL coverage」と同一、オフライン）を出力。
- スタック: `ratatui` + `crossterm`、非同期 `tokio`、XRPL は `src/xrpl/`。
- ユーザーが書いた設定ファイルとログ以外は何も永続化しない（Smart Account の詳細は §6.9「永続化」）。

## 2. ウォレット（行動契約）

Account タブの `WalletPanel` + `wallet_composer.rs`。**操作キー:** [`DESIGN.md`](../DESIGN.md)（Keys）。**Composer プレビューの配色:**（Components）。

行動契約:

- 認証情報の解決: `Config::new()` が `XRPL_SEED` / `XRPL_MNEMONIC` をマージし、`lib.rs::run` が `Config::take_secrets()` で秘密情報を取り出す（共有 `Arc<Config>` に秘密は残らない）。その後 `credential_from_secrets` → `PollContext.signing_credential`（`Option<SigningCredential>`）。
- すべてのウォレット TX: **network 照合 → simulate → sign → submit**（I-3）。mainnet / Xahau mainnet への書き込みは CLI **`--yes`** が必須（I-2）。
- 二重 submit ガード（TC-228）: キュー投入される書き込みを発行するのは `WalletPanel::on_key_event` だけである。params（`Debug`）が、まだ `*SubmitOk` / `*SubmitErr` を待っている書き込みと等しい場合は flash で拒否する。
- Endpoint network 照合（TC-203）: 署名前に `RpcClient::ensure_network` が `server_info.network_id` を `Network::network_id()`（mainnet 0 = 省略、testnet 1、devnet 2、xahau 21337、xahau-test 21338）と比較し、不一致なら拒否。TUI は全 XRPL submit が `fetch_account_summary_for_submit`、CLI は各 write arm で実行。`XRPL_RPC_SERVER` / `rpc_server` / `--server` は preset より優先されるため（§6.2）、別ネットワークの endpoint で `--yes` ガードが外れるのを防ぐ。
- 書き込みのネットワーク束縛: すべての write param は compose 時の `network` を持ち、アクティブなネットワークが変わると `poll.rs::stale_write_refusal` が拒否する（TC-350）。`RpcClient::submit_signed_tx` は 1 回限りの試行で、応答が失われた場合は `tx(hash)` で解決する（`rpc_once`、再送なし。不明な場合は明示的に報告、TC-351）。`terQUEUED` はキュー投入成功として扱う。保存された `network_id` が 1024 超（Xahau）のとき `NetworkID` を注入する — simulate には `ensure_network` が保存した id から、署名には `signing::with_network_id`（thread-local スコープ）経由で。**`NetworkID` 付き simulate の Xahau 実機での受理は未検証**（[`problems-ja.md`](problems-ja.md) P-007）。
- Flare 書き込み: `flare::verify_rpc_chain` が全 Flare サブコマンドで最初に 1 回 `eth_chainId` を読む。`check_write_chain` がそれを `expected_chain_id` と比較し、mainnet `--yes` の要否は**報告された**チェーン id から決める。TUI の C2 / Smart Account / C3 はさらに `flare::check_network_pair` を要求する（C3 は `PollContext.flare_network` を使用）。`send_capped` は gas × max fee が `MAX_TX_FEE_WEI`（10 FLR）を超えると拒否する。FDC は attestation 手数料を払う前に round パラメータを読み、tx ハッシュと `--round N` の再開ヒントを表示する。大文字小文字混在の EVM アドレスは EIP-55 を通ること。`tag_owner` は revert のみを「未予約」として扱う。Composer 内の Flare スナップショットは `NetworkChange` でクリアされ、300 秒より古いものは拒否される。
- 入力ルーティング: キーおよび bracketed-paste イベントは表示中のタブにのみ届く。paste は編集中のフィールドにのみ挿入され、ホットキーを発火しない。キーバインドは `config.rs::is_bindable` の許可リストに限る（ロード時に拒否）。設定ファイルは実際に読んだファイルでパーミッション検査され、`config_dir` / `data_dir` / `LAZYXRP_*` の `~` は展開される。認証情報は CLI > env > file の順に解決する（下位ソースのもう一方の種別は捨てられ、同一レベルに両種別があるとエラー）。
- 提供済みの Composer フェーズ: Payment、AccountSet、SetRegularKey、OfferCreate、OfferCancel、TrustSet、FXRP Mint (C2)、FXRP Execute (C3)、EscrowCreate（`9`）、Escrows finish/cancel（`0`）、AMMDeposit（`d`）、AMMWithdraw（`w`）、AccountDelete（`x`）、Smart Account（`a`）。escrow は TUI のみ（`-x` の escrow コマンドは無い）。
- Transaction composer（TC-255）: ピッカー行 `[m] Transaction` が spec'd 型（`xrpl::txspec`、§2.1）を列挙する。フォームは `TxSpec::fields` から生成される（フィールドごとに 1 行、spec の既定値をプリフィル、フォーカス行にヘルプ文とインライン検証エラーを表示、プレビュー行は `TxSpec::validate` が通れば緑の `▸ <summary>`、そうでなければエラー付きのオレンジ）。行移動／編集キーは共通の composer のものと同じ。`s` は poll タスクとまったく同じ検証を行い、`Action::GenericTxSubmit` を 1 件キューに積む。`TxSpec::confirm` を持つ型（NFTokenBurn `BURN`、NFTokenAcceptOffer `ACCEPT`）は直接キューに積まず、`s` が AccountDelete と同じ type-to-confirm の `DangerConfirm` を開く（Esc でフォームに戻る）。Assets タブ `x`（TC-256）: `NftTab` が `Action::NftBurnStart` に `Action::TxComposerOpen{NFTokenBurn, NFTokenID}` で応答し、app は Account タブへ移動してウォレットがプリフィル済みフォームを開く（署名認証情報がある場合のみ）。コンポーネントは `xrpl/` クライアントを呼ばず、静的な spec レジストリを読んで Action を発行するだけである。
- AMM（TC-222–226）: AMMDeposit は two-asset のみ（`tfTwoAsset`。`Amount` / `Amount2` = 上限値で、プールの指定も兼ねる。`signing::validate_amm_deposit_specs`: spec は `XRP:drops` / `CUR:issuer:value`、2 つの異なるアセット）。AMMWithdraw: LP 量が空 → `tfWithdrawAll`、それ以外は `tfLPToken` + `LPTokenIn`。その通貨／issuer（AMM アカウント）は simulate 前に `amm_info`（`RpcClient::amm_lp_token`）から得る。simulate → sign → submit は共通。composer `d` / `w` と `-x amm-deposit` / `-x amm-withdraw`。AMMCreate / single-asset / Vote / Bid は無し。
- AccountDelete（TC-232–233）: Destination = classic または X-address（タグは X-address 由来、network bit は Payment と同様に検査）。Composer は直接キューに積まず、有効な destination で type-to-confirm `DELETE` が開く（`DangerResume::AccountDelete` が destination を保持し、Esc で復元）。Fee はローカルで計算しない: simulate が owner-reserve fee（≤ `MAX_NODE_FEE_DROPS`）を埋め、その値が署名される。前提条件（`tecTOO_SOON` < 256 ledgers、`tecHAS_OBLIGATIONS` 所有オブジェクト、`tecDST_TAG_NEEDED`、`temDST_IS_SRC`）は simulate に任せる。`-x account-delete <DEST>` は CLI プロンプト／`--yes` を用いる同じ経路である。
- EscrowCreate: XRP のみ、crypto-condition 無し。`signing::validate_escrow_create_input`（composer + poll）: classic Destination、amount > 0、FinishAfter / 任意の CancelAfter は submit からの相対 duration（`90s` / `30m` / `2h` / `7d`）、CancelAfter は厳密に後。poll タスクが絶対 Ripple-epoch 時刻を（ローカル時計で）一度だけ確定し、simulate と署名済み tx が同じ値を持つ。FinishAfter が ledger 時刻で既に過ぎていれば simulate で失敗する。
- Escrows: 開くと `RefreshAccountEscrows` → `RpcClient::account_escrows`（`account_objects` type `escrow`、先頭 200 件、自分が所有するもの**と**宛先が自分のもの）を送る。`OfferSequence` = オブジェクトの `Sequence`（あれば）、無ければ作成 tx（`PreviousTxnID`。Escrow を変更するのは EscrowCreate のみ）の `Sequence`、それが 0 なら `TicketSequence`。`s` → EscrowFinish、`c` → EscrowCancel（`EscrowCloseSubmit`、simulate → sign → submit は共通）。ローカルで拒否するもの: sequence を解決できない場合、condition 付き escrow の Finish。タイミング規則（FinishAfter / CancelAfter 経過）は simulate に任せる（`tecNO_PERMISSION`）。
- OfferCancel: composer を開くと `RefreshAccountOffers` を送る（poll タスク → `account_offers`、`marker` を 1 ページ 200 件で最大 50 ページ辿る、署名アカウントのみ → `XrplAccountOffers`）。選択した `seq` → `OfferCancelSubmit`（同じ simulate → sign → submit 経路）。ポーリングはしない。
- Payment 検証: `signing::validate_payment_input`（composer のプレビュー / submit / quote と poll の `PaymentSubmit` で共有）— destination は classic チェックサムまたは X-address、正の amount（XRP drops / IOU 値）、IOU の通貨 + issuer チェックサム、Send max spec。poll はさらに X-address の network、自己送金、残高を検査する。
- クロスカレンシー Payment: 任意の `send_max` spec（OfferCreate と同じパーサ）→ `SendMax`、rippled 既定のパス（`Paths` 無し、partial payment 無し）。XRP→XRP の SendMax は拒否。SendMax 有りの場合: 自己送金を許可（swap）、残高チェックは SendMax の XRP drops を使う（IOU の SendMax は 0）。パスが枯れていれば simulate で失敗する（`tecPATH_DRY`/`tecPATH_PARTIAL`）。
- Send max quote（`p`）: `PaymentQuote` → poll タスクが署名アカウントから `ripple_path_find`（`source_currencies` = Send max フィールドの通貨プレフィックス。空 + IOU Amount → XRP）→ 最安の代替 + `[xrpl] slippage_bps`（既定 100 = 1%、切り上げ）→ `PaymentQuoteOk(spec)` がフィールドを埋める。読み取り専用で、送信は引き続き simulate → sign → submit を通る。
- FXRP C2: Core Vault への 32 バイト memo（`[flare.fassets] expected_core_vault` と等しい場合のみ署名、`flare::check_core_vault`）。C3: 貼り付けた FDC `XRPPayment` proof JSON（DA layer の `{response, proof}`、`response` = デコード済み attestation オブジェクト、または `{data, merkleProof}`）+ `[flare.fassets] execute=true` → `executeDirectMinting(IXRPPayment.Proof)`（selector `0x78d0299e`、TC-231）。revert したレシートはエラー。TUI は貼り付けた proof を受け取るだけで、proof の取得（verifier `prepareRequest` → `FdcHub.requestAttestation` → DA layer）は CLI `-x fxrp-execute`（`fdc::prove_xrp_payment`）が行う。32 バイト memo では任意の executor が先に finalize できる（その場合 C3 は `PaymentAlreadyConfirmed` で revert する。FXRP は既にミント済み）。
- FXRP C2 受取先のプリフィル（TC-367）: `open_fxrp_direct_mint_composer` は `Flare recipient` に署名者自身の personal account を入れる（`own_personal_account`: Smart Account の読み取りが署名 XRPL アドレス向けで、300 秒より新しいこと。そうでなければフィールドは空のまま）。この値は Flare RPC の `getPersonalAccount` の回答であり、ミントされる FXRP の宛先としては未ピン留めなので、composer は全体を表示し、エクスプローラで照合するよう注記する（security.md S-019）。編集は可能なままである。Payment の後、TUI はハッシュを報告するだけである: recipient-memo の素のミントには lazyxrp 内に「finalized」ビューが無い — `isTransactionIdUsed` はこれをカバーしない（Coston2 で実機確認: 公開 executor が finalize した素の C2 ミント `FB8556…` は `false` を返す）— ため、ユーザーは受取先の FXRP（personal account なら Overview の `SA FXRP`）を見るか、C3 / `-x fxrp-execute` で finalize する。AssetManager 自身の記録（その `transactionId` を持つ `DirectMintingExecuted` イベント）が情報源になり得るが、公開 RPC は `eth_getLogs` を 30 ブロック未満に制限するため、配線していない。
- FXRP v1.3 ガード（TC-330–333）: C2 は `max(getDirectMintingMinimumFeeUBA, fee_bips 取り分) + executor fee` を超えない金額を拒否する（`flare::check_direct_mint_amount`。最小手数料未満の支払いは全額が手数料受取人にミントされる）。また `mintingPaused()` が true または `emergencyPauseLevel() > 0` の間はキュー投入を拒否する（どちらも `fetch_fxrp_direct_mint_info` で読む。読み取り失敗は `None` = 不明であり、ブロックではない）。C3 は AssetManager のイベントからレシートを分類する（`flare::decode_direct_minting_outcome`）: `DirectMintingDelayed` / `LargeDirectMintingDelayed` は遅延として報告 — `executionAllowedAt` に同じ proof で再実行する。`DirectMintingPaymentTooSmallForFee` はエラー。
- FXRP ミント上限（TC-334–336）: `fetch_fxrp_direct_mint_info` は同じ並列バッチで `getDirectMinting{Hourly,Daily}LimitUBA` + `...LimiterState`（書き込みの間は古くなる → `flare::window_max_now` が 1 h / 1 d のタンブリングウィンドウを最新ブロック時刻まで進める）、large-mint の閾値／遅延、`assetMintingGranularityUBA`、FXRP の `totalSupply` と `mintingCapAMG` も読む。`AssetManagerSettings.Data` は 58 フィールドで alloy のタプル ABI サポートを超えるため、`getSettings()` は生バイトとして読み、構造体ヘッドの word 22 を取る（`settings_minting_cap_amg`、実機で相互確認: mainnet で 170M XRP）。`flare::classify_mint` → `Immediate` / `DelayedWindow` / `DelayedLarge` / `CapExceeded`。composer が拒否するのは `CapExceeded` のみ（cap チェックは revert するので XRP が Core Vault で待たされる）。遅延は FXRP パネル（`Max now`、`Cap used`）に表示する。いずれかの view が失敗すると `limits` は `None`（参考情報）。
- `lazyxrp -x fxrp [--tag N]`（TC-337）は XRPL クライアントより前に `lib.rs` で処理される（`Cmd::Fxrp`。Flare RPC = `FLARE_RPC_URL` または `[flare] network` の既定値 — `resolve_flare_rpc_url` は常に値を返すので「Flare RPC が無い」エラーは防御的なもの。エンドポイント検査は TUI と同じ）。`flare::fetch_fxrp_report` は JSON を返す: 上記の Direct Mint 情報、`getMintingTagManager()` → `reservationFee`、`--tag` 指定時は `ownerOf` / `mintingRecipient` / `allowedExecutor`（`ownerOf` のコントラクト revert → `reserved: false`。トランスポートエラーは伝播）、そして全体の `redemptionQueue(next, maxRedeemedTickets)` を `next == 0` までページング（最大 100 ページ。`complete` は終端まで到達したかを示す）。各ページは別個のピン留めなし呼び出しである。`getSettings()` のフィールド word（`lotSizeAMG` 19、`mintingCapAMG` 22、`maxRedeemedTickets` 30）は upstream の `AssetManagerSettings.sol` の順序に従う。
- Smart Account vaults（TC-339）: vault 一覧はオンチェーンの `MasterAccountController.getVaults()`（`uint256[] ids, address[] addresses, uint8[] types` — 1 Firelight、2 Upshift）で、vault アドレスのピン留めやハードコードは無い。`fetch_smart_account_info` は他の controller view と同じバッチで読み（失敗は `None` → 行なし）、その後、最大 8 個の vault について personal account の share `balanceOf` と ERC-4626 `convertToAssets(shares)` を読む（逐次、参考情報: 失敗した vault はスキップ、share がゼロの vault は省略）。`SmartAccountInfo.vaults` → FXRP パネルの行 `Vault <id> <kind> <assets> FXRP`（`convertToAssets` が無ければ生の share 数）。`-x fxrp` はさらに `coreVaultAvailableAmount()` と `getAllAgents` を報告し、redemption queue と合わせて FXRP 供給量の内訳になる（direct mint は redemption ticket を作らず、Core Vault への転送は agent ticket を消費する）。
- Flare 書き込み（`-x fxrp-execute` / `fxrp-redeem` / `fxrp-tag-*`、TC-340–342）は XRPL クライアントより前に `lib.rs::run_flare_command` で実行される。ゲート順: `[flare.fassets] execute = true` → Flare EVM 鍵（`FLARE_EVM_KEY`、起動時に取得）→ https RPC（署名付きの `check_flare_endpoint`）→ `verify_rpc_chain`（`eth_chainId` と選択ネットワークの照合。最初に 1 回実行）→ Flare mainnet では `--yes`（報告されたチェーン id から判定）。各書き込みはまずオンチェーンの前提条件を読み、`estimate_gas`（EVM の simulate）を呼び、プランを stderr に出力する。ハッシュは stdout に出る。`fdc::prove_xrp_payment`（verifier → `getRequestFee` 付きの `FdcHub.requestAttestation` → `Relay.isFinalized(200, round)` → DA layer）が `flare::execute_direct_minting_with` に proof を渡す（`--data` → `executeDirectMintingWithData`）。proof は署名者に束縛される（`proofOwner`）。`-x sa-vault` は `xrpl::run_smart_account_instruction` 経由で `submit_smart_account_instruction`（operator pin、fee 上限、simulate）を再利用する。`SmartAccountInstructionParams.vault` が vault 命令のエンコードを選ぶ。
- Smart Account 命令（TC-239–247、§6.9）: composer `a` → `Action::SmartAccountInstructionSubmit` → `PollCommand::SmartAccountInstruction` → `submit_smart_account_instruction`: 32 バイトの payment reference をエンコード（`signing::encode_fxrp_transfer_instruction` / `encode_fxrp_redeem_instruction`。transfer の値 = FXRP の生の最小単位で、composer が読み取った `decimals()` から変換）→ fee 上限（`check_smart_account_fee`）→ classic operator（X-address のタグ無し）→ mainnet の `--yes` ゲート → operator ≠ 署名者 → **operator pin**（`flare::check_operator` と `[flare.smart_accounts] expected_operator`）→ `account_info` → simulate（`tesSUCCESS`）→ sign → submit。Payment は `getXrplProviderWallets()[0]` 宛て、`Amount` = `getInstructionFee(id)` の XRP drops（厳密）、`DestinationTag` 無し、Memo 1 つ（`MemoData` = 32 バイト命令の hex）。その後 operator が Flare 上で FDC 証明付きの `executeInstruction` を実行する（lazyxrp の外）。TUI のヒントは personal account の残高を確認するよう促す。
- `RpcClient::simulate_tx` は送信前に JSON の `null` メンバーを落とす（xrpl-rust は未設定の `Memo` フィールドを `null` としてシリアライズし、rippled がそれを拒否するため。TC-230）。

### 2.1 Spec'd トランザクション型（`src/xrpl/txspec/`）とカバレッジ

XRPL トランザクション型ごとに宣言的な `TxSpec` を 1 つ置くことで、手書き型における型ごとの Action 3 点組 / `PollCommand` / `submit_*` / composer フェーズ / CLI arm を置き換える。汎用部分はすべて共有される。

| 部品 | 場所 | 役割 |
|-------|-------|------|
| `TxSpec`（type、family、summary、`fields: &[FieldSpec]`、`confirm`、`check`、`build_json`、`sign`） | `txspec/mod.rs` | レジストリのエントリ。`FieldKind` = `Uint(max)` / `Text(max bytes)` / `Address` / `Hex(len)` / `HexList` / `Amount`（`XRP:drops` または `CUR:issuer:value`、`signing::parse_offer_amount`）/ `Time`（`7d` または Ripple-epoch 秒）/ `Flags(table)`。`validate` = フィールド別の kind 検査 + 必須 + フィールド間 `check` + 未知フィールドの拒否 |
| Family ファイル（`nft.rs`） | `txspec/<family>.rs` | `SPECS` + TX ごとに: `*_tx(account, fields, sequence, fee_lls)` が型付き `xrpl::models` 構造体を一度だけ構築。`*_json` = simulate 用の未署名 `tx_json`（crate の `Model::validate` も実行）、`*_sign` = simulate の Sequence / Fee / LastLedgerSequence を使う `sign_blob!` |
| レジストリ | `txspec::FAMILIES` | spec'd 型を列挙する唯一の場所（`registry()`、`lookup()`） |
| Submit | `submit::submit_generic_transaction` | `GenericTxSubmitParams{tx_type, fields, skip_mainnet_prompt}` → `TxSpec::prepare`（検証、trim、`Time` → 絶対時刻に**一度だけ**変換）→ `run_submit`（mainnet `--yes` ゲート → `ensure_network` → `account_info` → リクエストに束縛された simulate `tesSUCCESS` → Fee 上限 → sign → submit。poll の `submit_lock` 下）。Action 3 点組 `GenericTxSubmit/Ok/Err` が 1 組、`PollCommand::GenericTx` が 1 つ |
| TUI | `wallet.rs` / `wallet_composer.rs` | `ComposerPhase::TxPick` / `Tx`（§2） |
| CLI | `cli_exec.rs::cmd_tx_request` | `Cmd::Tx` / `NftMint` / `NftBurn` → あらゆる RPC の前に同じ `prepare` → `simulate_confirm_submit` |
| カバレッジ | `src/coverage.rs` | `rows()` = `Existing`（手書き、`EXISTING` に列挙）+ `Spec`（レジストリから導出）+ `Planned`（xrpl-rust 1.3.0 に型付きモデルあり、未配線）+ `Unsupported`（型付きモデル無し）。`read` = TX 詳細パーサ登録済み。`lazyxrp -x coverage` は ROADMAP-ja.md「XRPL coverage」に貼る表を出力する。`roadmap_table_matches_registry`（TC-248）はずれを検知して失敗し、README の `**N / M**` の数も検査する |

NFT family（TC-249–251）: NFTokenMint（`NFTokenTaxon` u32、`URI` はプレーンテキスト ≤ 256 バイト → 大文字 hex、`Flags` burnable / onlyXRP / transferable、`TransferFee` 0..=50000 かつ transferable 時のみ、任意の `Issuer`）、NFTokenBurn（`NFTokenID` 64 hex、任意の `Owner`。confirm `BURN`）、NFTokenCreateOffer（`Amount`、`Flags=sell`、`Owner` は buy offer で必須・sell offer では拒否、`Destination`、`Expiration`）、NFTokenAcceptOffer（sell と buy のいずれか一方または両方の offer id、`NFTokenBrokerFee` は両方指定時のみ。confirm `ACCEPT`）、NFTokenCancelOffer（異なる offer id を 1..=500 個）。非対応: `tfMutable` ミント（xrpl-rust の `NFTokenMintFlag` に `0x10` が無い）、価格ゼロの offer（共有 amount パーサが 0 を拒否）。`NFTokenMinter` は AccountSet のフィールド（手書き AccountSet 経路）であり NFTokenMint のフィールドではない。別の issuer のためにミントする場合は `Issuer` を使う。

Account-objects family（`checks_misc.rs`、TC-260–263）: CheckCreate / CheckCash / CheckCancel / TicketCreate / DepositPreauth / SignerListSet / Clawback。confirm フレーズは `CASH` / `SIGNERS` / `CLAWBACK`。省略（crate または共有パーサが表現できない）: DepositPreauth の credential リスト、SignerEntry の `WalletLocator`、MPT Clawback（`Holder`）。

XChain family（`xchain.rs`、TC-320–328）: XChainCreateBridge / ModifyBridge / CreateClaimID / Commit / Claim / AccountCreateCommit。bridge は 1 つのフラットなフィールド `XChainBridge=lockDoor,lockIssue,issueDoor,issueIssue`（issue = `XRP` または `CUR:issuer`。door はチェックサム検査済みで相異なること。XRP↔IOU の組み合わせは不可）。rewards / minimum は素の drops。`XChainClaimID` は hex（`13f`）。confirm フレーズは `BRIDGE` / `COMMIT` / `CLAIM`。非対応: witness attestation 系の型（署名をフォームで作れない）と ConfidentialMPT*（ゼロ知識証明、型付きモデル無し）— どちらも `coverage.rs` では `Unsupported`。値がゼロの `Amount` は共有 amount パーサが拒否する。

MPT / Identity family（`mpt.rs` TC-301/303/305/310、`identity.rs` TC-300/302/304/308/309。`identity.rs` は共有ヘルパも置く）: MPTokenIssuanceCreate / Destroy / Set（lock、unlock のみ）/ MPTokenAuthorize、および CredentialCreate / Accept / Delete、DIDSet / DIDDelete、OracleSet / OracleDelete、PermissionedDomainSet / Delete。取り消せない型には confirm `DESTROY` / `DELETE`。blob フィールドはプレーンテキストを hex エンコードする。`PriceDataSeries=XRP/USD=740@2,…` と `AcceptedCredentials=rIssuer:Type,…` はフラットな文字列。CredentialCreate の `URI` は 128 バイト上限（xrpl-rust のモデル）。省略: MPT の変更フラグ / `ImmutableFlags` / confidential 鍵、DIDSet のフィールドクリア。Vault* は `Planned` のまま（MPT 対応の Asset / Amount）。

Channel / AMM family（`channel_amm.rs`、TC-280–286）: PaymentChannelCreate / Fund / Claim（drops のみの `Amount`、`PublicKey` 66 hex、`CancelAfter` / `Expiration` は `Time`。Claim は貼り付けた `Signature` + `PublicKey` を受け取り、ローカルでは生成しない。`Flags` renew / close）と AMMCreate / Vote / Bid / Delete（`Asset` / `Asset2` = `XRP` または `CUR:issuer`（`parse_amm_asset` 経由）、`TradingFee` 0..=1000、`BidMin` / `BidMax` は LP トークン `CUR:issuer:value`、`AuthAccounts` ≤ 4）。confirm フレーズは `LOCK` / `FUND` / `CLAIM` / `CREATE` / `BID` / `DELETE`（AMMVote には無し）。非対応: Claim の `CredentialIDs`。AMMClawback は `Unsupported` のまま。

**トランザクション型の追加方法**（今後の family: Check*、TicketCreate、DepositPreauth、SignerListSet、PaymentChannel*、AMM*）:

1. `src/xrpl/txspec/<family>.rs` を作る（`nft.rs` をコピー）: 型ごとに `pub const X: TxSpec` と `FieldSpec`（name = ledger のフィールド名、`FieldKind`、`required`、`help`、`default`）、任意のフィールド間 `check`、`*_tx`（`common_fields!` による型付き構造体）、`*_json`（`tx.validate()` + `serde_json::to_value`）、`*_sign`（`sign_blob!`）、そして `pub static SPECS: &[TxSpec] = &[…]`。
2. `txspec/mod.rs` に `pub(crate) mod <family>;` を追加し、`FAMILIES` に **1 エントリ** `<family>::SPECS` を追加する。それ以外は不要: ピッカー、フォーム、`-x tx`、submit、ガード、カバレッジの `Spec` 行はレジストリから導かれる。confirm フレーズを持つ型は `confirm: Some(Confirm{phrase, hint})` を設定する。
3. `src/coverage.rs` の `PLANNED`（または `UNSUPPORTED`）から当該型を外す（2 重に列挙すると `registry_is_consistent` が失敗する）。TX 詳細パーサが登録されていることを確認する（`Spec` 行には `read` が必要）。その後 `lazyxrp -x coverage` を ROADMAP-ja.md の表に貼り、README の `**N / M**` を直す。
4. テスト: 検証の受理／拒否表（境界値）、未署名 `tx_json` の形、Sequence / Fee / LastLedgerSequence を別々の値にした署名 blob のデコード（`nft.rs` のテストをコピー）。`submit.rs` の TC-088 mainnet ゲート表に型を追加。docs/test-ja.md に TC 行を追加。

受理される tx 型／フィールドは `-x tx` が取るものでもある: `lazyxrp -x tx NFTokenCreateOffer NFTokenID=… Amount=XRP:1000000 Flags=sell Expiration=7d`。

## 3. ランタイム・データフロー

実装: `src/xrpl/poll.rs`（ループ、読み取り）、`src/xrpl/submit.rs`（書き込み）、`src/app.rs`。

| 方向 | 経路 |
|-----------|------|
| 入力 | キーボード → `tui::Event` → `Action`。XRPL RPC/WS → `Action` |
| 手動リフレッシュ | `Action::Refresh*` → `PollCommand` → poll タスク |
| Submit | `Action::*Submit` → `PollCommand` → poll 内で simulate/sign/submit |
| 出力 | `Component::update` → `draw`。`App::run` は `needs_draw` のとき 1 ループ反復ごとに `draw_frame` を 1 回呼ぶ（固定フレームタイマーは無い） |

WebSocket の ledger close は poll トリガーを集約する（`MIN_POLL_INTERVAL`）。定期 poll は設定の `poll_interval_ms` を使う（対話利用の既定は **5000** ms。長時間／常時稼働セッションでは公開 RPC への負荷を減らすため **15000–30000** ms を推奨）。

**公開ネットワークへの配慮:** XRPL Foundation の dUNL（`unl.xrplf.org`）はプロセス内で **10 分**キャッシュされる（`DUNL_CACHE_TTL`）。更新に失敗した場合、古い要約を **60 秒**の再試行クールダウンの間返し続ける（`DUNL_RETRY_BACKOFF`、stale-on-error — 一度も成功する前の初回取得失敗にはキャッシュが無く、接続できるまでエラーになる）。Flare の `ContractRegistry` 参照（`FtsoV2`、`AssetManagerFXRP`）は RPC URL ごとにプロセス生存期間中キャッシュされる。poll の失敗は **エンドポイントごとの 2 つの独立したトラック** で指数バックオフする（2s→60s 上限）: XRPL RPC の取得成功は XRPL トラックに作用し、Flare の取得失敗は `warn!` するだけで XRPL トラックには触れずに同じ 2s→60s スケジュールの Flare 専用クールダウンをエスカレートする。dUNL の取得（別ホスト）も同様に、双方向で XRPL トラックから除外される。Account タブのパネル（`account_tx`、ウォレット概要）は Account タブがアクティブな間だけ poll し、Market/Assets タブの poll 判定と揃えている。HTTP 429 は遅延付きで再試行する。NFT 画像取得（`nft_image.rs`）はホップごとの DNS SSRF 検査、手動リダイレクト、ストリーミングでのレスポンスサイズ上限を適用する（[security-ja.md](security-ja.md) R-012）。

**信頼できない取得と署名のガード:** `nft_image::fetch_via` + `client_for_url` は NFT 画像**と** `xrp-ledger.toml`（validator-manifest の `domain` はプレーンな ASCII ホスト名であること）の唯一の取得経路である: プライベート／予約アドレスは拒否、接続は検査済み IP に固定、プロキシ無効、すべてのリダイレクトホップ（≤5）を再検査、本文サイズ上限（画像 4 MiB / toml 1 MiB）。JSON-RPC の本文はストリーミング中に 16 MiB で打ち切る。ノードが選んだ `Fee` が `signing::MAX_NODE_FEE_DROPS`（2 XRP）を超える場合は署名しない。CLI の出力／エラーに出るリモート文字列は `util::sanitize_remote` を通す。Flare RPC URL は XRPL エンドポイントと同じ https / http 経由では署名しないというルールに従う（`lib.rs::check_flare_endpoint`）。

**NFT 画像取得:** リクエストは `lazyxrp/<version>` の User-Agent を付ける（Wikimedia などの CDN は UA 無しのリクエストに 403 を返す）。IPFS 参照（`ipfs://<cid>`、二重の `ipfs://ipfs/<cid>`、`/ipfs/<cid>`、素の `Qm…`/`baf…` CID。scheme は大文字小文字を区別しない）は `ipfs_path` で正規化され、`IPFS_GATEWAYS`（`ipfs.io`、`gateway.pinata.cloud`、`dweb.link`）を順に試す。単一の公開ゲートウェイは 429/5xx を返したり、持っていないコンテンツを拒否したりするのが常だからである。`data:<mime>;base64,…` URI はローカルでデコードする（上限 4 MiB）。それ以外の `data:` 形式は拒否する。NFT タブの URI 列は固定列の残り幅を使い（`NFT_TABLE_OVERHEAD`）、プレビューは画像の上に URI 全体を折り返して表示する。どちらも `display_uri` のゲートウェイ形式（`https://ipfs.io/ipfs/…`、それ以外の URI はそのまま）を `sanitize_text` 経由で表示し、取得（`NftImageRequest`）はオンチェーンの生の URI を保持する。Assets タブは、別スレッドでのリサイズ完了時に `NftTab` が発行する `Action::Render` で再描画する（素の `Tick` ではフレームは dirty にならない）。

## 4. 設定値と起動パラメータ

### CLI

| フラグ | 効果 |
|------|--------|
| `--yes` | mainnet 書き込みの確認をスキップ（CLI + TUI submit） |
| `--no-wait` | `-x` の書き込みは submit 後に戻る（既定: validated を待ち、`tes` 以外は非ゼロ終了） |
| `--network` | `mainnet` \| `testnet` \| `devnet` \| `xahau` \| `xahau-test` |
| `--server`, `--ws-server` | カスタム RPC/WS（最優先） |
| `--allow-insecure-rpc` | `http://` / `ws://` を許可（XRPL と Flare RPC）。非セキュアなエンドポイント経由の署名は引き続き拒否 |
| `--tick-rate` | TUI ループの tick レート |
| `--seed` / `--mnemonic` | 認証情報の上書き（**非推奨** — env/config を使う） |
| `--account` | 監視アドレスの上書き |
| `--watch` | 読み取り専用（TC-234–235）: `lib::enter_watch_mode` が TUI / CLI の開始前に `Secrets`（seed / mnemonic / `flare_evm_key`。`take_secrets()` で `Config` から取り出したもの）を空にし、鍵のアドレスだけを既定の `--account` として残す。鍵が無ければ Wallet の代わりに Account パネル（composer 無し、`g` 無し）。poll タスクに認証情報は渡らない。`-x` の署名系サブコマンド（`Cmd::signs`）は `resolve_lazyxrp_command` で拒否される。非セキュアエンドポイントの署名チェックは認証情報を見ない |
| `-x` / `--exec` | スクリプト CLI モード |

### 設定ファイル（`Config::xrpl`）

- パス: `$XDG_CONFIG_HOME/lazyxrp/config.toml` → `~/.config/lazyxrp/config.toml`
- フィールド: `network`、`account`、`issuer`、`currency`、`currency_code`、`offer_limit`、`poll_interval_ms`（既定 5000。24/7 ダッシュボードでは上げる）、`slippage_bps`（Payment quote、既定 100 = 1%）、`oracles`、`oracle_pairs`、`[xrpl.signing]`、任意の `rpc_server` / `ws_server`
- 既定 issuer のフォールバック（mainnet Bitstamp USD）: `rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B`
- `currency_code`: `book_offers` 用の 160 ビット hex（表示の既定は `"USD"`）

### Flare（`[flare]`）

| フィールド | 既定 | 備考 |
|-------|---------|-------|
| `network` | `flare` | `flare` \| `songbird` \| `coston2` — RPC プリセット。`FLARE_RPC_URL` env が優先 |
| `display` | `full` | `full` \| `compact` \| `off` — パネルレイアウト + `off` のとき FTSO/FXRP の poll をスキップ（C3 の URL は変わらない） |
| `fassets.execute` | `false` | C3 `executeDirectMinting` には true が必要 |
| `wallet.address` | _（未設定）_ | 任意の `0x…` EVM アドレス — Overview の読み取りパネル（ネイティブ FLR + FXRP）。不正な hex は設定ロードに失敗する |
| `smart_accounts.expected_operator` | _（未設定）_ | Smart Account 命令 Payment 用の XRPL `r…` operator pin。Flare RPC の `getXrplProviderWallets()[0]` がこれと等しくなければならない（未設定／不一致 → 拒否）。帯域外で検証すること |

## 5. 読み取り API とパネル（FR-08–FR-11）

型、RPC メソッド、`Action` バリアント: `src/xrpl/types.rs`。UX レイアウト: [`DESIGN.md`](../DESIGN.md) §4。

### 5.1 パネル一覧

| タブ | コンポーネント | 主な RPC / ソース |
|-----|------------|----------------------|
| Overview | `ServerPanel`、`CombinedOraclePanel`、`FlareWalletPanel` | `server_info`、`fee`、`get_aggregate_price`、Flare FTSO、FXRP AssetManager、`eth_getBalance` + FXRP `balanceOf`（Overview タブのみ） |
| Overview (Flare) | `FxrpDirectMintPanel`（`CombinedOraclePanel` 内） | Smart Account 読み取り（§6.9）: 署名アカウント（無ければ監視アカウント）の `MasterAccountController` view。FXRP と同じ Flare バッチ・ゲート |
| Account | `WalletPanel`、`AccountPanel`、`TxHistoryPanel`（上段はウォレット**または**アカウント。レイアウト: [`DESIGN.md`](../DESIGN.md) §4） | `account_info`、`account_tx`、ウォレット submit |
| Market | Book、PathFind、AMM、TrustLines、FTSO、Oracle | `book_offers`、`ripple_path_find`、`amm_info`、`account_lines`、oracles |
| Assets | `NftTab` + 3× `LedgerObjectsPanel`（objects / pay channels / escrows。レイアウト: [`DESIGN.md`](../DESIGN.md) §4） | `account_nfts`、`account_objects` |

poll はタブが非アクティブのとき重い market 取得をスキップする（`poll.rs` の `should_poll_market_book`、`should_poll_asset_panels`）。

### 5.2 共通の挙動

- **TX 詳細:** `ArcValue` の tx/meta を持つテーブル行は、統一オーバーレイ（`tx_detail::render_tx_detail`）を開ける。パーサ契約: §5.2.1。オーバーレイの**キー**: [`DESIGN.md`](../DESIGN.md) §6。
- **Tx 履歴のページネーション:** `marker` → `PollCommand::TxHistoryMore` → `XrplTxHistoryAppend`。
- **Tx 履歴のフィルタ:** パネル内のフィルタ状態。RPC クエリは変えない（クライアント側）。
- **Path find:** 設定された book pair の金額を使い、`ripple_path_find` で自己送金のプレビューを行う。

#### 5.2.1 TX 詳細パーサのパイプライン

| シンボル | ファイル | 役割 |
|--------|------|------|
| `TxDetailState` | `tx_detail/mod.rs` | オーバーレイの可視性、スクロールオフセット、行キャッシュ |
| `render_tx_detail` | `tx_detail/mod.rs` | 中央寄せポップアップの描画 |
| `typed_detail_lines` | `tx_detail/parsers.rs` | `TransactionType` によるディスパッチ |
| `detail_lines_for` | `tx_detail/mod.rs` | 未知フィールドのフォールバック |
| `TX_DETAIL_PARSERS` | `tx_detail/parsers.rs` | 静的レジストリ（29 型） |

`raw_json: ArcValue`（tx + meta）を持つテーブル行は、フォーカス中に `Action::TxDetailToggle` でオーバーレイを開ける。

**パイプライン**

1. **ヘッダ** — `Result` は常に。`hash` / `ledger` / `date` は JSON にあれば。
2. **型付き分岐** — `typed_detail_lines` が `TX_DETAIL_PARSERS` から `TransactionType` を引く。パーサは整形済み `Line` のリストまたは `None` を返す。
3. **フォールバック** — `detail_lines_for` が既知のフィールド名を走査し、未列挙のキーは `format_value` で出す。`Amount2` は AMM の型付きセクションで既知（「Other fields」では重複させない）。
4. **キャッシュ** — TX ごとの最初のオープンで `cached_lines` を構築し、再描画はキャッシュを再利用、スクロールは段落オフセットを動かすだけ。新しい TX でキャッシュは無効化される。

**オーバーレイのガード**（`TxDetailState::handle_action`）

- **閉**のとき: パネルは選択行から事前解決した `(tx, meta)` で `TxDetailToggle` を処理する。
- **開**のとき: `Quit`、データ更新、`Tick`、ナビゲーション系 Action を消費し、ポップアップの下で背景のテーブルの選択が変わらないようにする。
- `SelectNext` / `FocusNext`（および prev）は飽和算術で詳細テキストをスクロールする。

**登録済みの型（29）**

`Payment`, `AccountSet`, `TrustSet`, `OfferCreate`, `OfferCancel`, `NFTokenMint`, `NFTokenBurn`, `NFTokenCreateOffer`, `NFTokenAcceptOffer`, `NFTokenCancelOffer`, `CheckCreate`, `CheckCash`, `CheckCancel`, `SignerListSet`, `SetRegularKey`, `DepositPreauth`, `EscrowCreate`, `EscrowFinish`, `EscrowCancel`, `PaymentChannelCreate`, `PaymentChannelFund`, `PaymentChannelClaim`, `AMMCreate`, `AMMDeposit`, `AMMWithdraw`, `AMMVote`, `AMMBid`, `AMMDelete`, `TicketCreate`.

6 つの AMM バリアントは `amm_detail_lines` を共有する（順序付きフィールド、明示的な `null`、create/vote での `TradingFee` 整形）。

**パーサの追加**

1. `parsers.rs` に `fn foo_detail_lines(tx: &Value) -> Option<Vec<Line>>` を実装する（`typed_detail` ヘルパを推奨）。
2. `TX_DETAIL_PARSERS` に 1 行追加する。
3. 挙動がユーザーに見える、または回帰しやすい場合は、`parsers.rs` または `registry_tests` に `/// TC-xxx` テストを追加する。
4. レジストリが増えたら、この節の型リストを更新する。

オーバーレイの操作が変わらない限り `DESIGN.md` の変更は不要。

**テスト:** **TC-094** はレジストリに重複が無く、必須の型がディスパッチされることを確かめる。パネル統合では、テーブルが `Enter` でオーバーレイを開く（`docs/test-ja.md` の wallet/history 行を参照）。

### 5.3 Path-Find（Market）

- Polling: 宛先金額として `book_pair.path_find_destination_amount_preview()` を使う。
- 表示: ルートは送信額が安い順にソートされる。生のルート JSON は TX 詳細パターンで参照できる。

### 5.4 Ledger objects

`PollCommand::LedgerObjects` → `account_objects` → `Action::XrplLedgerObjects`。Assets タブは `LedgerEntryType` でフィルタする（objects / pay channels / escrows）。

---

## 6. ネットワークと署名（FR-12–FR-13）

### 6.1 ネットワークプリセット

| ネットワーク | RPC | WS |
|---------|-----|-----|
| mainnet | `https://xrplcluster.com` | `wss://xrplcluster.com` |
| testnet | `https://s.altnet.rippletest.net:51234` | `wss://s.altnet.rippletest.net:51233` |
| devnet | `https://s.devnet.rippletest.net:51234` | `wss://s.devnet.rippletest.net:51233` |
| xahau | `https://xahau.network` | `wss://xahau.network` |
| xahau-test | `https://xahau-test.net` | `wss://xahau-test.net` |

**TUI のセッション内切り替え:** `<Ctrl-n>` でネットワークを巡回する（[`DESIGN.md`](../DESIGN.md) §6）。Poll/WS は `network_watch` 経由で再バインドされ、パネルは更新される。設定には**永続化されない**。カスタムの `--server` / `XRPL_RPC_SERVER` → `custom_rpc`: 切り替えてもエンドポイントは変わらない。**Mainnet ガード**は `mainnet` と `xahau` に適用される。

### 6.2 解決の優先順位

```
CLI --network > XRPL_NETWORK env > config.toml [xrpl] network > default (mainnet)
CLI --server / --ws-server > XRPL_RPC_SERVER / XRPL_WS_SERVER env > config rpc_server / ws_server > network preset
```

### 6.3 設定例

```toml
[xrpl]
network = "mainnet"   # mainnet | testnet | devnet | xahau | xahau-test

[flare]
network = "flare"     # flare | songbird | coston2
display = "full"      # full | compact | off

[flare.smart_accounts]
expected_operator = "r…"   # verified out of band; required for Smart Account writes
```

### 6.4 接続ブートストラップ

`src/lib.rs`: network = `--network` または `config.xrpl.network`（env は `Config::new` がマージ）。`resolve_rpc_url`、`resolve_ws_url`。`src/main.rs` は薄い `lazyxrp::run()`。

### 6.5 StatusBar のネットワーク表示

右寄せのバッジ `format!(" {} ", display_name())`。クローム: [`DESIGN.md`](../DESIGN.md) §5。`Action::NetworkChange` で更新される（TC-115）。

### 6.6 シークレット（FR-13）

読み込み順（seed **または** mnemonic のどちらか一方）:

1. CLI の `--seed` または `--mnemonic`
2. `XRPL_SEED` / `XRPL_MNEMONIC`
3. config の `[xrpl.signing]`

`SecretString` + `SigningCredential`。本番書き込みのプロンプトは、`--yes` が無い限り `prompt_production_confirmation` を通る。

### 6.7 Flare 表示レベル

ピクセル数／フォーカス数: [`DESIGN.md`](../DESIGN.md) §4。

| レベル | Poll |
|-------|------|
| `full` | Overview タブで FTSO + FXRP + Smart Account を取得 |
| `compact` | full と同じ |
| `off` | FTSO/FXRP/Smart Account の**読み取り**取得をスキップ。C3 execute の URL は維持。Composer `6` / `a` は自身の読み取りが必要なため、利用不可のまま |

### 6.8 Flare ウォレット読み取りパネル（FR-16）

- 設定: `[flare.wallet] address`（任意の EVM `0x…`）。
- Overview の右列: `display != off` のとき `CombinedOraclePanel`（上）+ `FlareWalletPanel`（下）。分割: [`DESIGN.md`](../DESIGN.md) §4。
- Poll: `fetch_flare_wallet_balance` は Overview タブのみ。`Action::FlareWalletBalance` を発行する。アドレス未設定 → パネル内にセットアップ案内（RPC 無し）。
- 読み取り専用: ネイティブ FLR、FXRP ERC20 残高、`[flare.fassets] execute` の設定、executor の env 鍵インジケータを表示する。Flare への書き込みは無い。

### 6.9 Flare Smart Accounts（proof ベースの命令フロー）

- 読み取り（`flare::fetch_smart_account_info(rpc, xrpl_address)` → `SmartAccountInfo` → `Action::SmartAccountInfo`）: `FlareContractRegistry`（`getContractAddressByName`。`AssetManagerFXRP` と同様に RPC URL ごとにキャッシュ。ゼロアドレスはエラー）経由で `MasterAccountController` を解決し、続いて `getPersonalAccount(xrpl)`、`getNonce(personal)`、`getXrplProviderWallets()`、XRP drops での `getInstructionFee(1)`（FXRP transfer）/ `getInstructionFee(2)`（FXRP redeem）、トークン最小単位での FXRP `balanceOf(personal)`、FXRP `decimals()`（`SmartAccountInfo.fxrp_decimals`。18 超はエラー。どちらも AssetManager の `fAsset()` から得たトークン経由）、`eth_getCode(personal)`（deployed = コードが空でない。アカウントは最初の命令でデプロイされる）を読む。リモート値は checked 変換を使う（panic しない）。読み取り専用。
- Poll: FTSO / FXRP と同じ Flare バッチ内（`poll_batch`）: Overview タブのみ、`flare display != off`、Flare のバックオフ窓、owner = 署名アカウント（無ければ監視アカウント。空ならスキップ）。成功すると他の flare Action と同様にステータスバーの flare ヘルスを点灯させる。
- UI: FXRP パネル内の追加行（personal account、FXRP 残高、nonce + deployed、operator、2 つの fee）。リモート文字列はサニタイズされる。Composer `a` は読み取りが必要（無ければ flash）で、別の XRPL アカウント向けに行った読み取りは拒否する。
- 命令エンコード（`src/signing.rs`、TC-239）: 32 バイト = `[0] instruction id` · `[1] walletId`（常に 0、「未割り当て」）· `[2..12] value`（10 バイト big-endian、最大 2^80−1、0 は拒否）· `[12..32]` 受取人（transfer: 20 バイトの EVM アドレス、ゼロアドレスは拒否）または 0（redeem）。`0x01` transfer: value = FXRP の**生の最小単位**（コントラクトはそれをそのまま `FXRP.transfer(to, value)` に渡す。オンチェーンでの変換は無い）、`0x02` redeem: value = **lots**。エンコーダは単位に依存しない。composer は 10 進の FXRP 量を**読み取った** `decimals()` で変換し（`signing::parse_token_amount`: 厳密な整数演算、`decimals` を超える小数桁は拒否、`decimals > 18` は拒否）、読み取った `fxrp_balance_units` と比較する（残高ちょうどは受理、+1 単位は拒否。TC-247）。読み取り／`decimals()` が未ロードの間はキュー投入を拒否する。エンコーダは TC-239 で参照ベクタ（value 10 / 1234567 の `0xf548…131d` 宛て transfer、redeem 3）に固定されている。
- 信頼境界: operator アドレスと fee は 1 回の Flare RPC 応答から来るため、operator が `[flare.smart_accounts] expected_operator` と等しく、fee が `1..=MAX_SMART_ACCOUNT_FEE_DROPS`（10 XRP）の範囲内のときだけ Payment に署名する。mainnet では `--yes` が必要。simulate は sign/submit に先行する。
- Memo 命令（`-x sa-memo <ignore-memo|set-nonce|replace-fee|set-executor|remove-executor> <XRP>`、TC-344）: レイアウトは upstream の `MemoInstructions.sol` による（`0xE0` skip memo、`0xE1` fast-forward nonce、`0xE2` replace executor fee、`0xD0` / `0xD1` executor pin）。Core Vault 宛ての Direct Mint Payment（`custom_memo` を使う C2 パイプライン。Core Vault pin、fee チェック、simulate 先行、mainnet では `--yes`、destination tag 無し）で、executor が `fxrp-execute <tx> --data 0x` で finalize する。`0xE1` は最初に personal account の `getNonce` を読む。
- カスタム命令 `0xFE`（`-x sa-call`、TC-345）: user operation は `PackedUserOperation{sender = personal account, nonce = getNonce, callData = executeUserOp(Call[]), rest empty}`（`abi.encode` 済み。controller は `keccak256(_data)` を memo と照合してから `abi.decode` する）。memo は `[0xFE | 0 | fee | keccak256(userOp)]`。executor はそのバイト列を `_data` として `executeDirectMintingWithData` に渡す（`fxrp-execute --data`）。呼び出しは FLR の value を持たない（`msg.value` は `fxrp-execute` が転送しない）。未実装: `0xFF`（インライン user operation）。
- Finalization（TC-366、`-x sa-status`）: `flare::smart_account_tx_used` が `MasterAccountController.isTransactionIdUsed(xrplTxHash)` を読む（Smart Account の支払い専用。同じ view は Coston2 で `sa-vault` の `0x12` operator instruction にも `true` を返した（F3-c）ので、`sa-status` はそれらにも答える。plain の recipient-memo C2 mint は**対象外** — Coston2 で実機確認済み: 公開 executor が finalize した plain C2 mint `FB8556…` は `false`）。`sa-memo` の Payment が validated になった後、`flare_cmd::await_public_executor` が 10 s 間隔で最大 300 s ポーリングし（`FINALIZE_POLL` / `FINALIZE_LIMIT`、`--no-wait` で省略）、誰も finalize しなければ `sa-status` と `fxrp-execute <tx> --data 0x` のフォールバックを表示する。コマンドは決して失敗させない。Payment は既に validated でハッシュも出力済みなので、Flare 読み取りの失敗は stderr に `sa-status` の再確認手順つきで報告し、終了コードは 0 のまま（非 0 だと Core Vault への 2 回目の支払いを誘う）。personal account に executor が固定されている場合、`0xE0` / `0xE1` / `0xE2` は待機を省略し（固定された executor しか finalize できない。`0xD0` / `0xD1` は固定を迂回する）、その旨を表示する。`sa-call` は待たない: 公開 executor は user-operation のバイト列を持たない（`0xFE` はそのハッシュしかコミットしない）。この view は情報表示のみで、署名や拒否には使わない。待機するのは、finalize が Flare 上の第三者のトランザクションだからである。AssetManager の `DirectMintingExecuted`（905 件、executor 10 者、受取先とは別）と `...ToSmartAccount`（1000+ 件、8 者）の 50 日間の mainnet サンプルでは XRPL の確定から 95–177 s（8 サンプル）で、公開 executor の存在は示せるが、executor 手数料の低い支払いを拾うこと（手数料下限は未測定）は示せない。また `executeDirectMintingWithData` が revert すると XRP は Core Vault に残る。
- 永続化: Smart Account に関するものは何も保存しない。表示されるすべて（personal account、nonce、operator wallets、命令 fee、FXRP 残高、vault ポジション）は Overview の poll ごとに Flare から読み、メモリ上にのみ保持する（`SmartAccountInfo`。300 秒より古い、または別ネットワークのスナップショットは拒否され、`verify_*_terms` は署名の直前に view を再読込する — TC-363）。`getPersonalAccount(xrplAddress)` は決定的なので覚えておく必要がなく、保存したコピーは（残高、nonce、operator、fee が変わるため）古くなるだけで、これらのガードを迂回してしまう。永続化する価値のある唯一のデータは、S-019 を閉じるための、ユーザーが検証した `[flare.smart_accounts] expected_personal_account` ピン（`expected_operator` と同様）だが、未実装（ROADMAP F5-b、P3）。lazyxrp が永続化するのはユーザーが書いた設定ファイルとログのみで、ウォレット、Smart Account、履歴のキャッシュは持たない。

---

## 7. 運用メモ

- `start_poll_task` = `PollContext` を伴う `tokio::spawn`。
- WS トリガーはデバウンスされる。定期 poll は `poll_interval_ms` を使う。dUNL と Flare レジストリのアドレスはセッション内でキャッシュされる（§3 参照）。
- リスク／負債: [`problems-ja.md`](problems-ja.md)、[`security-ja.md`](security-ja.md)。
