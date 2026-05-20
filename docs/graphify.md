# graphify — codebase knowledge graph

Generated artifacts live under [`../graphify-out/`](../graphify-out/). They are **not** hand-edited; rebuild after meaningful code or doc changes.

## Artifacts

| File | Use |
|------|-----|
| [`graphify-out/GRAPH_REPORT.md`](../graphify-out/GRAPH_REPORT.md) | God nodes, surprising connections, community list |
| [`graphify-out/graph.html`](../graphify-out/graph.html) | Interactive browser graph |
| [`graphify-out/graph.json`](../graphify-out/graph.json) | Machine-readable graph (GraphRAG, MCP) |

Freshness: compare `Built from commit:` in `GRAPH_REPORT.md` with `git rev-parse HEAD`. Mismatch or `graphify-out/needs_update` → run from repo root:

```bash
graphify update .
```

Full rebuild (semantic + AST): `graphify .`

## CLI for agents and humans

| Command | When |
|---------|------|
| `graphify query "how does wallet submit reach poll"` | Broad neighborhood (BFS) |
| `graphify query "…" --dfs` | Trace one dependency chain |
| `graphify path "WalletPanel" "simulate_tx"` | Shortest path between concepts |
| `graphify explain "detail_lines_for"` | One node + neighbors |

Answers must cite graph edges (EXTRACTED / INFERRED). Do not invent relationships absent from `graph.json`.

Root [`AGENTS.md`](../AGENTS.md) repeats the freshness rule for automated sessions.

## Community → documentation map

Communities are numbered in `GRAPH_REPORT.md` (labels vary by run). Use this map to jump from graph clusters to SSOT docs:

| Graph focus | Typical communities | Read |
|-------------|---------------------|------|
| TX detail rendering | 0, 5, 12 | [`tx-detail.md`](tx-detail.md), [`agent/DESIGN_ISSUES.md`](agent/DESIGN_ISSUES.md) |
| Config / seed / network | 1, 6 | [`agent/ARCHITECTURE.md`](agent/ARCHITECTURE.md), [`security.md`](security.md), [`agent/RISK_REGISTER.md`](agent/RISK_REGISTER.md) (R-001, R-006, R-007) |
| RPC / CLI parsing | 2, 3 | [`agent/DATA_MODEL.md`](agent/DATA_MODEL.md), [`directory.md`](directory.md) |
| Wallet / composer UI | 4 | [`design.md`](design.md) § Wallet |
| Product design narrative | 7 | [`design.md`](design.md) |
| Tests / AGENTS contract | 9, 10, 14 | [`test.md`](test.md), root `AGENTS.md` |
| Requirements | 15 | [`requirements.md`](requirements.md) |
| Dependency rules | 20 | [`agent/DEPENDENCY_RULES.md`](agent/DEPENDENCY_RULES.md) |
| Agent recon baseline | 29 | [`agent/ARCHITECTURE.md`](agent/ARCHITECTURE.md) |

## Surprising connections (examples)

From the current graph — worth reading before refactors:

- `fmt_xrpl_amount()` → `drops_to_xrp()` in `xrpl/client.rs` (UI detail ↔ RPC helpers)
- `signing.rs` seed resolution → `config.rs` `env_lock()` (startup ordering)

Query the live graph for up-to-date edges after large changes.
