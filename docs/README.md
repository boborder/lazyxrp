# lazyxrp Documentation

Single entry point for human docs, agent contracts, and the graphify knowledge graph.

## Who reads what

| Audience | Start here | Then |
|----------|------------|------|
| **Human (product / UX)** | This page → [`design.md`](design.md) | [`requirements.md`](requirements.md), [`test.md`](test.md) |
| **Human (security audit)** | [`security.md`](security.md) | [`agent/RISK_REGISTER.md`](agent/RISK_REGISTER.md) (implementation risks) |
| **Agent (change / review)** | Root [`AGENTS.md`](../AGENTS.md) | [`agent/`](agent/) (architecture, invariants, risks) |
| **Structure exploration** | [`graphify.md`](graphify.md) → [`../graphify-out/GRAPH_REPORT.md`](../graphify-out/GRAPH_REPORT.md) | `graphify query` / `path` / `explain` |

## Human-facing specs (`docs/`)

| Document | Contents | SSOT for |
|----------|----------|----------|
| [`requirements.md`](requirements.md) | Functional & non-functional requirements | FR/NFR IDs |
| [`design.md`](design.md) | Product behavior: modes, tabs, wallet UX, phases | UX & feature behavior |
| [`tx-detail.md`](tx-detail.md) | TX detail overlay pipeline & parser inventory | TX detail rendering |
| [`directory.md`](directory.md) | Directory tree & file ownership | Paths |
| [`test.md`](test.md) | Test strategy & case list (TC-001〜) | TC-IDs |
| [`security.md`](security.md) | Security review history (S-xxx) | Audit findings |
| [`problems.md`](problems.md) | Known issues & troubleshooting | Workarounds |
| [`tasks.md`](tasks.md) | Active tasks & milestones | Roadmap |
| [`RELEASE.md`](RELEASE.md) | Pre-publish checklist | Release gate |
| [`tech.md`](tech.md) | Tech stack & versions | Toolchain |
| [`references.md`](references.md) | External references | Links |
| [`architecture/c4-context.md`](architecture/c4-context.md) | C4 system context | Context diagram |
| [`architecture/c4-containers.md`](architecture/c4-containers.md) | C4 containers | Container diagram |

## External snapshots (`docs/external/`)

Curated notes for systems outside lazyxrp. **SSOT remains the linked official docs**; tables here include a snapshot date and may drift.

| Document | Contents |
|----------|----------|
| [`external/fassets-direct-mint-mainnet.md`](external/fassets-direct-mint-mainnet.md) | Flare Mainnet FXRP: Direct Minting (Tag, Memo, Smart Account C-1/C-2), go-live checklist |
| [`external/fassets-direct-mint-monitoring.md`](external/fassets-direct-mint-monitoring.md) | Same flows: mint-job state machine, events, SLAs, alerts, runbook |

## Agent-facing contracts (`docs/agent/`)

| Document | Contents | SSOT for |
|----------|----------|----------|
| [`agent/ARCHITECTURE.md`](agent/ARCHITECTURE.md) | Modules, channels, execution flows | Structure & data flow |
| [`agent/INVARIANTS.md`](agent/INVARIANTS.md) | Rules I-1〜I-11 | Must-not-break rules |
| [`agent/DATA_MODEL.md`](agent/DATA_MODEL.md) | Types, serialization, lifecycles | Domain types |
| [`agent/DEPENDENCY_RULES.md`](agent/DEPENDENCY_RULES.md) | Allowed imports | Module boundaries |
| [`agent/RISK_REGISTER.md`](agent/RISK_REGISTER.md) | Risks R-001〜 | Implementation risks |
| [`agent/RISK_TO_TESTS.md`](agent/RISK_TO_TESTS.md) | R ↔ TC mapping | Test coverage gaps |
| [`agent/DESIGN_ISSUES.md`](agent/DESIGN_ISSUES.md) | Known design debt | Refactor backlog |
| [`agent/CHANGE_GUIDE.md`](agent/CHANGE_GUIDE.md) | Per-module change checklist | How to change safely |
| [`agent/REPO_INVENTORY.md`](agent/REPO_INVENTORY.md) | Build commands, entry points | Commands & tree |

Do not duplicate channel diagrams or invariant tables in `design.md` — link here instead.

## Code map (graphify)

Built from commit in [`../graphify-out/GRAPH_REPORT.md`](../graphify-out/GRAPH_REPORT.md) (`Built from commit:` line). After code changes on tracked paths, run `graphify update .` from the repo root.

| God node (graph) | Role | Primary source |
|------------------|------|----------------|
| `detail_lines_for()` | TX detail line builder | `src/components/shared/tx_detail/mod.rs` |
| `build_detail_lines()` | Typed + fallback field lines | `src/components/shared/tx_detail/mod.rs` |
| `push_common_lines()` | Shared header fields | `src/components/shared/tx_detail/format.rs` |
| `WalletPanel` | Wallet + recent TX + composer | `src/components/panels/wallet.rs` |
| `RpcClient` | HTTP RPC (bridge to xrpl/) | `src/xrpl/client.rs` |
| `run()` | TUI event loop | `src/app.rs` |

See [`graphify.md`](graphify.md) for community → doc mapping and CLI usage.

## When to read which doc

| Task | Read first | Also |
|------|------------|------|
| New feature / UX change | `requirements.md`, `design.md` | `agent/CHANGE_GUIDE.md` |
| Bug in submit / seed / mainnet | `agent/INVARIANTS.md` (I-2, I-3) | `agent/RISK_REGISTER.md`, `security.md` |
| New `Action` variant | `agent/ARCHITECTURE.md` | Root `AGENTS.md` |
| New TX type in detail overlay | `tx-detail.md` | `agent/DESIGN_ISSUES.md` (Issue 9) |
| Cross-module “how does X relate to Y?” | `graphify.md` + `GRAPH_REPORT.md` | `graphify query "…"` |
| Config key change | `directory.md`, `tech.md` | Every `docs/` file that mentions the key |
| Flare FAssets direct mint (Mainnet) | `external/fassets-direct-mint-mainnet.md` | `external/fassets-direct-mint-monitoring.md`, [Operational parameters](https://dev.flare.network/fassets/operational-parameters) |
