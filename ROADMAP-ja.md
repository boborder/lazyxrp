# ロードマップ — lazyxrp

[English](ROADMAP.md)

**役割:** 進捗管理（最終ゴール・優先順位・マイルストーンの SSOT）。注意点・進路変更記録 → [`docs/roadmap-log-ja.md`](docs/roadmap-log-ja.md)。

**計画ビュー。** 要件 → [`requirements-ja.md`](docs/requirements-ja.md) · システム挙動 → [`architecture-ja.md`](docs/architecture-ja.md) · UI/UX → [`DESIGN.md`](DESIGN.md) · 進路変更記録 → [`roadmap-log-ja.md`](docs/roadmap-log-ja.md)。

**現行バージョン:** `Cargo.toml` → 0.8.0（0.7.0 は 2026-10-16 公開）
**最終更新:** 2026-10-24

**優先度:** P0 = リリースブロッカー · P1 = 次マイルストーンの主目標 · P2 = 計画済み · P3 = 余力・需要次第 · 「Later」= 未スケジュール。各 horizon 内は上から着手。

## 最終ゴール

XRPL エコシステム（開発者・ユーザー双方）で人気の CLI/TUI ツールになること。**セキュリティの維持は非交渉条件** — guards を弱める機能は出さない（[`docs/security-ja.md`](docs/security-ja.md)）。

方向性（長期）: XRPL / Xahau / Flare の multichain 深掘りと普及（install path・onboarding）。着手は中期消化後に査定。

## 一目で見る

