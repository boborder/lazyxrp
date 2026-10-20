graphify: Knowledge graph exists. Read graphify-out/GRAPH_REPORT.md for god nodes and community structure before searching raw files. Graph may be stale because files changed; after inspecting it, run `graphify update .` before relying on modified areas.

# architecture.md — System & behavioral design

**Role:** 詳細設計・スキーマ設計（ネットワーク・設定・データフロー・パネル契約の SSOT）。UI キー／レイアウトは [`DESIGN.md`](../DESIGN.md)。

From [`requirements.md`](requirements.md). Tests in [`test.md`](test.md) trace here and to FR/NFR ids.

**Not here:** keybindings, colors, layout splits, scroll/modal interaction — SSOT is root [`DESIGN.md`](../DESIGN.md).

| Topic | SSOT |
|-------|------|
| Look, keys, focus, theme | [`DESIGN.md`](../DESIGN.md) |
| FR/NFR ids | [`requirements.md`](requirements.md) |
| Modules, channels, startup/submit flows | `src/app.rs`, `src/xrpl/poll.rs`, `src/xrpl/submit.rs`, [`directory.md`](directory.md) §3 |
| Rules I-1〜I-11 | [`security.md`](security.md) (guards in `src/`) |
| Directory layout | [`directory.md`](directory.md) |
| TX detail parser registry | §5.2.1 below, `src/components/shared/tx_detail/` |
| Security audit S-xxx / risks R-xxx | [`security.md`](security.md) |
| Doc index | [`AGENTS.md`](../AGENTS.md) |
| Graphify | [`graphify-out/GRAPH_REPORT.md`](../graphify-out/GRAPH_REPORT.md) |

## System boundary

lazyxrp is a **single Rust binary** run locally. It connects to public **XRP Ledger** endpoints (JSON-RPC / WebSocket) for reads and signed submission when configured. Optional **Flare** RPC reads (FTSO, FXRP AssetManager) and optional C3 execute when explicitly enabled.

## Runtime structure (single process)

| Area | Role | Primary code |
|------|------|--------------|
| Watch (TUI) | Dashboard, panels, keyboard flows | `app.rs`, `tui.rs`, `components/` |
| CLI runner | Non-interactive subcommands | `cli.rs`, `xrpl/cli_exec.rs` |
| XRPL integration | RPC, WebSocket, poll task → `Action`; write paths | `xrpl/client.rs`, `ws.rs`, `poll.rs`, `submit.rs` |
| Config & signing | Merged `config.toml`, credentials | `config.rs`, `signing.rs` |
| Flare | FTSO / FXRP read (+ optional execute) | `flare.rs`, `poll.rs`, `submit.rs` |

Module wiring summary: [`directory.md`](directory.md) §3. Tab splits and keys: [`DESIGN.md`](../DESIGN.md).

## 1. 起動モードとスタック（要約）

| 入口 | 用途 | 例 |
|------|------|-----|
| `lazyxrp` | **TUI**（既定） | `lazyxrp --account <ADDR>` |
| `lazyxrp -x <subcmd>` | **スクリプト CLI** | `lazyxrp -x info` |
| `rc <subcmd>` | 同上（シェル alias） | `alias rc='lazyxrp -x'` |
| `rp` | lookup 専用バイナリ | `rp -t <txid\|ADDR>` |
| `lazyxrp --self-uninstall` | アンインストール | `logging::init` 前に終了 |

- エントリ: `src/lib.rs` — TUI → `App::new` → `run`、`-x` CLI → `xrpl::execute_cli_command(...)`.
- 裸サブコマンド（`-x` なし）はエラーで `lazyxrp -x <name>` を案内。`watch` サブコマンドは削除（TUI は引数なし `lazyxrp`）。CLI の書き込み（`send`・`offer-create`・`offer-cancel`・`amm-deposit`・`amm-withdraw`・`account-delete`）も **network 照合 → simulate → sign → submit → validated 待ち**（Sequence/Fee は simulate 由来。LastLedgerSequence は simulate が返せばその値、無ければ `ledger_index + 20` を `parse_simulate_result` が補完）。validated 待ち（`wait_validated`）: 1 秒ごとに `tx` を確認し、最終結果が `tes*` 以外なら Err、validated ledger が LLS を過ぎても未取込なら Err（expired）、5 分（`WAIT_LIMIT`）経っても決着しなければ Err（ノード停止・`validated_ledger` 欠落への上限）。`--no-wait` で submit 直後に戻る。`offers` は `account_offers` 読み取り。`faucet [ADDR]` は testnet/devnet の公開 faucet へ `destination` を POST（ADDR 省略時は `generate_wallet_keys_local` でローカル生成し seed は表示のみ）。応答の `classicAddress` が依頼先と違えば Err。
- Spec'd 書き込み（`-x tx <TYPE> Field=value…`・`-x nft-mint`・`-x nft-burn`、§2.1）も同じ network 照合 → simulate → sign → submit → validated 待ち。`--watch` は拒否（`Cmd::signs`）。`-x coverage` は XRPL TX 種別カバレッジ表（ROADMAP.md「XRPL coverage」と同一、オフライン）を出力。
- スタック: `ratatui` + `crossterm`、非同期 `tokio`、XRPL は `src/xrpl/`。

## 2. ウォレット（行動契約）

`WalletPanel` + `wallet_composer.rs` on Account tab. **Interaction keys:** [`DESIGN.md`](../DESIGN.md) (Keys). **Composer preview colors:** (Components).

Behavioral contract:

