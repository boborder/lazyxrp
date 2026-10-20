# Roadmap — lazyxrp

**Role:** 進捗管理（最終ゴール・優先順位・マイルストーンの SSOT）。注意点・進路変更記録 → [`docs/roadmap-log.md`](docs/roadmap-log.md)。

**Planning view**. Requirements → [`requirements.md`](docs/requirements.md) · System behavior → [`architecture.md`](docs/architecture.md) · UI/UX → [`DESIGN.md`](DESIGN.md).

**Current version:** `Cargo.toml` → 0.7.0（0.6.1 は 2026-10-11 公開: GitHub Release + crates.io）
**Last updated:** 2026-10-20

**Priority:** P0 = リリースブロッカー · P1 = 次マイルストーンの主目標 · P2 = 計画済み · P3 = 余力・需要次第。各 horizon 内は上から着手。

## Final goal (最終ゴール)

XRPL エコシステム（開発者・ユーザー双方）で人気の CLI/TUI ツールになること。**セキュリティの維持は非交渉条件** — guards を弱める機能は出さない（[`docs/security.md`](docs/security.md)）。

## Long-term (長期)

方向性: XRPL / Xahau / Flare の multichain 深掘りと普及（install path・onboarding）。着手は中期消化後に査定。

| Item | Priority | 前提 / Notes |
|------|----------|--------------|
| ~~TX surface completion — F-01 AccountDelete~~ **済**（0.5.0、2026-10-11） | — | composer `x`（`DELETE` 入力の確認必須）+ `-x account-delete`。Fee（owner reserve）は simulate 由来。TC-232–233、testnet で TUI（X-address tag 付き）/ CLI を validated |
| ~~Watch / Operate UI 分離~~ **済**（0.6.0、2026-10-11） | — | ユーザー選択: `--watch` フラグ。全鍵を起動時に破棄しアドレスだけ表示、TUI は Account パネル（composer・鍵生成なし）、`-x` 署名系は拒否。TC-234–235 |
| Flare Smart Accounts — proof-based FXRP transfer / redeem（`[Unreleased]`、2026-10-16） | — | 読み取り（personal account / FXRP 残高 + `decimals()` / nonce / operator / fee、FXRP パネル）+ composer `a`（`0x01` transfer・`0x02` redeem を operator へ Payment + 32-byte memo。transfer の額は読み取った `decimals()` で最小単位に整数変換）。署名は `[flare.smart_accounts] expected_operator` と一致時のみ。TC-239–247。読み取りは Coston2 / Flare mainnet で実機確認（FXRP `decimals()` = 6）、書き込みはオフライン（local JSON-RPC）のみ — 実機 E2E 未実施 |
| Smart Accounts follow-up: カスタム instruction `0xFE` / `0xFF`（`PackedUserOperation` 構築） | Later | 未対応。vault instruction・`executeDirectMintingWithData`・FDC proof 取得・memo 回復系（`sa-memo`）は F3 で実装済み。`docs/architecture.md` §6.9 |
| Action エラー型移行（`String` → 型付き） | Later | `docs/security.md` 構造課題。UI 文言の SSOT 化と同時に |
| Global row cache (A1) — render perf at scale | Later | 計測で問題が出たら |
| パネル内キー（`j`/`k`・`t`・composer）の設定化 | Later | グローバルキーは設定可（DESIGN §6）。需要が出たら中期へ |