未完了の作業のみ。出荷済みは[出荷履歴](#出荷履歴)、出荷済みだが未検証の挙動は[検証負債](#検証負債)を参照。

| Item | Area | Priority | Detail |
|------|------|----------|--------|
| F4-d: mainnet で公開 executor が lazyxrp の支払いを拾うかの確認（要ユーザー判断） | C | P1 | [C](#c-flare-と-fxrp) |
| `0xFF` カスタム instruction（F4-c。Smart Accounts follow-up） | C | P2 | [C](#c-flare-と-fxrp) |
| Smart Account `0x01` / `0x02` の実機 E2E（検証負債） | C | P2 | [検証負債](#検証負債) |
| F2-d: FTSO feed 検証 | B | P3 | [B](#b-uiux-と表示) |
| F5-a: DA layer の `X-API-KEY` ヘッダ（提案） | C | P3 | [C](#c-flare-と-fxrp) |
| F5-b: S-019 を閉じる任意の `expected_personal_account` ピン（提案） | C | P3 | [C](#c-flare-と-fxrp) |
| F5-c: AssetManager `DirectMintingExecuted` event による C2 finalize 状態（提案・ブロック中） | B / C | P3 | [B](#b-uiux-と表示) |
| personal account の全桁表示 / コピー（提案） | B | P3 | [B](#b-uiux-と表示) |
| Vault* family（VaultCreate / Set / Delete / Deposit / Withdraw / Clawback） | A | P3 | [A](#a-xrpl-トランザクションとカバレッジ) |
| Xahau `NetworkID` の simulate 検証（problems P-007） | A | P3 | [A](#a-xrpl-トランザクションとカバレッジ) |
| Action エラー型移行（`String` → 型付き） | D | Later | [D](#d-品質セキュリティ基盤) |
| `wallet.rs` / `wallet_composer.rs` の god-file 分割 | D | Later | [D](#d-品質セキュリティ基盤) |
| Global row cache (A1) — render perf at scale | B | Later | [B](#b-uiux-と表示) |
| パネル内キー（`j`/`k`・`t`・composer）の設定化 | B | Later | [B](#b-uiux-と表示) |
| `vendored-openssl` feature | D | Later | [D](#d-品質セキュリティ基盤) |
| install の attestation / 署名（現状は SHA-256 checksum のみ） | D | Later | [D](#d-品質セキュリティ基盤) |
| AccountSet GlobalFreeze / RequireAuth の確認ステップ | D | Later | [D](#d-品質セキュリティ基盤) |

## A. XRPL トランザクションとカバレッジ

### 出荷済み

書き込み可能な TX 種別は **50 / 77**（TUI または CLI で write 経路あり。種別ごとの全表は [XRPL カバレッジ](ROADMAP.md#xrpl-coverage)（`ROADMAP.md` のみ）を参照）。Status **Existing** = 型ごとの手書き 12 種（Payment、AccountSet、TrustSet、OfferCreate、OfferCancel、SetRegularKey、EscrowCreate / Finish / Cancel、AMMDeposit / AMMWithdraw、AccountDelete）、Status **Spec** = `src/xrpl/txspec/` の宣言的 spec 38 種（family: NFT、XChain、Account objects（Check、Ticket、DepositPreauth、SignerListSet、Clawback）、Channel / AMM、MPT、Identity（Credential、DID、Oracle、PermissionedDomain））で、TUI composer「Transaction」+ `-x tx` が共通経路（`-x nft-mint` / `-x nft-burn` は別名）。family の追加 = `txspec/` に 1 ファイル + `FAMILIES` に 1 行（手順は `docs/architecture-ja.md` §2.1）。マイルストーン: F-01 AccountDelete（0.5.0、2026-10-11）、EscrowCreate / Finish / Cancel と AMM Deposit / Withdraw（0.4.0、2026-10-11）、レビュー修正バッチの XRPL 修正（2026-10-21）— 詳細は[出荷履歴](#出荷履歴)。

**実機検証の状況:** testnet で validated したのは NFT mint / burn、AMM、escrow、AccountDelete のみ（加えて先行する Payment / OfferCreate / OfferCancel 経路）。他の Spec family はオフラインテストのみ。Xahau: `network_id > 1024` で `NetworkID` を注入するが、**Xahau の simulate 受理は未検証**（problems P-007）。

**Spec family で意図的に省いている field:** DepositPreauth の credentials、MPT の Clawback、NFT `tfMutable`、価格ゼロの offer、Claim の `CredentialIDs`、SignerEntry の `WalletLocator`、DIDSet のクリア、MPT の mutation flags。

### 未着手・計画

| Item | Priority | 必要なもの |
|------|----------|-----------|
| Vault* family（6 種、Status Planned: xrpl-rust 1.3.0 に型付きモデルあり、未配線） | P3 | フラットな Amount 文法を拡張して Asset / Amount に MPT を許したうえで、VaultCreate / Set / Delete / Deposit / Withdraw / Clawback を 1 family として追加 |
| Xahau `NetworkID` の simulate 検証（P-007） | P3 | Xahau endpoint での実機確認（[検証負債](#検証負債)参照） |
| 省略中の Spec field（上記） | Later | field ごとの判断。いずれもスケジュールなし |

### やらないこと（と理由）

Unsupported の種別（署名不可）を理由別にまとめる:

| 理由 | 種別 | 解消するもの |
|------|------|--------------|
| xrpl-rust 1.3.0 に型付きモデルなし | NFTokenModify（dynamic NFT）、AMMClawback、Batch、DelegateSet、LedgerStateFix | モデルが追加された xrpl-rust へのアップグレード |
| xrpl-rust 1.3.0 に型付きモデルなし（XLS-66 lending） | LoanBrokerSet、LoanBrokerDelete、LoanBrokerCoverDeposit、LoanBrokerCoverWithdraw、LoanBrokerCoverClawback、LoanSet、LoanDelete、LoanManage、LoanPay | モデルが追加された xrpl-rust へのアップグレード |
| witness server が署名する attestation で、フォームでは作れない | XChainAddClaimAttestation、XChainAddAccountCreateAttestation | 対応不可 |
| ゼロ知識証明が必要。xrpl-rust に型付きモデルなし | ConfidentialMPTConvert、ConfidentialMPTConvertBack、ConfidentialMPTSend、ConfidentialMPTMergeInbox、ConfidentialMPTClawback | フォームでは対応不可。xrpl-rust のモデルも必要 |
| Planned だが Asset / Amount が MPT になりうり、フラットな Amount 文法では表現できない | VaultCreate、VaultSet、VaultDelete、VaultDeposit、VaultWithdraw、VaultClawback | 文法の拡張（未着手・計画を参照） |

擬似トランザクション（`EnableAmendment` / `SetFee` / `UNLModify`）は対象外。

## B. UI/UX と表示

### 出荷済み

出荷済みのパネル・composer（Account、FXRP、FTSO、Payment / Offer / Escrow / AMM / AccountDelete / Transaction / Smart Account の wallet composer、Watch ビュー）は [`DESIGN.md`](DESIGN.md) に記述。キーとレイアウトは同ファイル、挙動は `docs/architecture-ja.md`。ここで出荷した表示関連: Watch / Operate 分離（`--watch`、0.6.0）、AMM no-pool 表示（0.6.1）、FTSO の `Age` staleness 列（F2-d、2026-10-20）、FXRP パネルの `Max now` / `Cap used` / 停止 badge / vault ポジション（F1-c、F1-d、F2-c）、C2 composer の personal account prefill（F4-b、2026-10-24）、レビュー修正バッチの UI 修正（2026-10-21）— [出荷履歴](#出荷履歴)参照。

### 未着手・計画

| Item | Priority | Notes |
|------|----------|-------|
| F2-d: FTSO feed 検証 | P3 | `FastUpdatesConfiguration` の feed 一覧から `FLARE_FEEDS` を検証 / 候補表示（ABI 未確認）。feed 安定度はオンチェーンに無い（docs の web ページのみ）ので対象外 |
| パネル内キー（`j`/`k`・`t`・composer）の設定化 | Later | グローバルキーは設定可（DESIGN §6）。需要が出たら中期へ |
| Global row cache (A1) — render perf at scale | Later | 計測で問題が出たら |
| `wallet.rs` / `wallet_composer.rs` の god-file 分割 | Later | [D](#d-品質セキュリティ基盤)参照 |
| personal account の全桁表示 / コピー（**提案**） | P3 | 要判断 |
| F5-c: TUI での plain C2 mint の finalize 状態（**提案**・ブロック中） | P3 | ブロック理由: 公開 RPC の `eth_getLogs` が 30 block 未満の上限のため AssetManager の `DirectMintingExecuted` を引けず、`isTransactionIdUsed` は plain mint を対象にしない（F4-a）。explorer API か archive RPC = 新しい外部依存が要る → 要判断 |

### やらないこと（と理由）

- TUI での `sa-memo` / `sa-call` の公開 — 対象外と決定（2026-10-24）: `sa-memo` は回復 / 固定用、`sa-call` は EVM 鍵か userOp バイトを持つ executor が要るため。CLI のみ（方針は [C](#c-flare-と-fxrp)、理由は [`docs/roadmap-log-ja.md`](docs/roadmap-log-ja.md)）。
- FTSO feed 安定度の表示 — オンチェーンに無い。

## C. Flare と FXRP

### 出荷済み

Smart Accounts（2026-10-16、`[Unreleased]`）: 読み取り（personal account / FXRP 残高 + `decimals()` / nonce / operator / fee、FXRP パネル）+ composer `a`（`0x01` transfer・`0x02` redeem を operator へ Payment + 32-byte memo。transfer の額は読み取った `decimals()` で最小単位に整数変換）。署名は `[flare.smart_accounts] expected_operator` と一致時のみ。TC-239–247。読み取りは Coston2 / Flare mainnet で実機確認（FXRP `decimals()` = 6）。**`0x01` / `0x02` の書き込みはオフライン（local JSON-RPC）のみで実機未実施**。同じ instruction pipeline の vault instruction（`0x11` / `0x12`、`-x sa-vault`）は Coston2 実機で確認済み（F3-c）。Flare 機能の残りは下の取り込み計画（F1–F4、未完了の行を除きすべて出荷済み）。

### Flare 取り込み計画（2026-10-20 調査）

調査元: Flare Developer Hub（`dev.flare.network/llms.txt` 索引）・flare.network ニュース・vendored `.agents/skills/flare-*`・Flare mainnet RPC 実測。lazyxrp の現状は FTSOv2 読み取り（`getFeedsById`）/ AssetManagerFXRP の Direct Mint 読み取り + C2（Core Vault へ XRPL Payment + 32-byte memo）+ C3（`executeDirectMinting`、proof 手貼り）/ Smart Account `0x01` transfer・`0x02` redeem。以下は**未導入**。段階は依存順で、F1 は新しい設計判断なしに着手できる。

**背景（2026 年の変更点）:** FAssets v1.3（Songbird 先行 → mainnet、2026-04-28）で mint が agent 非依存の「Core Vault へ直接 Payment + executor が FDC proof を中継」に変わり、destination tag（MintingTagManager）、executor 制限、proof 束縛、時間 / 日次 / 大口の mint 制限（delay 方式）、`DirectMintingPaymentTooSmallForFee` が入った。Smart Accounts v1.3（2026-07-28）は mint + vault 入金を XRPL 署名 1 回に統合（Firelight stXRP / Upshift 系 vault、memo opcode `0xFE` / `0xFF` / `0xE0`–`0xE2` / `0xD0`–`0xD1`）。Firelight は 2026-10-06 に稼働。

| # | Item | Priority | 根拠 / 完了条件 |
|---|------|----------|----------------|
| F1-a | ~~C2 の最小額ガード~~ **済**（`[Unreleased]`、2026-10-20） | — | `flare::check_direct_mint_amount`: 額が `max(min fee, fee_bips 分) + executor fee` を超えないと simulate 前に拒否。TC-330。upstream `DirectMintingFacet._computeFees` で確認済み（`fee = min(max(received·bips, min), received)`、`executor = min(execFee, received − fee)`、recipient = 残り → recipient が受け取るのは `received > fee + executorFee` のときだけ）。この閾値は契約の条件そのもの |
| F1-b | ~~C3 の結果判定~~ **済**（`[Unreleased]`、2026-10-20） | — | `flare::decode_direct_minting_outcome`: AssetManager が emit した event のみで minted / delayed（再実行時刻つき）/ 手数料没収を判別。手数料没収は Err、delayed は Ok だが「未 mint」と明示。TC-331。実機の delayed は未再現（mainnet の制限に当たる額を送れない） |
| F1-c | ~~FXRP 停止状態の読み取り + C2 拒否~~ **済**（`[Unreleased]`、2026-10-20） | — | `mintingPaused()` / `emergencyPauseLevel()`（upstream `IAssetManager.sol` 宣言済み、`EmergencyPause.Level` = NONE / START_OPERATIONS / FULL / FULL_AND_TRANSFER の uint8）を `FxrpDirectMintInfo` に（読めなければ `None` = 不明でブロックしない）、FXRP パネルに badge、停止中は composer が C2 を拒否。mainnet 実測は両方とも停止なし。TC-332–333 |
| F1-d | ~~mint 制限の事前確認（read-only）~~ **済**（`[Unreleased]`、2026-10-20） | — | 同一バッチで hourly / daily limit + limiter state（`flare::window_max_now` で最新 block 時刻まで再計算。`MintingRateLimiter._processElapsedWindows` / `recordMinting`（`>=` で delay）と照合済み）・large threshold / delay・granularity・`totalSupply`・`mintingCapAMG`（`getSettings()` は 58 field で alloy のタプル上限超え → raw bytes の word 22 を読む）。`flare::classify_mint`、パネルに `Max now` / `Cap used`（est.）、composer は cap 超過のみ拒否（delay は表示のみ）。mainnet 実測 cap 170M XRP / large 4M XRP / 2h が docs と一致、Coston2 smoke receipt（4.8 FXRP）が宣言した event ABI で `Minted` に decode。**既知の上限**: window 幅（1 h / 1 d）は governance が `setWindowSizeSeconds` で変更でき getter が無いため固定値を仮定。TC-334–336 |
| F2-a | ~~MintingTagManager 読み取り~~ **済**（`[Unreleased]`、2026-10-20） | — | `-x fxrp [--tag N]`: `getMintingTagManager()` → `reservationFee` / `nextAvailableTag` / `executorChangeAfterSeconds`、`--tag` で `ownerOf` / `mintingRecipient` / `allowedExecutor` / `pendingAllowedExecutorChange`（ABI は upstream `IMintingTagManager.sol` と照合済み。未予約は revert のみ `reserved: false`）。mainnet 実測: 次の tag 812、cooldown 600 s、tag 30 が予約済み。pending 変更ありの実例は未観測（`null` のみ確認）。TC-337 |
| F2-b | ~~償還キュー・agent 一覧（read-only）~~ **済**（`[Unreleased]`、2026-10-20） | — | `-x fxrp` が `redemptionQueue(next, maxRedeemedTickets)` を `next == 0` まで読み（最大 100 page）、ticket 数・合計 XRP / lot・上位 10 agent・`getAllAgents`（6 vault）・Core Vault（`coreVaultAvailableAmount`）を表示。**整合確認**: Core Vault 141,910,803 XRP + キュー 1,541,120 XRP = 143,451,923 XRP ≒ `totalSupply` 143,451,916 XRP（差 約 7 XRP = 別 RPC 呼び出し間の時間差）。direct mint は ticket を作らず（`DirectMintingFacet` に ticket 生成なし）、Core Vault 移送で agent の ticket が消える。`agentRedemptionQueue` は全体キューの agent フィルタと同じ内容なので別実装せず、`getAgentInfo` は 50 超 field の struct で表示価値が低いので対象外 |
| F2-c | ~~vault ポジション読み取り~~ **済**（`[Unreleased]`、2026-10-20） | — | vault アドレスの取得元は on-chain の `MasterAccountController.getVaults()`（id / address / type: 1 = Firelight, 2 = Upshift）で、固定値もピンも不要。mainnet 実測 3 vault（Firelight ×1、Upshift ×2）。FXRP パネルに personal account の持ち分（`balanceOf` + ERC-4626 `convertToAssets`、無ければ share 数）を表示。読めない vault は黙ってスキップ。ポジションありの実例は未観測（残高ゼロのアドレスで空になることのみ確認）。`convertToAssets` が Upshift に存在するかは未確認。TC-339 |
| F2-d | FTSO の staleness 表示 **済**（`[Unreleased]`、2026-10-20）。feed 検証は未対応 | P3 | FTSO パネルの `Time`（生の unix 秒）を `Age`（`2s` / `5m` / `3h`）に置換、60 s 超 / timestamp 0 は warning 色（`feed_age`、TC-338）。timestamp が unix 秒で新しいことは mainnet 実測（全 feed が現在時刻 ±120 s、live test）。未対応: `FastUpdatesConfiguration` の feed 一覧から `FLARE_FEEDS` を検証 / 候補表示（ABI 未確認）。feed 安定度はオンチェーンに無い（docs の web ページのみ）ので対象外 |
| F3-a | ~~tag 方式 mint の Flare 書き込み~~ **済**（`[Unreleased]`、2026-10-20） | — | `-x fxrp-tag-reserve`（`reserve()`、`reservationFee` を FLR で支払い、`MintingTagReserved` から tag 番号を表示）/ `fxrp-tag-set <tag> [--recipient] [--executor]` / `fxrp-tag-transfer <tag> <to>`。全て owner 事前確認・ゼロアドレス拒否・`estimate_gas`（= simulate）後に送信、`execute = true` + EVM key + Flare mainnet は `--yes`。**実送信は一度も行っていない**（読み取り側の拒否経路のみ mainnet で確認: 未予約 FLR 残高 0 / owner 不一致）。TC-341 |
| F3-b | ~~EVM 側 redeem~~ **済**（`[Unreleased]`、2026-10-20） | — | `-x fxrp-redeem <XRP> <r-address> [--tag N]`: `redeemAmount` / `redeemWithTag`（tag は 32 bit）。`minimumRedeemAmountUBA`（mainnet 5 XRP 実測）・FXRP 残高を事前検査し `estimate_gas` 後に送信。**Coston2 で実送信成功（2026-10-20）**: approve なしで `redeemAmount` が通り（FXRP の burn Transfer + `RedemptionRequested`、`RedemptionAmountIncomplete` なし）、approve 不要が実機で確定。`redeemWithTag --tag 12345` も成功し、receipt に `RedemptionWithTagRequested`（topic0 `0x69d3fbff…`、destinationTag = 0x3039 = 12345）を確認。partial redemption の警告経路（`RedemptionAmountIncomplete` の topic0）は実機で再現できておらず未確認。TC-341 |
| F3-c | Smart Account 拡張 **済**（`0xFF` のみ未対応。`[Unreleased]`、2026-10-20） | — | vault instruction `0x11`–`0x13` / `0x21`–`0x23` を `-x sa-vault`（既存 instruction パイプライン。vault id は `getVaults()` で登録 + type 照合、fee は `getInstructionFee(id)`。value の単位は upstream `Instructions.sol` / `PaymentReferenceParser.sol` で確認済み: deposit = FXRP 最小単位、redeem = shares、claim = period / `YYYYMMDD`。TC-342）、`executeDirectMintingWithData`、memo 回復系 `0xE0` / `0xE1` / `0xE2` / `0xD0` / `0xD1` を `-x sa-memo`（TC-344）、カスタム instruction `0xFE` を `-x sa-call`（TC-345）。**Coston2 実機（2026-10-20）**: `0xD1`（公開 executor が finalize、`isTransactionIdUsed(tx hash)` = true）、`0xD0`（自分の鍵を pin、`getExecutor` で確認）、`0xE2`、`0xE1`（`--target` mint 済みを確認して送信、nonce 0 → 3）、`0xE0`（finalize 前の `0xFE` を対象に送信 → 先に `0xE0` を finalize → 元の `0xFE` を finalize すると FXRP は mint されるが userOp は実行されない: nonce は 4 のままで、`FXRP.transfer` による +1 FXRP は受取側に現れない。受取側の残高 1.0 → 1.1 FXRP の +0.1 は元の `0xFE` の finalize receipt の executor 手数料 100000 UBA の Transfer）、`0xFE`（`FXRP.transfer` が personal account から実行され、受取側 5 → 6 FXRP、nonce 3 → 4）、`sa-vault firelight deposit 1 1000000`（vault 1 の shares 999260、value = FXRP 最小単位）、`sa-vault firelight request-redeem 1 500000`（`0x12`。送信 tx `84EBBD8C…` の `isTransactionIdUsed` = true を引数付きで確認。operator が数分後に実行し、shares 999260 → 499260、500000 shares の burn と出金リクエスト event を vault の log（tx `0xc1a818f4…`）で確認。送信から約 3 分は残高が変わらず operator の実行に遅延がある）。operator `rEyj8nsH…` の pin は **公式一覧との照合ではなく相互確認**: `getXrplProviderWallets()` を独立した 3 つの Coston2 RPC で読んで一致、XRPL testnet 上でも多数の別アカウントと取引（testnet のため許容。mainnet では out of band で確認すること）。**未確認**: `sa-vault` の claim（`0x13` は期間の経過待ち。出金リクエスト event の最初の data word が `0x214` = 532 で、これが period の候補: `sa-vault firelight claim 1 532`。event の意味は vault の ABI を未確認なので推定）。**未対応**: `0xFF`（memo 内 userOp。需要が出たら）|
| F3-d | ~~FDC proof 自動取得~~ **済**（`[Unreleased]`、2026-10-20、ユーザー承認） | — | `-x fxrp-execute <XRPL tx hash> [--data HEX] [--round N]`: verifier `prepareRequest` → `FdcHub.requestAttestation`（mainnet 実測 **20 FLR**、Coston2 は 1000 wei）→ `Relay.isFinalized(200, round)` 待ち（RPC の一時切断は連続 5 回まで許容）→ DA layer proof → `executeDirectMinting`。attestation 後に round を表示し、途中で落ちたら `--round N` で再手数料なしに再開。信頼境界: verifier 応答の `abiEncodedRequest` を tx hash / proofOwner / 型 / source と byte 照合、https 限定・redirect 無効・応答 1 MiB 上限・fee 上限 100 FLR、proof は contract が Merkle 検証。**Coston2 実機で一連を確認（2026-10-20）**: `sa-memo` の Payment（XRPL testnet）→ attestation → round 確定 → DA proof → 自分の鍵からの `executeDirectMintingWithData` が成功（「minted 4.9 FXRP … routed to the personal account」）。公開 executor が先に finalize する環境では、自分を `sa-memo set-executor` で pin して回避（pin 後は自分の鍵だけが finalize できる）。`--round` 再開も実機で確認（手数料なし）。TC-340 |
| F4-a | ~~`sa-memo` の finalize 待機・`sa-status`~~ **済**（`[Unreleased]`、2026-10-24） | — | `flare::smart_account_tx_used`（`isTransactionIdUsed`）を 10 s 間隔・最大 300 s で見て、公開 executor が finalize したか表示。未完了なら `sa-status` と `fxrp-execute <tx> --data 0x` を案内。待機は支払いの validated 後なので失敗させない（読み取りエラーは stderr + exit 0）。executor 固定中の `0xE0` / `0xE1` / `0xE2` は待たず固定 executor を案内。`sa-call` は userOp バイトが無い executor には finalize できないので待たない。**plain の recipient-memo mint（C2）にはこの view が効かない（Coston2 で、公開 executor が finalize した C2 の tx が `false`）**ため、TUI は FXRP 残高の確認 / C3 を案内するのみ（AssetManager の `DirectMintingExecuted` を引くには公開 RPC の `eth_getLogs` が 30 block 未満の上限で不向き）。`sa-status` は `sa-vault` の operator instruction にも使える（F3-c で `0x12` が `true`）。TC-366。mainnet の `sa-status` は読み取りで確認（finalized 済み tx / 未使用 hash） |
| F4-b | ~~C2 受取先を自分の personal account で既定入力~~ **済**（`[Unreleased]`、2026-10-24） | — | 署名アカウントの Smart Account 読み取りが 300 s 以内のときだけ prefill。全桁表示。**新しい信頼境界**: personal account は Flare RPC の単一応答（pin なし）→ security.md S-019 / R-019 を **accepted** として記録。TC-367。個別の「副作用なし mint」op は追加しない（upstream に純粋な mint 用 opcode はなく、C2 の plain memo + personal account 受取先で足りる）。personal account への plain mint が FXRP を personal account に残し `0x01` / `0x02` で使えることは **Coston2 / mainnet とも実機未確認**（`0xFE` 経由の mint は Coston2 で確認済み） |
| F4-c | `0xFF` カスタム instruction（下の「未着手・計画」表の行） | P2 | 公開 executor で完結できる XRPL 署名のみのカスタム経路。Coston2 実機確認が前提 |
| F4-d | **mainnet で公開 executor が lazyxrp の支払いを拾うかの確認** | P1（要ユーザー判断） | 2026-10-24 の読み取り調査（`docs/test.md`）: 通常 905 件 / 10 executor、Smart Account 1000+ 件 / 8 executor、XRPL 確定から 95–177 s（8 件）。**未確認**: executor の手数料下限（低手数料の支払いが拾われるか）・executor の運営者・`0xD1`（固定なし）の mainnet での通過。確認には mainnet への少額実送信が要る。失敗時は XRP が Core Vault に残り、自分で finalize するには EVM 鍵・約 20 FLR + gas が必要。mainnet で試すなら、Coston2 で動作確認済みの C2（plain mint）を少額で。`sa-memo remove-executor` は未検証の `0xD1` に依存するため避ける |
| F5-a | DA layer の `X-API-KEY` ヘッダ（verifier には送信済み）（**提案**） | P3 | mainnet でレート制限に当たってから追加（方針を参照） |
| F5-b | S-019 を閉じる任意の `[flare.smart_accounts] expected_personal_account` ピン（**提案**） | P3 | 未実装。[データ永続化の判断](#データ永続化の判断)参照。要判断 |
| F5-c | AssetManager `DirectMintingExecuted` event による C2 finalize 状態（**提案**・ブロック中） | P3 | [B](#b-uiux-と表示)の通りブロック: explorer API か archive RPC = 新しい外部依存が要る → 要判断 |

### 未着手・計画

| Item | Priority | Notes |
|------|----------|-------|
| F4-d mainnet の executor 確認 | P1 | 要ユーザー判断（上の行を参照） |
| `0xFF` / F4-c — memo 内に userOp を直接載せるカスタム instruction（1024 byte 上限） | P2 | 未対応。**公開 executor が userOp のバイトなしで中継できる唯一のカスタム instruction**（upstream skill: 「executor と調整したくないなら `0xFF`」）。`0xFE` は executor が userOp バイトを必要とするため、XRPL 署名のみの利用者には自分の EVM 鍵か信頼できる executor が要る。実装は小さい（`-x sa-call` の memo が `[0xFF｜wallet｜fee｜userOp]` になるだけ）が、1024 byte 上限・revert 時に XRP が Core Vault に残る点を Coston2 実機で確認するまで出さない。`docs/architecture-ja.md` §6.9、F4-c。2026-10-24 に Later から P2 へ引き上げ |
| Smart Account `0x01` / `0x02` の実機 E2E | P2 | 検証負債: オフラインテストのみ |
| F5-a / F5-b / F5-c | P3 | 提案。計画表を参照 |

### やらないこと（と理由）

**方針（2026-10-24、理由は [`docs/roadmap-log-ja.md`](docs/roadmap-log-ja.md) の同日の項）:** XRPL 署名のみを既定の経路（読み取り・Smart Account instruction・XRP の送金・mint 先 = 自分の personal account）とし、Flare EVM 鍵は「自分で finalize する」「EVM 側 redeem / tag」用の任意機能に留める。FCC / PMW・FLR staking・OFT・自前 executor ボット・TUI での `sa-memo` / `sa-call` は対象外（`sa-memo` は回復 / 固定用、`sa-call` は EVM 鍵か executor が要るため）。DA layer への API key 送信（`X-API-KEY`、verifier には送信済み）は mainnet でレート制限に当たってから追加する（F5-a）。

**対象外（理由）:** FCC（TEE extension を作る開発者向け基盤で、クライアント機能ではない）· FLR の staking / delegation / FlareDrops / rFLR（lazyxrp の XRP ↔ FXRP 軸から外れる）· FXRP OFT ブリッジ送信（LayerZero 手数料・DVN・per-endpoint rate limit の write 面が大きい。読み取りだけなら `FAssetOFTAdapter` の token fee / limit を F2 に追加可）· gasless FXRP（EIP-712 + relayer、x402）· Web2Json（mainnet / Songbird に source なし）· FBTC / FDOGE（FXRP 以外は mint 経路が別）。

**未解決:** `fsa.flare.network` は 2026-10-20 時点で「The FAsset System is currently paused for XRP」と表示するが、同日 mainnet の AssetManagerFXRP は `mintingPaused()` / `emergencyPaused()` / `emergencyPauseLevel()` がすべて false / 0（`web3_sha3` で selector を出して `eth_call`）。banner の根拠は未特定（FSA 側の別条件か表示の遅れ。`terminated()` は revert で存在しない）。計画は停止状態に依存しないので観測のみ記録し、F1-c 実装時に再確認する。

### データ永続化の判断

Smart Account については何も保存しない。表示する値（personal account・nonce・operator wallet・instruction fee・FXRP 残高・vault ポジション）はすべて Overview の poll ごとに Flare から読み、メモリ上にのみ保持する（`SmartAccountInfo`。300 s より古い snapshot や別 network の snapshot は拒否し、`verify_*_terms` は署名直前に view を読み直す — TC-363）。`getPersonalAccount(xrplAddress)` は決定的なので覚えておくものがなく、保存したコピーは古くなる（残高・nonce・operator・fee は変わる）うえにこれらの guard を迂回してしまう。永続化する価値があるデータは、S-019 を閉じるための、ユーザーが検証した `[flare.smart_accounts] expected_personal_account` ピン（`expected_operator` と同様）だけだが、未実装（ROADMAP F5-b、P3）。lazyxrp が永続化するのはユーザーが書いた設定ファイルとログのみで、wallet・Smart Account・履歴のキャッシュは持たない。

## D. 品質・セキュリティ・基盤

### 出荷済み

監査 run-2（0.3.1、TC-204–214）とその残り 5 件（https→http redirect・IPv6 埋め込み IPv4 / `fec0::/10` SSRF・bidi / 書式文字・tx 時刻 overflow・`install.sh` https 限定）は `[Unreleased]` で修正（TC-236–238）。レビュー修正バッチ（セキュリティ・正確性、2026-10-21、TC-346–362）。ponytail 監査 ×2（0.6.1）。リリース運用（Release 本文の自動生成、`mise run release <ver>`、週次 CI の advisory / yanked。手順は `docs/maintenance-ja.md`）。OSS パッケージ整備、dependabot + Actions SHA 固定（0.3.1）。[出荷履歴](#出荷履歴)参照。

### 未着手・計画

残りの項目は完了条件が未定義のため、着手前に査定する。

| Item | Priority | Notes |
|------|----------|-------|
| Action エラー型移行（`String` → 型付き） | Later | `docs/security-ja.md` の構造課題。UI 文言の SSOT 化と同時に |
| `wallet.rs` / `wallet_composer.rs` の god-file 分割 | Later | レビュー修正バッチで**意図的に先送り** |
| `vendored-openssl` feature | Later | レビュー修正バッチで**意図的に先送り** |
| install の attestation / 署名 | Later | レビュー修正バッチで**意図的に先送り**（現状は SHA-256 checksum のみ） |
| AccountSet GlobalFreeze / RequireAuth の確認ステップ | Later | レビュー修正バッチで**意図的に先送り** |

### やらないこと（と理由）

この領域で「やらない」と決めたものはない。上の項目は先送りであって却下ではない。guards を弱めるものは最終ゴールにより対象外。

## 検証負債

出荷済みだが実機で未検証の挙動。

| Item | 未検証の内容 | 閉じるために必要なこと |
|------|-------------|----------------------|
| Flare mainnet の書き込み | 一度も送信していない（C3 mainnet smoke 未実施。検証済みは testnet / Coston2 のみ） | mainnet での少額実送信（ユーザー判断。EVM 鍵と FLR が要る） |
| `fxrp-tag-*` の送信（F3-a） | `-x fxrp-tag-reserve` / `fxrp-tag-set` / `fxrp-tag-transfer` は一度も実送信していない。読み取り側の拒否経路のみ mainnet で確認 | 実送信（まず Coston2） |
| Smart Account `0x01` / `0x02` の実機 E2E | オフライン（local JSON-RPC）のみ | Coston2 での実際の transfer / redeem |
| `sa-vault` の claim `0x13` | 期間の経過待ち。候補の period 532（`sa-vault firelight claim 1 532`）は event の最初の data word `0x214` からの推定で、event の意味は vault の ABI で未確認 | 期間経過後に claim を実行、vault ABI の確認 |
| `fxrp-redeem` の partial-redemption 警告経路（F3-b） | 実機で再現できていない（`RedemptionAmountIncomplete` の topic0） | 実際の partial redemption |
| delayed mint（`DirectMintingDelayed`）の実機（F1-b） | 未再現（mainnet の制限に当たる額を送れない） | 制限に当たる mint |
| Upshift の `convertToAssets`（F2-c） | Upshift に存在するか未確認 | Upshift vault への読み取り |
| shares が 0 でない vault ポジション（F2-c） | 残高ゼロのアドレスで空になることのみ確認 | 実際のポジション |
| mainnet で公開 executor が低手数料の支払いを拾うか + pin なしの `0xD1`（F4-d） | executor の手数料下限・運営者・`0xD1`（固定なし）の mainnet での通過 | mainnet での少額実送信（ユーザー判断） |
| Xahau の `NetworkID` 付き simulate（P-007） | 注入した `NetworkID`（`network_id > 1024`）を Xahau の simulate が受理するか | Xahau endpoint での実機確認 |
| pending executor 変更の観測（F2-a） | pending 変更ありの実例は未観測（`null` のみ） | pending executor 変更のある tag |
| personal account への plain mint が `0x01` / `0x02` で使えるか（F4-b） | Coston2 / mainnet とも未確認（`0xFE` 経由の mint は Coston2 で確認済み） | personal account 宛ての plain C2 mint の後に `0x01` / `0x02` |

## XRPL カバレッジ

**表は `ROADMAP.md` のみにある**（[ROADMAP.md の XRPL coverage](ROADMAP.md#xrpl-coverage)）。**SSOT は `src/coverage.rs`**（`lazyxrp -x coverage` が同じ表を出力）。表は手で更新せず、出力を貼り直す — `roadmap_table_matches_registry`（TC-248）が行ごとの不一致（Type / Read / TUI / CLI / Status。Note 列は自由記述）で失敗する。Status: **Existing** = 型ごとの手書き write 経路 · **Spec** = `src/xrpl/txspec/` の宣言的 spec（TUI composer「Transaction」+ `-x tx` が共通経路。`-x nft-mint` / `-x nft-burn` は別名） · **Planned** = xrpl-rust 1.3.0 に型付きモデルあり、未配線 · **Unsupported** = 型付きモデルなし（署名不可）。Read = TX detail パーサ登録済み。擬似トランザクション（`EnableAmendment` / `SetFee` / `UNLModify`）は対象外。

書き込み可能 **50 / 77**（TUI または CLI で write 経路あり。残りの Planned（Vault 系）と Unsupported は表の Note と [A](#a-xrpl-トランザクションとカバレッジ)を参照）。新 family の追加 — 1 family = `txspec/` に 1 ファイル + `FAMILIES` に 1 行、手順は `docs/architecture-ja.md` §2.1。

Unsupported / Planned の種別の理由別グルーピングは [A](#a-xrpl-トランザクションとカバレッジ)の「やらないこと（と理由）」にある。

## 出荷履歴

時系列。中期（0.4.0）・短期・出荷済みの各表をここに統合した。Flare 取り込み F1–F4 の全行は [C](#c-flare-と-fxrp) にある。取り消し線は完了したロードマップ項目。

| 日付 | バージョン | 項目 | 備考 |
|------|-----------|------|------|
| — | 0.1.23 | Quality pass | **済** |
| — | — | Agent/docs hygiene | **済** |
| 2026-09-12 | 0.2.0+ | Wallet TX + FXRP | **済**。C3 mainnet smoke → 中期（P2） |
| — | — | Docs / agent hygiene | **済**。人間向け doc index は README、`docs/` は自己完結 |
| — | — | Ratatui optimization | **済**。dirty render + 共有 table を出荷 |
| 2026-09-12 | — | Docs hierarchy redesign | **済**。keys → `DESIGN.md`、architecture = 挙動、`docs/README` と `docs/tasks` を削除 |
| 2026-09-12 | 0.2.5+ | Network multichain | **済**。Xahau + `<Ctrl-n>` セッション切替 + Flare 表示 + wallet 読み取りパネル |
| 2026-09-16 | 0.2.9 | テストカバレッジ / ローカル統合パス | **済**。ローカル RPC の読み取り専用 CLI カバレッジ、`lazyxrp`/`rp` バイナリ smoke test、Assets とパネル状態のカバレッジ |
| 2026-09-17 | 0.2.10 | レビューバッチ #3: production guard + テストカバレッジ | **済**。Xahau の production write guard、ローカル CLI RPC 移行、nft_image SSRF 強化、バイナリ smoke test、alloy 2 / MSRV 1.94.1 |
| 2026-09-17 | 0.2.11 | Flare 読み取り経路の安定化 | **済**。FTSOv2 `getFeedsById` のバッチ化（5→1 RPC/poll）、fee-aware retry、`FXRP/USD`→`XRP/USD` premap、AssetManager / wallet 読み取りを `try_join!`、Flare のみの失敗 cooldown（2→60s）を XRPL backoff から分離 |
| 2026-09-17 | 0.2.11 | XRPL poll 効率化 | **済**。dUNL を XRPL backoff の双方向から除外 + エラー時 stale 表示と 60 s retry cooldown、Account タブのパネル（`account_tx`、wallet overview）を Market/Assets と同様にゲート |
| 2026-09-17 | 0.2.11 | ステータスバーの health 表示 | **済**。XRPL chip は静かな backoff 中に STALE、`server_info` エラーで OFFLINE、flare ドット（`flr:`）は失敗で赤・stale でオレンジ、`dunl:` のデータ経過マーカーは cache TTL 超過で `!` |
| 2026-10-10 | 0.3.0 | DEX trading + CLI 再設計 + keymap docs | **済**。Wallet composer の OfferCancel（`8`）+ Payment の Send max / `p` path quote（`slippage_bps`）、CLI: 引数なしサブコマンドと `watch` を削除、`-x send` は simulate-first + 失敗時 non-zero exit、`-x offers` / `-x offer-cancel`、Payment 検証の共通化（P-004）、live `simulate` の LLS 補完（TC-200）、`--seed`/`--mnemonic` は stderr 警告つきで維持（FR-13）、グローバルキーバインドはユーザーがマージ可能（DESIGN §6）、`rc` エイリアスは README のみ |
| 2026-10-11 | 0.3.1 | Live 検証 + testnet DX + 監査 run-2 | **済**。`-x faucet` / `-x offer-create`、CLI write の validated 待ち、endpoint `network_id` 照合（S-017）、opt-in testnet E2E、監査 run-2 修正（TC-204–214、Core Vault ピンは破壊的）、OSS パッケージ整備、dependabot + Actions SHA 固定 |
| 2026-10-11 | 0.3.1 | 中期 #2 — ~~Sequence / LastLedgerSequence 整合検査~~ | **済**（0.3.1、監査 run-2）。出典: `docs/security-ja.md` Fee cap 節の残余。P2。TC-204 |
| 2026-10-11 | — | 短期 #1 — ~~Testnet E2E — TUI~~ | **済**（tmux で操作）。OfferCreate（即約定）・非約定 OfferCreate → OfferCancel・`p` 見積り → cross-currency Payment（1 USD 着金）を validated。hash は `docs/test-ja.md` Testnet E2E |
| 2026-10-11 | — | 短期 #2 — ~~リリース運用の最適化~~ | **済**。Release 本文を自動生成（前タグからのコミット + GitHub 生成ノート）、`mise run release <ver>`（bump + lock 同期 + gates + 1 コミット）、週次 CI（advisory / yanked）。手順は `docs/maintenance-ja.md` |
| 2026-10-11 | — | 短期 #3 — ~~v0.3.0 Release 本文の後埋め~~ | **済**。破壊的変更・移行リンク・コミット一覧を記入 |
| 2026-10-11 | — | 中期 #1 — ~~`poll.rs` 分割~~ | **済**。出典: `docs/security-ja.md` 構造課題。P1。書き込み経路（`submit_*`・`simulate_then_finalize`・quote・関連テスト）を `xrpl/submit.rs`（1,526 行）へ移動、`poll.rs` は 2,633 → 1,120 行。挙動不変（260 テスト緑、新規 TC なし） |
| 2026-10-11 | — | 中期 #3 — ~~B-02 EscrowCreate composer UI~~（F-03 Finish/Cancel を前倒し） | **済**。出典: `docs/problems-ja.md` P-003。P2。実際は signing も未実装（poll が黙って捨てていた）。signing + `submit_escrow_*` + composer `9` Create（相対時間 `90s`〜`7d`、CancelAfter 任意）・`0` 一覧から Finish/Cancel、TUI のみ。TC-215–221、testnet で Create ×2 / Cancel / Finish を validated |
| 2026-10-11 | — | 中期 #4 — ~~F-04 AMM Deposit/Withdraw TX~~ | **済**。DEX trading の後続。P2。two-asset deposit・withdraw 全額 / LP 量。composer `d` / `w` + `-x amm-deposit` / `-x amm-withdraw`。TC-222–226、testnet で CLI ×2 / TUI ×2 を validated（プールは使い捨てスクリプトで作成、AMMCreate は対象外） |
| 2026-10-11 | — | 中期 #5 — ~~Fake WebSocket integration fixtures~~ | **済**。出典: `docs/test-ja.md`。P2。TC-227。fixture が実バグを検出: `xrpl_receive` が stream メッセージの `type` を落とし、ledger close / account tx の WS イベントが一度も届いていなかった（interval poll のみで動作）。生フレームを読むよう修正、testnet で ledger close 受信を確認。panel 側の反映は既存 TC（account.rs）が担保 |
| 2026-10-11 | — | 中期 #6 — ~~二重送信ガード（同一内容の連続 submit）~~ | **済**。出典: `docs/security-ja.md` 未修正リスト。P3。TC-228。WalletPanel の出口 1 か所で、同一 params の write を結果（Ok/Err）到着まで拒否。CLI は validated 待ちのため対象外 |
| 2026-10-11 | — | 中期 #7 — ~~`account_offers` marker paging（> 200 件）~~ | **済**。出典: `client.rs` ponytail 注記。P3。TC-229。200 件/page で marker を追う（上限 50 page）。TUI OfferCancel と `-x offers` の両方 |
| 2026-10-11 | — | 中期 #8 — ~~C3 execute smoke (`execute=true`)~~ | **済**（testnet）。P3。ユーザー判断で実資金は testnet（XRPL testnet + Coston2）。検証で C2 の simulate 拒否（memo null、TC-230）と C3 の ABI 不一致（存在しない selector、TC-231）を発見・修正し、C2 → 公開 executor による mint、48-byte memo → lazyxrp C3 で FXRP 9.8 → 14.7 を確認。mainnet は未実施 |
| 2026-10-11 | 0.4.0 | write 基盤整理 + TX 拡張 | **済**。`poll.rs` → `submit.rs` 分割、EscrowCreate / Finish / Cancel（TUI）、AMM Deposit / Withdraw（TUI + `-x`）、`amm_info` 実応答形の parse 修正（TC-015）。testnet で Escrow ×4・AMM ×4 validated。依存順は `poll.rs` 分割 → EscrowCreate / AMM（submit 種別追加は分割後） |
| 2026-10-11 | 0.4.1 | 中期 #5–#7 | **済**。WS イベント未認識の修正（TC-227）、二重送信ガード（TC-228）、`account_offers` paging（TC-229） |
| 2026-10-11 | 0.4.2 | C3 testnet smoke | **済**。FXRP C2 simulate の null 拒否修正（TC-230）、C3 ABI 修正（TC-231）。XRPL testnet + Coston2 で mint / execute を確認 |
| 2026-10-11 | 0.5.0 | AccountDelete（長期 F-01 — TX surface completion） | **済**。composer `x` + `-x account-delete`、TUI は `DELETE` 入力の確認必須（TC-232–233）。Fee（owner reserve）は simulate 由来。testnet で TUI（X-address tag 付き）/ CLI validated |
| 2026-10-11 | 0.6.0 | Watch / Operate 分離 | **済**。`--watch` 閲覧専用起動（TC-234–235）。ユーザー選択: `--watch` フラグ。全鍵を起動時に破棄しアドレスだけ表示、TUI は Account パネル（composer・鍵生成なし）、`-x` 署名系は拒否 |
| 2026-10-11 | 0.6.1 | AMM no-pool 表示 + ponytail 監査 ×2 | **済**。プールなしペアでステータスバーにエラーを出さずパネル表示に。依存 7 個削除（`config`・`signal-hook`・`tracing-error`・`strum`・`url`・`time`・build-dep `anyhow`）、挙動不変 |
| 2026-10-16 | `[Unreleased]` | Flare Smart Accounts — proof-based FXRP transfer / redeem | **済**。[C](#c-flare-と-fxrp)の出荷済みを参照（TC-239–247。`0x01` / `0x02` の書き込みはオフラインのみ） |
| 2026-10-20 / 2026-10-24 | `[Unreleased]` | Flare 取り込み計画 F1–F4 | F4-c / F4-d を除き**済**。全行は [C](#c-flare-と-fxrp)（TC-330–345、TC-366–367） |
| 2026-10-21 | `[Unreleased]` | レビュー修正バッチ（セキュリティ・正確性） | **済**。Flare: `eth_chainId` 検証（`expected_chain_id` / `check_write_chain` / `verify_rpc_chain`、`--yes` 判定は報告された chain id、全 Flare サブコマンドの先頭で 1 回）、TUI C3 も Flare network で gate（`PollContext.flare_network`）、`check_network_pair` を `flare.rs` に移し TUI C2 / Smart Account / C3 に適用、`send_capped` で 10 FLR 手数料上限（`MAX_TX_FEE_WEI`）、FDC は round パラメータ確認後に支払い（tx hash + `--round` 再開ヒント）、EIP-55 強制、`tag_owner` は revert のみ未予約扱い、`sa-memo` / `sa-call` の cap 検査、oracle 未設定は `XrplOracleNotConfigured`。XRPL: write params に compose 時 `network`（`stale_write_refusal`）、`submit_signed_tx` 単発 + `tx(hash)` 照会、`terQUEUED` = queued 成功、ask 価格 1e6/q 修正、`network_id > 1024` で `NetworkID` 注入（**Xahau の simulate 受理は未検証**、problems P-007）、WS 30 s read timeout + network 切替で再接続、toml 検証の base58 / hex、activation = `account_info` 成功、dUNL VL / field-id、通貨表示規則、strict `parse_drops`、IPv6 NFT URI。設定: keybinding allowlist（`is_bindable`）、実際に読んだファイルの権限検査、`~` 展開、資格情報優先順位（CLI > env > file）、signing.rs の zeroize、release `overflow-checks = true`、RUSTSEC-2026-0235 ignore 撤去（rkyv は Cargo.lock に無い）、`install.sh` は binary install で Rust を既定導入しない（`--install-rust`）。UI: 表示中タブのみキー / paste、Overview にウォレット無し、TrustSet 空開始、NoFreeze は `NOFREEZE` 入力、Flare snapshot は NetworkChange で消去・300 s 超は拒否、IOU 通貨欄 40 文字、splash は `server_info` エラーで終了、fetch 重複排除（TC-346–362） |
| — | `[Unreleased]` | 監査 run-2 の残り 5 件 | **済**。https→http redirect・IPv6 埋め込み IPv4 / `fec0::/10` SSRF・bidi / 書式文字・tx 時刻 overflow・`install.sh` https 限定（TC-236–238） |

長期項目の出荷: F-01 AccountDelete（0.5.0、2026-10-11）は composer `x` + `-x account-delete`、TC-232–233。Watch / Operate 分離（0.6.0、2026-10-11）、TC-234–235。

## 進捗管理

| 関心事 | SSOT |
|--------|------|
| ゴール階層 + 優先度 + 出荷履歴 | このファイル |
| 計画上の注意点 + 進路変更記録 | `docs/roadmap-log-ja.md` |
| 人間向け doc index | ルート `README.md` § Documentation |

## 作業の追加方法

1. 領域（A–D）を選んでこのファイルに行を追加（優先度も記録）。未完了なら[一目で見る](#一目で見る)にも追加
2. 新しい要件 → `docs/requirements-ja.md`（FR/NFR）
3. 出荷した挙動 / network / 設定ルール → `docs/architecture-ja.md`（+ `docs/test-ja.md` に TC）
4. キー・レイアウト・色・モーダル → ルートの [`DESIGN.md`](DESIGN.md)
5. 進路変更・計画上の注意点 → [`docs/roadmap-log-ja.md`](docs/roadmap-log-ja.md)
6. 新しい coverage 行は `lazyxrp -x coverage` から（出力を marker の間に貼る。表は手で編集しない）
7. 二言語ルール: `docs/*.md` と `ROADMAP.md` はすべて日本語版 `*-ja.md` を持つ。英語版が agent 向けの正本で、片方の変更は同じ変更内でもう片方にも行う。ID の集合（TC/FR/NFR/S/R/I/P/F）は一致させる