- Credential resolution: `Config::new()` merges `XRPL_SEED` / `XRPL_MNEMONIC`, then `credential_from_secrets` → `PollContext.signing_credential` (`Option<SigningCredential>`).
- All wallet TX: **network 照合 → simulate → sign → submit** (I-3). Mainnet / Xahau mainnet writes require CLI **`--yes`** (I-2).
- Double-submit guard (TC-228): `WalletPanel::on_key_event` is the single exit for queued writes; a write whose params (`Debug`) equal the one still awaiting its `*SubmitOk` / `*SubmitErr` is refused with a flash.
- Endpoint network 照合（TC-203）: 署名前に `RpcClient::ensure_network` が `server_info.network_id` を `Network::network_id()`（mainnet 0 = 省略、testnet 1、devnet 2、xahau 21337、xahau-test 21338）と比較し、不一致なら拒否。TUI は全 XRPL submit が `fetch_account_summary_for_submit`、CLI は各 write arm で実行。`XRPL_RPC_SERVER` / `rpc_server` / `--server` は preset より優先されるため（§6.2）、別ネットワークの endpoint で `--yes` ガードが外れるのを防ぐ。
- Composer phases shipped: Payment, AccountSet, SetRegularKey, OfferCreate, OfferCancel, TrustSet, FXRP Mint (C2), FXRP Execute (C3), EscrowCreate (`9`), Escrows finish/cancel (`0`), AMMDeposit (`d`), AMMWithdraw (`w`), AccountDelete (`x`), Smart Account (`a`). TUI only for escrow (no `-x` escrow commands).
- Transaction composer (TC-255): picker row `[m] Transaction` lists the spec'd types (`xrpl::txspec`, §2.1); the form is generated from `TxSpec::fields` (one row per field, spec defaults prefilled, the focused row shows help text and its inline validation error, the preview line is green `▸ <summary>` when `TxSpec::validate` passes and orange with the error otherwise). Row nav / edit keys are the shared composer ones; `s` validates exactly like the poll task and queues one `Action::GenericTxSubmit`. Types with `TxSpec::confirm` (NFTokenBurn `BURN`, NFTokenAcceptOffer `ACCEPT`) never queue directly — `s` opens the same type-to-confirm `DangerConfirm` as AccountDelete (Esc restores the form). Assets tab `x` (TC-256): `NftTab` answers `Action::NftBurnStart` with `Action::TxComposerOpen{NFTokenBurn, NFTokenID}`; the app jumps to the Account tab and the wallet opens the form prefilled (only with a signing credential). The components never call `xrpl/` clients — they read the static spec registry and emit Actions.
- AMM (TC-222–226): AMMDeposit is two-asset only (`tfTwoAsset`; `Amount` / `Amount2` = maxima, also naming the pool; `signing::validate_amm_deposit_specs`: specs `XRP:drops` / `CUR:issuer:value`, two different assets). AMMWithdraw: empty LP amount → `tfWithdrawAll`; otherwise `tfLPToken` + `LPTokenIn` whose currency / issuer (AMM account) come from `amm_info` (`RpcClient::amm_lp_token`) before simulate. Same simulate → sign → submit; composer `d` / `w` and `-x amm-deposit` / `-x amm-withdraw`. No AMMCreate / single-asset / Vote / Bid.
- AccountDelete (TC-232–233): Destination = classic or X-address (tag from the X-address, network bit checked like Payment). The composer never queues directly: a valid destination opens type-to-confirm `DELETE` (`DangerResume::AccountDelete` carries the destination; Esc restores it). Fee is not computed locally: simulate fills the owner-reserve fee (≤ `MAX_NODE_FEE_DROPS`) and that value is signed. Preconditions (`tecTOO_SOON` < 256 ledgers, `tecHAS_OBLIGATIONS` owned objects, `tecDST_TAG_NEEDED`, `temDST_IS_SRC`) are left to simulate. `-x account-delete <DEST>` is the same path with the CLI prompt / `--yes`.
- EscrowCreate: XRP only, no crypto-condition. `signing::validate_escrow_create_input` (composer + poll): classic Destination, amount > 0, FinishAfter / optional CancelAfter as durations relative to submit (`90s` / `30m` / `2h` / `7d`), CancelAfter strictly later. The poll task fixes absolute Ripple-epoch times once (local clock) so simulate and the signed tx carry the same values; a FinishAfter already passed by ledger time fails in simulate.
- Escrows: opening sends `RefreshAccountEscrows` → `RpcClient::account_escrows` (`account_objects` type `escrow`, first 200, owned **and** incoming). `OfferSequence` = object `Sequence` when present, else the creating tx (`PreviousTxnID`; only EscrowCreate modifies an Escrow) `Sequence`, or `TicketSequence` when 0. `s` → EscrowFinish, `c` → EscrowCancel (`EscrowCloseSubmit`, same simulate → sign → submit). Refused locally: unresolved sequence, Finish of a condition escrow. Timing rules (FinishAfter / CancelAfter passed) are left to simulate (`tecNO_PERMISSION`).
- OfferCancel: opening the composer sends `RefreshAccountOffers` (poll task → `account_offers`, follows `marker` 200 per page up to 50 pages, signing account only → `XrplAccountOffers`); selected `seq` → `OfferCancelSubmit` (same simulate → sign → submit path). Not polled.
- Payment validation: `signing::validate_payment_input` (shared by composer preview / submit / quote and poll `PaymentSubmit`) — destination classic checksum or X-address, positive amount (XRP drops / IOU value), IOU currency + issuer checksum, Send max spec. Poll adds X-address network, self-pay, balance.
- Cross-currency Payment: optional `send_max` spec (same parser as OfferCreate) → `SendMax`, rippled default paths (no `Paths`, no partial payment). XRP→XRP SendMax rejected. With SendMax: self-payment allowed (swap), balance check uses SendMax XRP drops (0 for IOU SendMax). Dry paths fail in simulate (`tecPATH_DRY`/`tecPATH_PARTIAL`).
- Send max quote (`p`): `PaymentQuote` → poll task `ripple_path_find` from the signing account with `source_currencies` = Send max field's currency prefix (empty + IOU Amount → XRP) → cheapest alternative + `[xrpl] slippage_bps` (default 100 = 1%, rounded up) → `PaymentQuoteOk(spec)` fills the field. Read-only; send still goes simulate → sign → submit.
- FXRP C2: 32-byte memo to Core Vault (signed only if it equals `[flare.fassets] expected_core_vault`, `flare::check_core_vault`); C3: pasted FDC `XRPPayment` proof JSON (DA layer `{response, proof}`, `response` = decoded attestation object, or `{data, merkleProof}`) + `[flare.fassets] execute=true` → `executeDirectMinting(IXRPPayment.Proof)` (selector `0x78d0299e`, TC-231); a reverted receipt is an error. Obtaining the proof (verifier `prepareRequest` → `FdcHub.requestAttestation` → DA layer) stays outside lazyxrp. With a 32-byte memo any executor may finalize first (then C3 reverts `PaymentAlreadyConfirmed`; the FXRP is already minted).
- FXRP v1.3 guards (TC-330–333): C2 refuses an amount that does not exceed `max(getDirectMintingMinimumFeeUBA, fee_bips share) + executor fee` (`flare::check_direct_mint_amount`; a payment below the minimum fee is minted entirely to the fee receiver) and refuses to queue while `mintingPaused()` is true or `emergencyPauseLevel() > 0` (both read in `fetch_fxrp_direct_mint_info`; a failed read is `None` = unknown, not a block). C3 classifies the receipt from AssetManager events (`flare::decode_direct_minting_outcome`): `DirectMintingDelayed` / `LargeDirectMintingDelayed` is reported as delayed — re-run with the same proof at `executionAllowedAt`; `DirectMintingPaymentTooSmallForFee` is an error.
- FXRP mint limits (TC-334–336): `fetch_fxrp_direct_mint_info` also reads, in the same parallel batch, `getDirectMinting{Hourly,Daily}LimitUBA` + `...LimiterState` (stale between writes → `flare::window_max_now` advances the 1 h / 1 d tumbling window to the latest block time), the large-mint threshold / delay, `assetMintingGranularityUBA`, FXRP `totalSupply` and `mintingCapAMG`. `AssetManagerSettings.Data` has 58 fields, beyond alloy's tuple ABI support, so `getSettings()` is read as raw bytes and word 22 of the struct head is taken (`settings_minting_cap_amg`, cross-checked live: 170M XRP on mainnet). `flare::classify_mint` → `Immediate` / `DelayedWindow` / `DelayedLarge` / `CapExceeded`; the composer refuses only `CapExceeded` (the cap check reverts, so the XRP would wait in the Core Vault), delays are shown in the FXRP panel (`Max now`, `Cap used`). `limits` is `None` when any view fails (advisory).
- `lazyxrp -x fxrp [--tag N]` (TC-337) is handled in `lib.rs` before the XRPL client (`Cmd::Fxrp`; Flare RPC = `FLARE_RPC_URL` or the `[flare] network` default — `resolve_flare_rpc_url` always yields one, so the "no Flare RPC" error is defensive; same endpoint check as the TUI). `flare::fetch_fxrp_report` returns JSON: the Direct Mint info above, `getMintingTagManager()` → `reservationFee` and, with `--tag`, `ownerOf` / `mintingRecipient` / `allowedExecutor` (a contract revert on `ownerOf` → `reserved: false`; transport errors propagate), and the global `redemptionQueue(next, maxRedeemedTickets)` paged until `next == 0` (max 100 pages; `complete` says whether it ended). Pages are separate unpinned calls. `getSettings()` field words (`lotSizeAMG` 19, `mintingCapAMG` 22, `maxRedeemedTickets` 30) follow upstream `AssetManagerSettings.sol` order.
- Smart Account vaults (TC-339): the vault list is on-chain, `MasterAccountController.getVaults()` (`uint256[] ids, address[] addresses, uint8[] types` — 1 Firelight, 2 Upshift), so no vault address is pinned or hard-coded. `fetch_smart_account_info` reads it in the same batch as the other controller views (a failure is `None` → no rows), then for at most 8 vaults reads the personal account's share `balanceOf` and ERC-4626 `convertToAssets(shares)` (sequential, advisory: a failing vault is skipped; zero-share vaults are omitted). `SmartAccountInfo.vaults` → FXRP panel rows `Vault <id> <kind> <assets> FXRP` (or the raw share count when `convertToAssets` is absent). `-x fxrp` additionally reports `coreVaultAvailableAmount()` and `getAllAgents`, which together with the redemption queue account for the FXRP supply (direct mints create no redemption ticket; transfers to the Core Vault consume agent tickets).
- Flare writes (`-x fxrp-execute` / `fxrp-redeem` / `fxrp-tag-*`, TC-340–342) run in `lib.rs::run_flare_command` before the XRPL client. Gate order: `[flare.fassets] execute = true` → Flare EVM key (`FLARE_EVM_KEY`, captured at startup) → https RPC (`check_flare_endpoint` with signing) → `--yes` on Flare mainnet. Each write reads the on-chain preconditions first, calls `estimate_gas` (the EVM simulate) and prints the plan on stderr; the hash goes to stdout. `fdc::prove_xrp_payment` (verifier → `FdcHub.requestAttestation` with `getRequestFee` → `Relay.isFinalized(200, round)` → DA layer) feeds `flare::execute_direct_minting_with` (`--data` → `executeDirectMintingWithData`). The proof is bound to the signer (`proofOwner`). `-x sa-vault` reuses `submit_smart_account_instruction` (operator pin, fee bound, simulate) through `xrpl::run_smart_account_instruction`; `SmartAccountInstructionParams.vault` selects the vault instruction encoding.
- Smart Account instruction (TC-239–247, §6.9): composer `a` → `Action::SmartAccountInstructionSubmit` → `PollCommand::SmartAccountInstruction` → `submit_smart_account_instruction`: encode the 32-byte payment reference (`signing::encode_fxrp_transfer_instruction` / `encode_fxrp_redeem_instruction`; transfer value = raw FXRP base units, converted in the composer from the read `decimals()`) → fee bound (`check_smart_account_fee`) → classic operator (no X-address tag) → mainnet `--yes` gate → operator ≠ signer → **operator pin** (`flare::check_operator` vs `[flare.smart_accounts] expected_operator`) → `account_info` → simulate (`tesSUCCESS`) → sign → submit. The Payment goes to `getXrplProviderWallets()[0]`, `Amount` = `getInstructionFee(id)` XRP drops (exact), no `DestinationTag`, one Memo (`MemoData` = the 32-byte instruction hex). The operator then runs the FDC-proven `executeInstruction` on Flare (outside lazyxrp); the TUI hint says to check the personal-account balance.
- `RpcClient::simulate_tx` drops JSON `null` members before sending (xrpl-rust serializes unset `Memo` fields as `null`, which rippled rejects; TC-230).