## Flare adoption plan (Flare 最新機能の取り込み計画、2026-10-20 調査)

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
| F3-b | ~~EVM 側 redeem~~ **済**（`[Unreleased]`、2026-10-20） | — | `-x fxrp-redeem <XRP> <r-address> [--tag N]`: `redeemAmount` / `redeemWithTag`（tag は 32 bit）。`minimumRedeemAmountUBA`（mainnet 5 XRP 実測）・FXRP 残高を事前検査し `estimate_gas` 後に送信。**Coston2 で実送信成功（2026-10-20）**: approve なしで `redeemAmount` が通り（FXRP の burn Transfer + `RedemptionRequested`、`RedemptionAmountIncomplete` なし）、approve 不要が実機で確定。`redeemWithTag` と partial redemption の警告経路は未実施。TC-341 |
| F3-c | Smart Account 拡張 **一部済**（`[Unreleased]`、2026-10-20） | — | 済: vault instruction `0x11`–`0x13` / `0x21`–`0x23` を `-x sa-vault`（既存 instruction パイプライン。vault id は `getVaults()` で登録 + type 照合、fee は `getInstructionFee(id)`。**value の単位**は upstream `Instructions.sol` / `PaymentReferenceParser.sol` で確認済み: deposit = FXRP 最小単位、redeem = shares、claim = period / `YYYYMMDD`、value は byte 2–11・vault id は byte 14–15。TC-342）、`executeDirectMintingWithData`（`fxrp-execute --data`）、memo 回復系 `0xE0` / `0xE1` / `0xE2` / `0xD0` / `0xD1` を `-x sa-memo`（upstream `MemoInstructions.sol` の layout、Core Vault 宛の Direct Mint Payment。`0xE1` は nonce 窓を事前検査。TC-344）。**未対応**: `0xFE` / `0xFF`（`PackedUserOperation` 構築。`abi.encode` した userOp の組み立てと nonce 管理が要る）。実送信は未実施 |
| F3-d | ~~FDC proof 自動取得~~ **済**（`[Unreleased]`、2026-10-20、ユーザー承認） | — | `-x fxrp-execute <XRPL tx hash>`: verifier `prepareRequest` → `FdcHub.requestAttestation`（mainnet 実測 **20 FLR**、Coston2 は 1000 wei）→ `Relay.isFinalized(200, round)` 待ち（RPC の一時切断は 5 回まで許容）→ DA layer proof → `executeDirectMinting`。信頼境界: verifier 応答の `abiEncodedRequest` を tx hash / proofOwner / 型 / source と byte 照合、https 限定・redirect 無効・応答 1 MiB 上限・fee 上限 100 FLR、proof は contract が Merkle 検証。**Coston2 で実機確認（2026-10-20）**: `sa-memo` の Payment（XRPL testnet）→ attestation 要求 → round 確定待ち → DA layer から proof 取得まで通過。最後の `executeDirectMinting` は公開 executor が先に finalize 済みで `PaymentAlreadyConfirmed`（`0x18dce79f`）で revert（エラーに説明を追加）。自前の executeDirectMinting 送信成功は未確認。TC-340 |

**対象外（理由）:** FCC（TEE extension を作る開発者向け基盤で、クライアント機能ではない）· FLR の staking / delegation / FlareDrops / rFLR（lazyxrp の XRP ↔ FXRP 軸から外れる）· FXRP OFT ブリッジ送信（LayerZero 手数料・DVN・per-endpoint rate limit の write 面が大きい。読み取りだけなら `FAssetOFTAdapter` の token fee / limit を F2 に追加可）· gasless FXRP（EIP-712 + relayer、x402）· Web2Json（mainnet / Songbird に source なし）· FBTC / FDOGE（FXRP 以外は mint 経路が別）。

**未解決:** `fsa.flare.network` は 2026-10-20 時点で「The FAsset System is currently paused for XRP」と表示するが、同日 mainnet の AssetManagerFXRP は `mintingPaused()` / `emergencyPaused()` / `emergencyPauseLevel()` がすべて false / 0（`web3_sha3` で selector を出して `eth_call`）。banner の根拠は未特定（FSA 側の別条件か表示の遅れ。`terminated()` は revert で存在しない）。計画は停止状態に依存しないので観測のみ記録し、F1-c 実装時に再確認する。

## XRPL coverage (TX 種別カバレッジ)

**SSOT は `src/coverage.rs`**（`lazyxrp -x coverage` が同じ表を出力）。この表は手で更新せず、出力を貼り直す — `roadmap_table_matches_registry`（TC-248）が行ごとの不一致（Type / Read / TUI / CLI / Status。Note 列は自由記述）で失敗する。Status: **Existing** = 型ごとの手書き write 経路 · **Spec** = `src/xrpl/txspec/` の宣言的 spec（TUI composer「Transaction」+ `-x tx` が共通経路。`-x nft-mint` / `-x nft-burn` は別名） · **Planned** = xrpl-rust 1.3.0 に型付きモデルあり、未配線 · **Unsupported** = 型付きモデルなし（署名不可）。Read = TX detail パーサ登録済み。擬似トランザクション（`EnableAmendment` / `SetFee` / `UNLModify`）は対象外。

