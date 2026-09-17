# Roadmap — lazyxrp

**Role:** 進捗管理（マイルストーンとポートフォリオバックログの SSOT）。

**Planning view**. Requirements → [`requirements.md`](docs/requirements.md) · System behavior → [`architecture.md`](docs/architecture.md) · UI/UX → [`DESIGN.md`](DESIGN.md).

**Current version:** `Cargo.toml` → 0.2.10
**Last updated:** 2026-09-17

## Milestones

| Milestone | Version | Status | Notes |
|-----------|---------|-----------------|--------|-------|
| Quality pass | 0.1.23 | **Done** | |
| Agent/docs hygiene | — | **Done** | |
| Wallet TX + FXRP | 0.2.0+ | **Done** (2026-09-12) | C3 mainnet smoke → Open work below |
| Docs / agent hygiene | — | **Done** | Human doc index in README; `docs/` self-contained |
| Ratatui optimization | — | **Done** | dirty render + shared table shipped |
| Docs hierarchy redesign | — | **Done** (2026-09-12) | keys → `DESIGN.md`; architecture = behavior; removed `docs/README` + `docs/tasks` |
| Network multichain | 0.2.5+ | **Done** (2026-09-12) | Xahau + `<Ctrl-n>` session switch + Flare display + wallet read panel |
| Test coverage / local integration pass | 0.2.9 | **Done** (2026-09-16) | Local RPC read-only CLI coverage, `lazyxrp`/`rp` binary smoke tests, Assets and panel state coverage |
| Review batch #3: production guard + test coverage | 0.2.10 | **Done** (2026-09-17) | Xahau production write guard, local CLI RPC migration, nft_image SSRF hardening, binary smoke tests, alloy 2 / MSRV 1.94.1 |

## Open work

### Manual / external

| Task | Priority |
|------|----------|
| Mainnet C3 execute smoke (`execute=true`) | P2 — needs FDC verifier API key + real funds; Coston2 pipeline documented |


### Cross-cutting (no change yet)

| Item | Source | Priority |
|------|--------|----------|
| B-02 EscrowCreate composer UI | `docs/problems.md` P-003 | P3 |
| F-01 AccountDelete TX | — | Later |
| F-02 OfferCancel TX | — | Later |
| F-03 EscrowFinish/Cancel TX | — | Later |
| F-04 AMM Deposit/Withdraw TX | — | Later |
| Fake WebSocket integration fixtures | `docs/test.md` | P2 — local WS boundary coverage remains open |
| Global row cache (A1) | quality-pass bottleneck plan | Later |

## Task tracking

| Concern | SSOT |
|---------|------|
| Portfolio milestones + cross-cutting backlog | This file |
| This week's implementation checkboxes | `openspec/changes/<name>/tasks.md` (when using OpenSpec) |
| Completion history | `openspec/changes/archive/**` + git |
| Human doc index | Root `README.md` § Documentation |
| Agent contracts | `openspec/specs/` + `openspec/config.yaml` |

## How to add work

1. Add a row to the Milestones table in this file
2. New requirement → `docs/requirements.md` (FR/NFR)
3. Shipped behavior / network / config rules → `docs/architecture.md` (+ TC in `docs/test.md`)
4. Keys, layout, colors, modals → root [`DESIGN.md`](DESIGN.md)
5. Active implementation checkboxes (when using OpenSpec) → `openspec/changes/<name>/tasks.md`
