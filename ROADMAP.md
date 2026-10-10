# Roadmap — lazyxrp

**Role:** 進捗管理（最終ゴール・優先順位・マイルストーンの SSOT）。注意点・進路変更記録 → [`docs/roadmap-log.md`](docs/roadmap-log.md)。

**Planning view**. Requirements → [`requirements.md`](docs/requirements.md) · System behavior → [`architecture.md`](docs/architecture.md) · UI/UX → [`DESIGN.md`](DESIGN.md).

**Current version:** `Cargo.toml` → 0.2.11
**Last updated:** 2026-10-10

## Final goal (最終ゴール)

XRPL エコシステム（開発者・ユーザー双方）で人気の CLI/TUI ツールになること。**セキュリティの維持は非交渉条件** — guards を弱める機能は出さない（[`docs/security.md`](docs/security.md)）。

## Long-term (長期)

方向性: XRPL / Xahau / Flare の multichain 深掘りと普及（install path・onboarding）。着手は中期消化後に査定。

| Item | Priority | Notes |
|------|----------|-------|
| TX surface completion — F-01 AccountDelete, F-03 EscrowFinish/Cancel | Later | 中期の `poll.rs` 分割後（submit 種別追加の前提） |
| Global row cache (A1) — render perf at scale | Later | 計測で問題が出たら |
| Action エラー型移行（`String` → 型付き） | Later | `docs/security.md` 構造課題。UI 文言の SSOT 化と同時に |
| Watch / Operate UI 分離 | Later | `docs/security.md` 構造課題。署名系 UI を閲覧専用起動から外す |
| パネル内キー（`j`/`k`・`t`・composer）の設定化 | Later | グローバルキーは設定可（DESIGN §6）。需要が出たら中期へ |

## Mid-term (中期)

| Item | Source | Priority | 完了条件 |
|------|--------|----------|----------|
| `poll.rs` 分割（2,477 行: poll loop / read batch / submit 群） | `docs/security.md` 構造課題 | P2 | `submit_*` を `xrpl/submit.rs` へ移動、挙動不変（既存テスト緑のみ）。EscrowCreate / AMM 追加の**前**に実施 |
| Fake WebSocket integration fixtures | `docs/test.md` | P2 | ledger close → poll trigger → panel 更新をローカル WS で検証 |
| Sequence / LastLedgerSequence 整合検査 | `docs/security.md`（Fee cap 節の残余） | P2 | simulate 由来 Sequence ≠ `account_info` / LLS が現在 ledger から過大なら署名拒否 + TC |
| Testnet onboarding: `-x faucet`（testnet/devnet 限定） | 提案（普及・E2E smoke 用） | P2 | 本番ネットワークでは拒否、取得 seed は表示のみ（保存しない） |
| Mainnet C3 execute smoke (`execute=true`) | — | P2 | **外部ブロック**: FDC verifier API key + 実資金 |
| B-02 EscrowCreate composer UI | `docs/problems.md` P-003 | P3 | `poll.rs` 分割後。simulate 経路再利用 |
| F-04 AMM Deposit/Withdraw TX | — | P3 | DEX trading の後続。composer + CLI |
| `account_offers` marker paging（> 200 件） | `client.rs` ponytail 注記 | P3 | 200 件超アカウントで全件表示 |
| 二重送信ガード（同一内容の連続 submit） | `docs/security.md` 未修正リスト | P3 | 直前 submit 結果待ち中の同一 params を拒否 |

## Short-term (短期) — current focus: **0.3.0 リリース**

未リリースの破壊的変更（裸サブコマンド / `watch` 削除）と新 write 経路を、検証してから出す。上から順に着手。

| # | Task | Priority | 完了条件 |
|---|------|----------|----------|
| 1 | ~~CLI write の終了コード修正~~ **済** | P0 | `-x send` の sign/submit 失敗を `?` 伝播 → 非 0 終了（TC-196） |
| 2 | ~~P-004 Payment 検証の一本化（Send max spec 含む）~~ **済** | P1 | `signing::validate_payment_input` を composer preview / submit / quote と poll `PaymentSubmit` が共有（TC-197–198）。signing の model 構築は最終防衛線として残す |
| 3 | ~~poll write 経路の local RPC fixture~~ **済** | P1 | `test_support::rpc_mock` 共有。Payment / OfferCancel / `quote_payment` を検証（TC-199）。他 submit 種別は同じ `simulate_then_finalize` 経由 |
| 4 | Testnet E2E smoke（手動）— **CLI 済**、TUI 残 | P0 | CLI: `-x send` / `-x offers` / `-x offer-cancel` を faucet 口座で validated（hash は `docs/test.md` Testnet E2E）。live `simulate` が LastLedgerSequence を返さず全 write が失敗していた不具合を発見・修正（TC-200）。残: TUI OfferCreate → OfferCancel、`p` 見積り → cross-currency Payment |
| 5 | 0.3.0 リリース | P0 | #1–#4 完了後。`Cargo.toml`/`Cargo.lock` 同一コミット bump、README に CLI 移行注記、`docs/maintenance.md` 手順 |

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
| DEX trading + CLI redesign + keymap docs | 0.2.11+ (unreleased) | **Done** (2026-10-10) | Wallet composer OfferCancel (`8`) + Payment Send max / `p` path quote (`slippage_bps`); CLI: bare subcommands + `watch` removed, `-x send` simulate-first, `-x offers` / `-x offer-cancel`; `--seed`/`--mnemonic` kept with stderr warning (FR-13); global keybindings user-mergeable (DESIGN §6); `rc` alias stays README-only |

## Task tracking

| Concern | SSOT |
|---------|------|
| Goal hierarchy + priorities + shipped history | This file |
| Planning cautions + pivot log | `docs/roadmap-log.md` |
| This week's implementation checkboxes | `openspec/changes/<name>/tasks.md` (when using OpenSpec) |
| Completion history | `openspec/changes/archive/**` + git |
| Human doc index | Root `README.md` § Documentation |
| Agent contracts | `openspec/specs/` + `openspec/config.yaml` |

## How to add work

1. Horizon（短期 / 中期 / 長期）を選んでこのファイルに row 追加（優先度も記録）
2. New requirement → `docs/requirements.md` (FR/NFR)
3. Shipped behavior / network / config rules → `docs/architecture.md` (+ TC in `docs/test.md`)
4. Keys, layout, colors, modals → root [`DESIGN.md`](DESIGN.md)
5. Active implementation checkboxes (when using OpenSpec) → `openspec/changes/<name>/tasks.md`
6. 進路変更・計画上の注意点 → [`docs/roadmap-log.md`](docs/roadmap-log.md)