<!-- xrpl-coverage:start -->
| Type | Read | TUI write | CLI write | Status | Note |
|------|------|-----------|-----------|--------|------|
| Payment | ✓ | ✓ | ✓ | Existing |  |
| AccountSet | ✓ | ✓ | – | Existing |  |
| TrustSet | ✓ | ✓ | – | Existing |  |
| OfferCreate | ✓ | ✓ | ✓ | Existing |  |
| OfferCancel | ✓ | ✓ | ✓ | Existing |  |
| SetRegularKey | ✓ | ✓ | – | Existing |  |
| EscrowCreate | ✓ | ✓ | – | Existing |  |
| EscrowFinish | ✓ | ✓ | – | Existing |  |
| EscrowCancel | ✓ | ✓ | – | Existing |  |
| AMMDeposit | ✓ | ✓ | ✓ | Existing |  |
| AMMWithdraw | ✓ | ✓ | ✓ | Existing |  |
| AccountDelete | – | ✓ | ✓ | Existing |  |
| NFTokenMint | ✓ | ✓ | ✓ | Spec | NFT |
| NFTokenBurn | ✓ | ✓ | ✓ | Spec | NFT |
| NFTokenCreateOffer | ✓ | ✓ | ✓ | Spec | NFT |
| NFTokenAcceptOffer | ✓ | ✓ | ✓ | Spec | NFT |
| NFTokenCancelOffer | ✓ | ✓ | ✓ | Spec | NFT |
| XChainCreateBridge | ✓ | ✓ | ✓ | Spec | XChain |
| XChainModifyBridge | ✓ | ✓ | ✓ | Spec | XChain |
| XChainCreateClaimID | ✓ | ✓ | ✓ | Spec | XChain |
| XChainCommit | ✓ | ✓ | ✓ | Spec | XChain |
| XChainClaim | ✓ | ✓ | ✓ | Spec | XChain |
| XChainAccountCreateCommit | ✓ | ✓ | ✓ | Spec | XChain |
| CheckCreate | ✓ | ✓ | ✓ | Spec | Account objects |
| CheckCash | ✓ | ✓ | ✓ | Spec | Account objects |
| CheckCancel | ✓ | ✓ | ✓ | Spec | Account objects |
| TicketCreate | ✓ | ✓ | ✓ | Spec | Account objects |
| DepositPreauth | ✓ | ✓ | ✓ | Spec | Account objects |
| SignerListSet | ✓ | ✓ | ✓ | Spec | Account objects |
| Clawback | ✓ | ✓ | ✓ | Spec | Account objects |
| PaymentChannelCreate | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| PaymentChannelFund | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| PaymentChannelClaim | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| AMMCreate | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| AMMVote | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| AMMBid | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| AMMDelete | ✓ | ✓ | ✓ | Spec | Channel / AMM |
| MPTokenIssuanceCreate | ✓ | ✓ | ✓ | Spec | MPT |
| MPTokenIssuanceDestroy | ✓ | ✓ | ✓ | Spec | MPT |
| MPTokenIssuanceSet | ✓ | ✓ | ✓ | Spec | MPT |
| MPTokenAuthorize | ✓ | ✓ | ✓ | Spec | MPT |
| CredentialCreate | ✓ | ✓ | ✓ | Spec | Identity |
| CredentialAccept | ✓ | ✓ | ✓ | Spec | Identity |
| CredentialDelete | ✓ | ✓ | ✓ | Spec | Identity |
| DIDSet | ✓ | ✓ | ✓ | Spec | Identity |
| DIDDelete | ✓ | ✓ | ✓ | Spec | Identity |
| OracleSet | ✓ | ✓ | ✓ | Spec | Identity |
| OracleDelete | ✓ | ✓ | ✓ | Spec | Identity |
| PermissionedDomainSet | ✓ | ✓ | ✓ | Spec | Identity |
| PermissionedDomainDelete | ✓ | ✓ | ✓ | Spec | Identity |
| VaultCreate | – | – | – | Planned | Single-asset vault: Asset may be an MPT, which the flat Amount grammar cannot express |
| VaultSet | – | – | – | Planned | Single-asset vault: deferred as a family with VaultCreate (MPT-capable assets) |
| VaultDelete | – | – | – | Planned | Single-asset vault: deferred as a family with VaultCreate (MPT-capable assets) |
| VaultDeposit | – | – | – | Planned | Single-asset vault: Amount may be an MPT, which the flat Amount grammar cannot express |
| VaultWithdraw | – | – | – | Planned | Single-asset vault: Amount may be an MPT, which the flat Amount grammar cannot express |
| VaultClawback | – | – | – | Planned | Single-asset vault: Amount may be an MPT, which the flat Amount grammar cannot express |
| NFTokenModify | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 (dynamic NFT) |
| AMMClawback | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 |
| Batch | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 |
| DelegateSet | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 |
| LedgerStateFix | – | – | – | Unsupported | no typed model in xrpl-rust 1.3.0 |
| XChainAddClaimAttestation | – | – | – | Unsupported | witness-server signed attestation; cannot be authored in a form |
| XChainAddAccountCreateAttestation | – | – | – | Unsupported | witness-server signed attestation; cannot be authored in a form |
| ConfidentialMPTConvert | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
| ConfidentialMPTConvertBack | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
| ConfidentialMPTSend | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
| ConfidentialMPTMergeInbox | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
| ConfidentialMPTClawback | – | – | – | Unsupported | needs zero-knowledge proofs; no typed model in xrpl-rust |
<!-- xrpl-coverage:end -->

