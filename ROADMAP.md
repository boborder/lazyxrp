# Roadmap — lazyxrp

**Role:** 進捗管理（最終ゴール・優先順位・マイルストーンの SSOT）。注意点・進路変更記録 → [`docs/roadmap-log.md`](docs/roadmap-log.md)。

**Planning view**. Requirements → [`requirements.md`](docs/requirements.md) · System behavior → [`architecture.md`](docs/architecture.md) · UI/UX → [`DESIGN.md`](DESIGN.md).

**Current version:** `Cargo.toml` → 0.2.11
**Last updated:** 2026-09-21

## Final goal (最終ゴール)

XRPL エコシステム（開発者・ユーザー双方）で人気の CLI/TUI ツールになること。**セキュリティの維持は非交渉条件** — guards を弱める機能は出さない（[`docs/security.md`](docs/security.md)）。

## Long-term (長期)

方向性: XRPL / Xahau / Flare の multichain 深掘りと普及（install path・onboarding）。着手は中期消化後に査定。

| Item | Priority |
|------|----------|
| TX surface completion — F-01 AccountDelete, F-03 EscrowFinish/Cancel | Later |
| Global row cache (A1) — render perf at scale | Later |

## Mid-term (中期)

| Item | Source | Priority |
|------|--------|----------|
| Fake WebSocket integration fixtures | `docs/test.md` | P2 |
| Mainnet C3 execute smoke (`execute=true`) | — | P2 — needs FDC verifier API key + real funds (external) |
| B-02 EscrowCreate composer UI | `docs/problems.md` P-003 | P3 |
| F-04 AMM Deposit/Withdraw TX | — | P3 — DEX trading の後続 |

## Short-term (短期) — current focus

| Task | Priority |
|------|----------|
| DEX trading — trading UX: OfferCancel（旧 F-02）+ cross-currency Payment + オーダー管理 UI（OfferCreate composer は実装済み） | P1 |
| CLI redesign — `lazyxrp`/`rp` command surface restructure | P2 |
| Keymap customization — keybindings configurable via `config.toml`（組込デフォルトは `config.json5`） | P3 |

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