### 2.1 Spec'd transaction types (`src/xrpl/txspec/`) and coverage

One declarative `TxSpec` per XRPL transaction type replaces the per-type Action trio / `PollCommand` / `submit_*` / composer phase / CLI arm of the hand-written types. Everything generic is shared:

| Piece | Where | Role |
|-------|-------|------|
| `TxSpec` (type, family, summary, `fields: &[FieldSpec]`, `confirm`, `check`, `build_json`, `sign`) | `txspec/mod.rs` | Registry entry. `FieldKind` = `Uint(max)` / `Text(max bytes)` / `Address` / `Hex(len)` / `HexList` / `Amount` (`XRP:drops` or `CUR:issuer:value`, `signing::parse_offer_amount`) / `Time` (`7d` or Ripple-epoch seconds) / `Flags(table)`; `validate` = per-field kind check + required + cross-field `check` + unknown-field refusal |
| Family file (`nft.rs`) | `txspec/<family>.rs` | `SPECS` + per TX: `*_tx(account, fields, sequence, fee_lls)` builds the typed `xrpl::models` struct once; `*_json` = unsigned `tx_json` for simulate (also runs the crate's `Model::validate`), `*_sign` = `sign_blob!` with simulate's Sequence / Fee / LastLedgerSequence |
| Registry | `txspec::FAMILIES` | The only place a spec'd type is listed (`registry()`, `lookup()`) |
| Submit | `submit::submit_generic_transaction` | `GenericTxSubmitParams{tx_type, fields, skip_mainnet_prompt}` → `TxSpec::prepare` (validate, trim, `Time` → absolute **once**) → `run_submit` (mainnet `--yes` gate → `ensure_network` → `account_info` → simulate `tesSUCCESS` bound to the request → Fee cap → sign → submit, under the poll `submit_lock`). One Action trio `GenericTxSubmit/Ok/Err`, one `PollCommand::GenericTx` |
| TUI | `wallet.rs` / `wallet_composer.rs` | `ComposerPhase::TxPick` / `Tx` (§2) |
| CLI | `cli_exec.rs::cmd_tx_request` | `Cmd::Tx` / `NftMint` / `NftBurn` → same `prepare` before any RPC → `simulate_confirm_submit` |
| Coverage | `src/coverage.rs` | `rows()` = `Existing` (hand-written, listed in `EXISTING`) + `Spec` (derived from the registry) + `Planned` (typed model in xrpl-rust 1.3.0, not wired) + `Unsupported` (no typed model); `read` = TX detail parser registered. `lazyxrp -x coverage` prints the table pasted into ROADMAP.md "XRPL coverage"; `roadmap_table_matches_registry` (TC-248) fails on drift and checks the README `**N / M**` count |

NFT family (TC-249–251): NFTokenMint (`NFTokenTaxon` u32, `URI` plain text ≤ 256 bytes → upper-case hex, `Flags` burnable / onlyXRP / transferable, `TransferFee` 0..=50000 and only with transferable, optional `Issuer`), NFTokenBurn (`NFTokenID` 64 hex, optional `Owner`; confirm `BURN`), NFTokenCreateOffer (`Amount`, `Flags=sell`, `Owner` required for buy offers and refused for sell offers, `Destination`, `Expiration`), NFTokenAcceptOffer (sell and/or buy offer id, `NFTokenBrokerFee` only with both; confirm `ACCEPT`), NFTokenCancelOffer (1..=500 distinct offer ids). Not supported: `tfMutable` minting (xrpl-rust's `NFTokenMintFlag` has no `0x10`), zero-price offers (the shared amount parser rejects 0). `NFTokenMinter` is an AccountSet field (hand-written AccountSet path), not an NFTokenMint field; minting for another issuer uses `Issuer`.

Account-objects family (`checks_misc.rs`, TC-260–263): CheckCreate / CheckCash / CheckCancel / TicketCreate / DepositPreauth / SignerListSet / Clawback; confirm phrases `CASH` / `SIGNERS` / `CLAWBACK`. Omitted (crate or shared parser cannot express): DepositPreauth credential lists, SignerEntry `WalletLocator`, MPT Clawback (`Holder`).

XChain family (`xchain.rs`, TC-320–328): XChainCreateBridge / ModifyBridge / CreateClaimID / Commit / Claim / AccountCreateCommit. The bridge is one flat field `XChainBridge=lockDoor,lockIssue,issueDoor,issueIssue` (issue = `XRP` or `CUR:issuer`; doors checksummed and distinct; no XRP↔IOU pairing); rewards / minimum are plain drops; `XChainClaimID` is hex (`13f`). Confirm phrases `BRIDGE` / `COMMIT` / `CLAIM`. Not supported: the witness attestation types (signatures cannot be authored in a form) and ConfidentialMPT* (zero-knowledge proofs, no typed model) — both `Unsupported` in `coverage.rs`; zero-valued `Amount` is refused by the shared amount parser.

MPT / Identity families (`mpt.rs` TC-301/303/305/310, `identity.rs` TC-300/302/304/308/309; `identity.rs` also hosts the shared helpers): MPTokenIssuanceCreate / Destroy / Set (lock, unlock only) / MPTokenAuthorize and CredentialCreate / Accept / Delete, DIDSet / DIDDelete, OracleSet / OracleDelete, PermissionedDomainSet / Delete; confirm `DESTROY` / `DELETE` on the irreversible ones. Blob fields are plain text hex-encoded; `PriceDataSeries=XRP/USD=740@2,…` and `AcceptedCredentials=rIssuer:Type,…` are flat strings. CredentialCreate `URI` is capped at 128 bytes (xrpl-rust model). Omitted: MPT mutation flags / `ImmutableFlags` / confidential keys, DIDSet field clearing. Vault* stay `Planned` (MPT-capable Asset / Amount).

Channel / AMM family (`channel_amm.rs`, TC-280–286): PaymentChannelCreate / Fund / Claim (drops-only `Amount`, `PublicKey` 66 hex, `Time` for `CancelAfter` / `Expiration`; Claim takes a pasted `Signature` + `PublicKey`, never generated locally, `Flags` renew / close) and AMMCreate / Vote / Bid / Delete (`Asset` / `Asset2` = `XRP` or `CUR:issuer` via `parse_amm_asset`, `TradingFee` 0..=1000, `BidMin` / `BidMax` LP-token `CUR:issuer:value`, `AuthAccounts` ≤ 4). Confirm phrases `LOCK` / `FUND` / `CLAIM` / `CREATE` / `BID` / `DELETE` (none on AMMVote). Not supported: `CredentialIDs` on Claim; AMMClawback stays `Unsupported`.

**How to add a transaction type** (later families: Check*, TicketCreate, DepositPreauth, SignerListSet, PaymentChannel*, AMM*):

1. Create `src/xrpl/txspec/<family>.rs` (copy `nft.rs`): per type a `pub const X: TxSpec` with `FieldSpec`s (name = ledger field name, `FieldKind`, `required`, `help`, `default`), an optional cross-field `check`, `*_tx` (typed struct via `common_fields!`), `*_json` (`tx.validate()` + `serde_json::to_value`), `*_sign` (`sign_blob!`), and `pub static SPECS: &[TxSpec] = &[…]`.
2. Add `pub(crate) mod <family>;` in `txspec/mod.rs` and **one entry** `<family>::SPECS` to `FAMILIES`. Nothing else: the picker, form, `-x tx`, submit, guards and coverage `Spec` rows follow from the registry. Confirm-phrase types set `confirm: Some(Confirm{phrase, hint})`.
3. Move the type from `PLANNED` (or `UNSUPPORTED`) in `src/coverage.rs` out (a type listed twice fails `registry_is_consistent`); make sure its TX detail parser is registered (`Spec` rows need `read`); then paste `lazyxrp -x coverage` into the ROADMAP.md table and fix the README `**N / M**`.
4. Tests: validation accept / reject table (boundaries), unsigned `tx_json` shape, signed blob decode with distinct Sequence / Fee / LastLedgerSequence (copy `nft.rs` tests); add the type to the TC-088 mainnet-gate table in `submit.rs`; docs/test.md TC row.

Accepted tx types/fields are also what `-x tx` takes: `lazyxrp -x tx NFTokenCreateOffer NFTokenID=… Amount=XRP:1000000 Flags=sell Expiration=7d`.

## 3. ランタイム・データフロー

Implementation: `src/xrpl/poll.rs` (loop, reads), `src/xrpl/submit.rs` (writes), `src/app.rs`.

| Direction | Path |
|-----------|------|
| Input | Keyboard → `tui::Event` → `Action`; XRPL RPC/WS → `Action` |
| Manual refresh | `Action::Refresh*` → `PollCommand` → poll task |
| Submit | `Action::*Submit` → `PollCommand` → simulate/sign/submit in poll |
| Output | `Component::update` → `draw`; `App::run` calls `draw_frame` once per loop iteration when `needs_draw` (no fixed frame timer) |

WebSocket ledger close coalesces poll triggers (`MIN_POLL_INTERVAL`). Scheduled poll uses `poll_interval_ms` from config (default **5000** ms for interactive use; **15000–30000** ms recommended for long-running / always-on sessions to reduce public RPC load).

**Public-network politeness:** XRPL Foundation dUNL (`unl.xrplf.org`) is cached in-process for **10 minutes** (`DUNL_CACHE_TTL`); when a refresh fails, the stale summary keeps being served for a **60 s** retry cooldown (`DUNL_RETRY_BACKOFF`, stale-on-error — a first-ever fetch failure before any success has no cache and errors until it connects). Flare `ContractRegistry` lookups (`FtsoV2`, `AssetManagerFXRP`) are cached per RPC URL for the process lifetime. Poll failures back off exponentially (2s→60s cap) on **two independent tracks keyed per endpoint**: XRPL RPC fetch successes key the XRPL track, while Flare fetch failures only `warn!` and escalate a flare-only cooldown on the same 2s→60s schedule without touching the XRPL track; the dUNL fetch (different host) is likewise excluded from the XRPL track in both directions. Account-tab panels (`account_tx`, wallet overview) poll only while the Account tab is active, mirroring the Market/Assets tab gates. HTTP 429 is retried with delay. NFT image fetch (`nft_image.rs`) applies per-hop DNS SSRF checks, manual redirects, and a streamed response-size cap ([security.md](security.md) R-012).

**Untrusted-fetch & signing guards:** `nft_image::fetch_via` + `client_for_url` is the single fetch path for NFT images **and** `xrp-ledger.toml` (validator-manifest `domain` must be a plain ASCII hostname): private/reserved addresses rejected, connection pinned to the checked IP, proxies disabled, every redirect hop (≤5) re-checked, body capped (4 MiB image / 1 MiB toml). JSON-RPC bodies are capped at 16 MiB while streaming. A node-chosen `Fee` above `signing::MAX_NODE_FEE_DROPS` (2 XRP) is never signed. Remote strings that reach CLI output/errors pass `util::sanitize_remote`. The Flare RPC URL follows the same https / no-signing-over-http rules as the XRPL endpoints (`lib.rs::check_flare_endpoint`).

**NFT image fetch:** requests carry a `lazyxrp/<version>` User-Agent (Wikimedia and other CDNs answer UA-less requests with 403). `ipfs://` URIs try `IPFS_GATEWAYS` in order (`ipfs.io`, then `gateway.pinata.cloud`) because a single public gateway routinely returns 429/5xx. The Assets tab redraws on the `Action::Render` that `NftTab` emits when an off-thread resize finishes (a bare `Tick` does not mark the frame dirty).

## 4. 設定値と起動パラメータ

### CLI

| Flag | Effect |
|------|--------|
| `--yes` | Skip mainnet write confirmation (CLI + TUI submit) |
| `--no-wait` | `-x` writes return after submit (default: wait for validated, non-`tes` = non-zero exit) |
| `--network` | `mainnet` \| `testnet` \| `devnet` \| `xahau` \| `xahau-test` |
| `--server`, `--ws-server` | Custom RPC/WS (highest priority) |
| `--allow-insecure-rpc` | Allow `http://` / `ws://` (XRPL and Flare RPC); signing is still refused over insecure endpoints |
| `--tick-rate` | TUI loop tick rate |
| `--seed` / `--mnemonic` | Override credentials (**non-preferred** — use env/config) |
| `--account` | Watch address override |
| `--watch` | Read-only (TC-234–235): `lib::enter_watch_mode` drops every key from `Config` (seed / mnemonic incl. plaintext, `flare_evm_key`) before the TUI / CLI start, keeping only the key's address as the default `--account`. No key → Account panel instead of Wallet (no composer, no `g`); poll task gets no credential; `-x` signing subcommands (`Cmd::signs`) refused in `resolve_lazyxrp_command`. Insecure-endpoint signing checks see no credential |
| `-x` / `--exec` | Script CLI mode |

### Config file (`Config::xrpl`)

- Paths: `$XDG_CONFIG_HOME/lazyxrp/config.toml` → `~/.config/lazyxrp/config.toml`
- Fields: `network`, `account`, `issuer`, `currency`, `currency_code`, `offer_limit`, `poll_interval_ms` (default 5000; raise for 24/7 dashboards), `slippage_bps` (Payment quote, default 100 = 1%), `oracles`, `oracle_pairs`, `[xrpl.signing]`, optional `rpc_server` / `ws_server`
- Default issuer fallback (mainnet Bitstamp USD): `rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B`
- `currency_code`: 160-bit hex for `book_offers` (default display `"USD"`)

### Flare (`[flare]`)

| Field | Default | Notes |
|-------|---------|-------|
| `network` | `flare` | `flare` \| `songbird` \| `coston2` — RPC preset; `FLARE_RPC_URL` env wins |
| `display` | `full` | `full` \| `compact` \| `off` — panel layout + FTSO/FXRP poll skip when `off` (C3 URL unchanged) |
| `fassets.execute` | `false` | Must be true for C3 `executeDirectMinting` |
| `wallet.address` | _(unset)_ | Optional `0x…` EVM address — Overview read panel (native FLR + FXRP); invalid hex fails config load |
| `smart_accounts.expected_operator` | _(unset)_ | XRPL `r…` operator pin for Smart Account instruction Payments. Flare RPC's `getXrplProviderWallets()[0]` must equal it (unset / mismatch → refused). Verify out of band |

## 5. Read APIs & panels (FR-08–FR-11)

Types, RPC methods, `Action` variants: `src/xrpl/types.rs`. UX layout: [`DESIGN.md`](../DESIGN.md) §4.

### 5.1 Panel inventory

| Tab | Components | Primary RPC / source |
|-----|------------|----------------------|
| Overview | `ServerPanel`, `CombinedOraclePanel`, `FlareWalletPanel` | `server_info`, `fee`, `get_aggregate_price`, Flare FTSO, FXRP AssetManager, `eth_getBalance` + FXRP `balanceOf` (Overview tab only) |
| Overview (Flare) | `FxrpDirectMintPanel` (inside `CombinedOraclePanel`) | Smart Account read (§6.9): `MasterAccountController` views for the signing (else watched) account, same Flare batch and gating as FXRP |
| Account | `WalletPanel`, `AccountPanel`, `TxHistoryPanel` (top is wallet **xor** account; layout: [`DESIGN.md`](../DESIGN.md) §4) | `account_info`, `account_tx`, wallet submit |
| Market | Book, PathFind, AMM, TrustLines, FTSO, Oracle | `book_offers`, `ripple_path_find`, `amm_info`, `account_lines`, oracles |
| Assets | `NftTab` + 3× `LedgerObjectsPanel` (objects / pay channels / escrows; layout: [`DESIGN.md`](../DESIGN.md) §4) | `account_nfts`, `account_objects` |

Poll skips heavy market fetches when tab inactive (`should_poll_market_book`, `should_poll_asset_panels` in `poll.rs`).

### 5.2 Shared behaviors

- **TX detail:** Any table row with `ArcValue` tx/meta can open unified overlay (`tx_detail::render_tx_detail`). Parser contract: §5.2.1. Overlay **keys**: [`DESIGN.md`](../DESIGN.md) §6.
- **Tx history pagination:** `marker` → `PollCommand::TxHistoryMore` → `XrplTxHistoryAppend`.
- **Tx history filter:** In-panel filter state; does not change RPC query (client-side).
- **Path find:** Self-payment preview via `ripple_path_find` using configured book pair amount.

#### 5.2.1 TX detail parser pipeline

| Symbol | File | Role |
|--------|------|------|
| `TxDetailState` | `tx_detail/mod.rs` | Overlay visibility, scroll offset, line cache |
| `render_tx_detail` | `tx_detail/mod.rs` | Draw centered popup |
| `typed_detail_lines` | `tx_detail/parsers.rs` | Dispatch by `TransactionType` |
| `detail_lines_for` | `tx_detail/mod.rs` | Fallback for unknown fields |
| `TX_DETAIL_PARSERS` | `tx_detail/parsers.rs` | Static registry (29 types) |

Any table row carrying `raw_json: ArcValue` (tx + meta) can open the overlay via `Action::TxDetailToggle` when focused.

**Pipeline**

1. **Header** — `Result` always; `hash` / `ledger` / `date` when present in JSON.
2. **Typed branch** — `typed_detail_lines` looks up `TransactionType` in `TX_DETAIL_PARSERS`; parser returns formatted `Line` list or `None`.
3. **Fallback** — `detail_lines_for` walks known field names; unlisted keys via `format_value`. `Amount2` is known for AMM typed sections (not duplicated under “Other fields”).
4. **Cache** — First open per TX builds `cached_lines`; redraw reuses cache; scroll only moves paragraph offset. New TX invalidates cache.

**Overlay guard** (`TxDetailState::handle_action`)

- While **closed**: panels handle `TxDetailToggle` with pre-resolved `(tx, meta)` from selected row.
- While **open**: consumes `Quit`, data refresh, `Tick`, and navigation actions so background tables do not change selection under the popup.
- `SelectNext` / `FocusNext` (and prev) scroll detail text with saturating arithmetic.

**Registered types (29)**

`Payment`, `AccountSet`, `TrustSet`, `OfferCreate`, `OfferCancel`, `NFTokenMint`, `NFTokenBurn`, `NFTokenCreateOffer`, `NFTokenAcceptOffer`, `NFTokenCancelOffer`, `CheckCreate`, `CheckCash`, `CheckCancel`, `SignerListSet`, `SetRegularKey`, `DepositPreauth`, `EscrowCreate`, `EscrowFinish`, `EscrowCancel`, `PaymentChannelCreate`, `PaymentChannelFund`, `PaymentChannelClaim`, `AMMCreate`, `AMMDeposit`, `AMMWithdraw`, `AMMVote`, `AMMBid`, `AMMDelete`, `TicketCreate`.

Six AMM variants share `amm_detail_lines` (ordered fields, explicit `null`, `TradingFee` formatting on create/vote).

**Adding a parser**

1. Implement `fn foo_detail_lines(tx: &Value) -> Option<Vec<Line>>` in `parsers.rs` (prefer `typed_detail` helper).
2. Add one row to `TX_DETAIL_PARSERS`.
3. Add `/// TC-xxx` test in `parsers.rs` or `registry_tests` when behavior is user-visible or regression-prone.
4. Update the type list in this section if the registry grows.

No `DESIGN.md` change unless overlay interaction changes.

**Tests:** **TC-094** — registry has no duplicates; required types dispatch. Panel integration — tables open overlay on `Enter` (see `docs/test.md` wallet/history rows).

### 5.3 Path-Find (Market)

- Polling: `book_pair.path_find_destination_amount_preview()` as destination amount.
- Display: routes sorted cheapest-send-first; raw route JSON available via TX detail pattern.

### 5.4 Ledger objects

`PollCommand::LedgerObjects` → `account_objects` → `Action::XrplLedgerObjects`. Assets tab filters by `LedgerEntryType` (objects / pay channels / escrows).

---

## 6. Network & signing (FR-12–FR-13)

### 6.1 Network presets

| Network | RPC | WS |
|---------|-----|-----|
| mainnet | `https://xrplcluster.com` | `wss://xrplcluster.com` |
| testnet | `https://s.altnet.rippletest.net:51234` | `wss://s.altnet.rippletest.net:51233` |
| devnet | `https://s.devnet.rippletest.net:51234` | `wss://s.devnet.rippletest.net:51233` |
| xahau | `https://xahau.network` | `wss://xahau.network` |
| xahau-test | `https://xahau-test.net` | `wss://xahau-test.net` |

**TUI session switch:** `<Ctrl-n>` cycles networks ([`DESIGN.md`](../DESIGN.md) §6). Poll/WS rebound via `network_watch`; panels refresh. **Not persisted** to config. Custom `--server` / `XRPL_RPC_SERVER` → `custom_rpc`: endpoint unchanged on switch. **Mainnet guard** applies to `mainnet` and `xahau`.

### 6.2 Resolution precedence

```
CLI --network > XRPL_NETWORK env > config.toml [xrpl] network > default (mainnet)
CLI --server / --ws-server > XRPL_RPC_SERVER / XRPL_WS_SERVER env > config rpc_server / ws_server > network preset
```

### 6.3 Config example

```toml
[xrpl]
network = "mainnet"   # mainnet | testnet | devnet | xahau | xahau-test

[flare]
network = "flare"     # flare | songbird | coston2
display = "full"      # full | compact | off

[flare.smart_accounts]
expected_operator = "r…"   # verified out of band; required for Smart Account writes
```

### 6.4 Connection bootstrap

`src/lib.rs`: network = `--network` or `config.xrpl.network` (env merged by `Config::new`); `resolve_rpc_url`, `resolve_ws_url`. `src/main.rs` is thin `lazyxrp::run()`.

### 6.5 StatusBar network indicator

Right-aligned badge `format!(" {} ", display_name())`. Chrome: [`DESIGN.md`](../DESIGN.md) §5. Updates on `Action::NetworkChange` (TC-115).

### 6.6 Secrets (FR-13)

Load order (seed **xor** mnemonic):

1. CLI `--seed` or `--mnemonic`
2. `XRPL_SEED` / `XRPL_MNEMONIC`
3. `[xrpl.signing]` in config

`SecretString` + `SigningCredential`; production write prompt via `prompt_production_confirmation` unless `--yes`.

### 6.7 Flare display levels

Pixels / focus counts: [`DESIGN.md`](../DESIGN.md) §4.

| Level | Poll |
|-------|------|
| `full` | FTSO + FXRP + Smart Account fetch on Overview tab |
| `compact` | Same as full |
| `off` | Skip FTSO/FXRP/Smart Account **read** fetch; C3 execute URL kept. Composers `6` / `a` need their read, so they stay unavailable |

### 6.8 Flare wallet read panel (FR-16)

- Config: `[flare.wallet] address` (optional EVM `0x…`).
- Overview right column: `CombinedOraclePanel` (top) + `FlareWalletPanel` (bottom) when `display != off`. Split: [`DESIGN.md`](../DESIGN.md) §4.
- Poll: `fetch_flare_wallet_balance` on Overview tab only; emits `Action::FlareWalletBalance`. Unconfigured address → in-panel setup guidance (no RPC).
- Read-only: shows native FLR, FXRP ERC20 balance, `[flare.fassets] execute` gate, executor env key indicator. No Flare writes.

### 6.9 Flare Smart Accounts (proof-based instruction flow)

- Read (`flare::fetch_smart_account_info(rpc, xrpl_address)` → `SmartAccountInfo` → `Action::SmartAccountInfo`): resolves `MasterAccountController` through the `FlareContractRegistry` (`getContractAddressByName`, cached per RPC URL like `AssetManagerFXRP`; a zero address is an error), then `getPersonalAccount(xrpl)`, `getNonce(personal)`, `getXrplProviderWallets()`, `getInstructionFee(1)` (FXRP transfer) / `getInstructionFee(2)` (FXRP redeem) in XRP drops, FXRP `balanceOf(personal)` in token base units, FXRP `decimals()` (`SmartAccountInfo.fxrp_decimals`; a value above 18 is an error; both via the token from AssetManager `fAsset()`) and `eth_getCode(personal)` (deployed = code non-empty; the account is deployed on its first instruction). Remote values use checked conversions (never panic). Read-only.
- Poll: in the same Flare batch as FTSO / FXRP (`poll_batch`): Overview tab only, `flare display != off`, flare backoff window, owner = signing account else the watched account (skipped when empty). A success arms the status-bar flare health like the other flare actions.
- UI: extra rows inside the FXRP panel (personal account, FXRP balance, nonce + deployed, operator, both fees); remote strings are sanitized. Composer `a` needs the read (else a flash) and refuses a read made for another XRPL account.
- Instruction encoding (`src/signing.rs`; TC-239): 32 bytes = `[0] instruction id` · `[1] walletId` (always 0, "unassigned") · `[2..12] value` (10 bytes big-endian, max 2^80−1, 0 refused) · `[12..32]` recipient (transfer: 20-byte EVM address, zero address refused) or zero (redeem). `0x01` transfer: value = FXRP **raw base units** (the contract calls `FXRP.transfer(to, value)` with it unchanged; no on-chain conversion), `0x02` redeem: value = **lots**. The encoders are unit-agnostic; the composer converts the decimal FXRP amount with the **read** `decimals()` (`signing::parse_token_amount`: exact integer math, more fraction digits than `decimals` refused, `decimals > 18` refused) and compares it to the read `fxrp_balance_units` (exact balance accepted, +1 unit refused; TC-247). It refuses to queue while the read / `decimals()` is not loaded. The encoder is pinned to the reference vectors (value 10 / 1234567 transfer to `0xf548…131d`, redeem 3) in TC-239.
- Trust boundary: operator address and fee come from one Flare RPC answer, so the Payment is signed only when the operator equals `[flare.smart_accounts] expected_operator` and the fee is `1..=MAX_SMART_ACCOUNT_FEE_DROPS` (10 XRP). Mainnet needs `--yes`; simulate precedes sign/submit.
- Memo instructions (`-x sa-memo <ignore-memo|set-nonce|replace-fee|set-executor|remove-executor> <XRP>`; TC-344): layouts from upstream `MemoInstructions.sol` (`0xE0` skip memo, `0xE1` fast-forward nonce, `0xE2` replace executor fee, `0xD0` / `0xD1` executor pin). They are Direct Mint Payments to the Core Vault (C2 pipeline with `custom_memo`; Core Vault pin, fee check, simulate-first, `--yes` on mainnet, no destination tag), finalized by an executor with `fxrp-execute <tx> --data 0x`. `0xE1` reads the personal account's `getNonce` first.
- Not implemented (ROADMAP): custom instructions `0xFE` / `0xFF` (`PackedUserOperation` construction). Vault instructions (`-x sa-vault`), `executeDirectMintingWithData`, FDC proof fetching (`-x fxrp-execute`) and the memo instructions above shipped in F3.

---

## 7. Operations notes

- `start_poll_task` = `tokio::spawn` with `PollContext`.
- WS triggers debounced; interval poll uses `poll_interval_ms`. dUNL + Flare registry addresses are session-cached (see §3).
- Risks / debt: [`problems.md`](problems.md), [`security.md`](security.md).