書き込み可能 **50 / 68**（TUI または CLI で write 経路あり。残りの Planned（Vault 系）と Unsupported は表の Note 参照。新 family の追加 — 1 family = `txspec/` に 1 ファイル + `FAMILIES` に 1 行、手順は `docs/architecture.md` §2.1）。

## Mid-term (中期) — **0.4.0: write 基盤整理 + TX 拡張**

依存順: `poll.rs` 分割 → EscrowCreate / AMM（submit 種別追加は分割後）。

| # | Item | Source | Priority | 完了条件 |
|---|------|--------|----------|----------|
| 1 | ~~`poll.rs` 分割~~ **済**（2026-10-11） | `docs/security.md` 構造課題 | P1 | 書き込み経路（`submit_*`・`simulate_then_finalize`・quote・関連テスト）を `xrpl/submit.rs`（1,526 行）へ移動、`poll.rs` は 2,633 → 1,120 行。挙動不変（260 テスト緑、新規 TC なし） |
| 2 | ~~Sequence / LastLedgerSequence 整合検査~~ **済**（0.3.1、監査 run-2） | `docs/security.md` Fee cap 節の残余 | P2 | TC-204 |
| 3 | ~~B-02 EscrowCreate composer UI~~ **済**（2026-10-11、F-03 Finish/Cancel を前倒し） | `docs/problems.md` P-003 | P2 | 実際は signing も未実装（poll が黙って捨てていた）。signing + `submit_escrow_*` + composer `9` Create（相対時間 `90s`〜`7d`、CancelAfter 任意）・`0` 一覧から Finish/Cancel、TUI のみ。TC-215–221、testnet で Create ×2 / Cancel / Finish を validated |
| 4 | ~~F-04 AMM Deposit/Withdraw TX~~ **済**（2026-10-11） | DEX trading の後続 | P2 | two-asset deposit・withdraw 全額 / LP 量。composer `d` / `w` + `-x amm-deposit` / `-x amm-withdraw`。TC-222–226、testnet で CLI ×2 / TUI ×2 を validated（プールは使い捨てスクリプトで作成、AMMCreate は対象外） |
| 5 | ~~Fake WebSocket integration fixtures~~ **済**（2026-10-11） | `docs/test.md` | P2 | TC-227。fixture が実バグを検出: `xrpl_receive` が stream メッセージの `type` を落とし、ledger close / account tx の WS イベントが一度も届いていなかった（interval poll のみで動作）。生フレームを読むよう修正、testnet で ledger close 受信を確認。panel 側の反映は既存 TC（account.rs）が担保 |
| 6 | ~~二重送信ガード（同一内容の連続 submit）~~ **済**（2026-10-11） | `docs/security.md` 未修正リスト | P3 | TC-228。WalletPanel の出口 1 か所で、同一 params の write を結果（Ok/Err）到着まで拒否。CLI は validated 待ちのため対象外 |
| 7 | ~~`account_offers` marker paging（> 200 件）~~ **済**（2026-10-11） | `client.rs` ponytail 注記 | P3 | TC-229。200 件/page で marker を追う（上限 50 page）。TUI OfferCancel と `-x offers` の両方 |
| 8 | ~~C3 execute smoke (`execute=true`)~~ **済**（2026-10-11、testnet） | — | P3 | ユーザー判断で実資金は testnet（XRPL testnet + Coston2）。検証で C2 の simulate 拒否（memo null、TC-230）と C3 の ABI 不一致（存在しない selector、TC-231）を発見・修正し、C2 → 公開 executor による mint、48-byte memo → lazyxrp C3 で FXRP 9.8 → 14.7 を確認。mainnet は未実施 |

## Next (次期)

中期 #1–#8 はすべて済み（0.4.0–0.4.2）。長期は F-01 AccountDelete（0.5.0）と Watch / Operate 分離（0.6.0）が済み。監査 run-2 の残り 5 件（https→http redirect・IPv6 埋め込み IPv4 / `fec0::/10` SSRF・bidi / 書式文字・tx 時刻 overflow・`install.sh` https 限定）は `[Unreleased]` で修正（TC-236–238）。残り（Action エラー型・row cache・パネル内キー設定化）は完了条件が未定義のため着手前に査定。

## Short-term (短期) — 0.3.0 公開後の残務

| # | Task | Priority | 完了条件 |
|---|------|----------|----------|
| 1 | ~~Testnet E2E — TUI~~ **済**（2026-10-11、tmux で操作） | — | OfferCreate（即約定）・非約定 OfferCreate → OfferCancel・`p` 見積り → cross-currency Payment（1 USD 着金）を validated。hash は `docs/test.md` Testnet E2E |
| 2 | ~~リリース運用の最適化~~ **済**（2026-10-11） | — | Release 本文を自動生成（前タグからのコミット + GitHub 生成ノート）、`mise run release <ver>`（bump + lock 同期 + gates + 1 コミット）、週次 CI（advisory / yanked）。手順は `docs/maintenance.md` |
| 3 | ~~v0.3.0 Release 本文の後埋め~~ **済**（2026-10-11） | — | 破壊的変更・移行リンク・コミット一覧を記入 |

## Shipped

完了マイルストーン（履歴）。次の着手先は上の horizon 参照。

| Milestone | Version | Status | Notes |
|-----------|---------|--------|-------|
| Quality pass | 0.1.23 | **Done** | |
| Agent/docs hygiene | — | **Done** | |
| Wallet TX + FXRP | 0.2.0+ | **Done** (2026-09-12) | C3 mainnet smoke → Mid-term (P2) |
| Docs / agent hygiene | — | **Done** | Human doc index in README; `docs/` self-contained |
| Ratatui optimization | — | **Done** | dirty render + shared table shipped |
| Docs hierarchy redesign | — | **Done** (2026-09-12) | keys → `DESIGN.md`; architecture = behavior; removed `docs/README` + `docs/tasks` |
| Network multichain | 0.2.5+ | **Done** (2026-09-12) | Xahau + `<Ctrl-n>` session switch + Flare display + wallet read panel |
| Test coverage / local integration pass | 0.2.9 | **Done** (2026-09-16) | Local RPC read-only CLI coverage, `lazyxrp`/`rp` binary smoke tests, Assets and panel state coverage |
| Review batch #3: production guard + test coverage | 0.2.10 | **Done** (2026-09-17) | Xahau production write guard, local CLI RPC migration, nft_image SSRF hardening, binary smoke tests, alloy 2 / MSRV 1.94.1 |
| Flare read-path stabilization | 0.2.11 | **Done** (2026-09-17) | Batched FTSOv2 `getFeedsById` (5→1 RPC/poll), fee-aware retry, `FXRP/USD`→`XRP/USD` premap, AssetManager/wallet reads via `try_join!`, flare-only failure cooldown (2→60s) decoupled from XRPL backoff |
| XRPL poll efficiency pass | 0.2.11 | **Done** (2026-09-17) | dUNL excluded from XRPL backoff both ways + stale-on-error with 60 s retry cooldown; Account-tab panels (`account_tx`, wallet overview) gated like Market/Assets |
| Status bar health tracks | 0.2.11 | **Done** (2026-09-17) | XRPL chip goes STALE during quiet backoff + OFFLINE on `server_info` errors; flare dot (`flr:`) with red-on-failure/orange-stale; `dunl:` data-age marker with `!` past cache TTL |
| DEX trading + CLI redesign + keymap docs | 0.3.0 | **Done** (2026-10-10) | Wallet composer OfferCancel (`8`) + Payment Send max / `p` path quote (`slippage_bps`); CLI: bare subcommands + `watch` removed, `-x send` simulate-first + non-zero exit on failure, `-x offers` / `-x offer-cancel`; shared Payment validation (P-004); live `simulate` LLS fill (TC-200); `--seed`/`--mnemonic` kept with stderr warning (FR-13); global keybindings user-mergeable (DESIGN §6); `rc` alias stays README-only |
| Live 検証 + testnet DX + 監査 run-2 | 0.3.1 | **Done** (2026-10-11) | `-x faucet` / `-x offer-create`、CLI write の validated 待ち、endpoint `network_id` 照合（S-017）、opt-in testnet E2E、監査 run-2 修正（TC-204–214、Core Vault ピンは破壊的）、OSS パッケージ整備、dependabot + Actions SHA 固定 |
| write 基盤整理 + TX 拡張 | 0.4.0 | **Done** (2026-10-11) | `poll.rs` → `submit.rs` 分割、EscrowCreate / Finish / Cancel（TUI）、AMM Deposit / Withdraw（TUI + `-x`）、`amm_info` 実応答形の parse 修正（TC-015）。testnet で Escrow ×4・AMM ×4 validated |
| 中期 #5–#7 | 0.4.1 | **Done** (2026-10-11) | WS イベント未認識の修正（TC-227）、二重送信ガード（TC-228）、`account_offers` paging（TC-229） |
| C3 testnet smoke | 0.4.2 | **Done** (2026-10-11) | FXRP C2 simulate の null 拒否修正（TC-230）、C3 ABI 修正（TC-231）。XRPL testnet + Coston2 で mint / execute を確認 |
| AccountDelete | 0.5.0 | **Done** (2026-10-11) | 長期 F-01。composer `x` + `-x account-delete`、TUI は `DELETE` 入力の確認必須（TC-232–233）。testnet で TUI / CLI validated |
| Watch / Operate 分離 | 0.6.0 | **Done** (2026-10-11) | `--watch` 閲覧専用起動（TC-234–235） |
| AMM no-pool 表示 + ponytail 監査 ×2 | 0.6.1 | **Done** (2026-10-11) | プールなしペアでステータスバーにエラーを出さずパネル表示に。依存 7 個削除（`config`・`signal-hook`・`tracing-error`・`strum`・`url`・`time`・build-dep `anyhow`）、挙動不変 |

## Task tracking

| Concern | SSOT |
|---------|------|
| Goal hierarchy + priorities + shipped history | This file |
| Planning cautions + pivot log | `docs/roadmap-log.md` |
| Human doc index | Root `README.md` § Documentation |

## How to add work

1. Horizon（短期 / 中期 / 長期）を選んでこのファイルに row 追加（優先度も記録）
2. New requirement → `docs/requirements.md` (FR/NFR)
3. Shipped behavior / network / config rules → `docs/architecture.md` (+ TC in `docs/test.md`)
4. Keys, layout, colors, modals → root [`DESIGN.md`](DESIGN.md)
5. 進路変更・計画上の注意点 → [`docs/roadmap-log.md`](docs/roadmap-log.md)
