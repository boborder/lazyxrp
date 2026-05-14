# AGENTS.md

**lazyxrp** — Rust TUI for XRPL. Agents follow this contract on every task.

**Summary**: Elm-style unidirectional flow (`Action` → `App` → panels); network I/O lives under `xrpl/`, UI under `components/`. Details: [`docs/agent/ARCHITECTURE.md`](docs/agent/ARCHITECTURE.md).

## Quick reference

| Topic    | Command / note |
|----------|------------------|
| Rust pin | `rust-toolchain.toml` + `Cargo.toml` `rust-version` (see `docs/tech.md`, Rust version) |
| Rust idioms | [`.agents/skills/rust-skills/SKILL.md`](.agents/skills/rust-skills/SKILL.md) — prefixes `own-` / `err-` / `async-` / …; invoke `/rust-skills` |
| Verify   | `cargo check` (minimum after code changes) |
| Format   | `cargo fmt` |
| Install  | `./install.sh` or `mise run install` (see `.mise.toml`) |

## Execution contract

1. If the request is missing **target**, **reproduction**, or **completion criteria** → ask **1–2** focused questions before implementation.
2. If sufficient → implement.
3. After implementation → run **`cargo check`** (minimum).
4. After `cargo check` → sync related **`docs/`**.
5. Changes to **`src/config.rs`** keys or behavior → update every **`docs/`** file that mentions them, in the same change.

If immediate action is required and assumptions are unavoidable, state them explicitly and get agreement before proceeding. Proactively mention `docs/` sync and `cargo check` when the topic is clearly relevant.

### Missing-information template

Adapt to the request type. Skip items that don't apply:

- **Target**: Which file, feature, or configuration key should be changed?
- **Reproduction**: What input/steps reproduce the issue, and what is expected vs actual behavior?
- **Completion criteria**: What defines done (tests, output, behavior)?

## Prohibitions

- No unrelated refactors or renames in the same change.
- Do not remove features just to bypass errors.
- Do not leave behavior that contradicts `docs/`.

## Development policy

- Follow Rust 2024 conventions; channel is in `rust-toolchain.toml` (`stable`), MSRV in `Cargo.toml`.
- Commit `Cargo.lock` and use `cargo … --locked` in CI.
- Use Conventional Commits (e.g. `fix(xrpl): avoid tokio spawn lifetime issue`).
- Follow GitHub Flow: branch from `main`, merge via PR.
- Do not run destructive git operations (e.g. `reset --hard`) without explicit agreement.

## Testing

- Add tests at the smallest practical unit for important logic.
- See `docs/test.md` for test policy, TC-ID case list, and TDD roadmap.

## Project reference

- Directory structure: `docs/directory.md`.
- Key config: `Cargo.toml` (dependencies), `src/config.rs` (default config/merge behavior).
- Install/distribution: `README.md` (`./install.sh`, `mise run install`).
- Troubleshooting: `docs/problems.md` (unused code warnings, `critical-section` symbols).

## Architecture rules

- **Message flow**: `Action` flows top-down only (WS/poll tasks → `action_rx` → `App` → components). Components never call `xrpl/` directly.
- **Tab-panel consistency**: `TAB_TITLES.len()` MUST equal `panels.len()` (currently 5). Add assertion when changing.
- **Config merge priority**: CLI flags > env vars > user `config.toml` > built-in defaults (`config.json5`). Never invert this order.
- **Module boundaries**: `components/` imports only from `action`, `config`, `xrpl/types`. Never from `xrpl/client`, `xrpl/poll`, or `app`.
- **Coupling / drift**: [`docs/agent/DEPENDENCY_RULES.md`](docs/agent/DEPENDENCY_RULES.md), [`docs/agent/DESIGN_ISSUES.md`](docs/agent/DESIGN_ISSUES.md).

## Data model rules

- **Seed safety**: Plaintext seed (`RawSigningConfig.seed`) MUST be cleared after `Config::new()`. Only `secret_seed: SecretString` remains.
- **ArcValue immutability**: `TxRow.tx_json` / `meta_json` are shared via `ArcValue`. NEVER mutate shared JSON — use clone if modification needed.
- **Submit pipeline**: All TX submits MUST follow: validate → `simulate_tx` → check `tesSUCCESS` → extract `Sequence`/`Fee` → `sign` → `submit`. Never sign without simulation.
- See: [`docs/agent/DATA_MODEL.md`](docs/agent/DATA_MODEL.md).

## Invariants not to violate

Cheat sheet (full list **I-1–I-11** + enforcement table): [`docs/agent/INVARIANTS.md`](docs/agent/INVARIANTS.md).

| Invariant | Rule |
|-----------|------|
| I-2 | Mainnet writes require `--yes` flag |
| I-3 | simulate → sign → submit (never skip simulate) |
| I-6 | Poll interval ≥ 10s (`MIN_POLL_INTERVAL`) |
| I-7 | "not found" from `account_tx` → empty list, not error |
| I-8 | `Tui::drop()` MUST NOT panic (terminal cleanup) |
| I-11 | Every RPC call MUST have a timeout |

## Side-effect boundaries

| Boundary | Location | Mechanism |
|----------|----------|-----------|
| Terminal I/O | `tui.rs` | crossterm raw mode |
| RPC (HTTP) | `xrpl/client.rs` | reqwest |
| WebSocket | `xrpl/ws.rs` | xrpl-rust WS client |
| File system (config) | `config.rs` | config crate |
| File system (logs) | `logging.rs` | tracing file appender |
| Environment | `config.rs`, `main.rs` | std::env::var |
| Process signal | `tui.rs` | signal-hook SIGTSTP |
| Panic handler | `errors.rs` | human-panic, better-panic |

## How to make a change

1. **Locate**: [`docs/agent/ARCHITECTURE.md`](docs/agent/ARCHITECTURE.md) component map.
2. **Design**: [`docs/agent/INVARIANTS.md`](docs/agent/INVARIANTS.md) — invariant violations?
3. **Implement**: [`docs/agent/CHANGE_GUIDE.md`](docs/agent/CHANGE_GUIDE.md) per-module notes.
4. **Validate**: Minimum `cargo check`; relevant tests; `docs/test.md` TC-IDs when applicable.
5. **Document**: Config keys → all affected `docs/` in the same change.

## How to validate a change

- `cargo check` — minimum gate
- `cargo fmt` — style
- `cargo test` — full test suite
- For config changes: verify merge precedence (built-in → file → env → CLI)
- For submit changes: verify simulate → sign → submit flow and mainnet guard

## When to update docs

- **Always**: `src/config.rs` key changes → update all `docs/` mentioning that key
- **Always**: New `Action` variant → sync [`docs/agent/ARCHITECTURE.md`](docs/agent/ARCHITECTURE.md) data flow
- **Always**: New TX type parser → update parser-count note in [`docs/agent/DESIGN_ISSUES.md`](docs/agent/DESIGN_ISSUES.md)
- **Consider**: New invariant → [`docs/agent/INVARIANTS.md`](docs/agent/INVARIANTS.md); new cross-module import pattern → [`docs/agent/DEPENDENCY_RULES.md`](docs/agent/DEPENDENCY_RULES.md)

## Forbidden shortcuts

- ❌ Sign and submit without simulation
- ❌ Mainnet writes without `--yes` guard
- ❌ Skip `cargo check` after implementation
- ❌ Mutate `ArcValue` shared JSON
- ❌ Use `config.xrpl.signing.seed` (cleared field) — always use `secret_seed`
- ❌ Remove features just to bypass compile errors
- ❌ Unrelated refactors in same change

## Current high-risk areas

From [`docs/agent/RISK_REGISTER.md`](docs/agent/RISK_REGISTER.md) — tackle with tests before refactors when possible:

- **R-001** (High): Seed priority chain across CLI/env/file — centralized resolution needed
- **R-002** (High): Submit pipeline error silently swallowed on channel close
- **R-005** (High): TUI Drop panic could leave terminal in raw mode
- **R-006** (Critical): Mainnet write guard bypass via param construction

## Documentation map (`docs/agent/`)

Keep this file operational; expand in linked docs only.

| Doc | Use when |
|-----|----------|
| [`REPO_INVENTORY.md`](docs/agent/REPO_INVENTORY.md) | Build commands, entry points, tree |
| [`ARCHITECTURE.md`](docs/agent/ARCHITECTURE.md) | Components, channels, flows |
| [`DATA_MODEL.md`](docs/agent/DATA_MODEL.md) | Types, serialization, lifecycles |
| [`INVARIANTS.md`](docs/agent/INVARIANTS.md) | Rules I-1–I-11, enforcement |
| [`DEPENDENCY_RULES.md`](docs/agent/DEPENDENCY_RULES.md) | Allowed imports, violations |
| [`DESIGN_ISSUES.md`](docs/agent/DESIGN_ISSUES.md) | Known design debt, parser counts |
| [`RISK_REGISTER.md`](docs/agent/RISK_REGISTER.md) | R-001+, scenarios, tests |
| [`CHANGE_GUIDE.md`](docs/agent/CHANGE_GUIDE.md) | Per-module change checklist |
| [`adr/0001-observed-architecture.md`](docs/agent/adr/0001-observed-architecture.md) | Observed baseline ADR |

## Codebase context (graphify)

High-centrality nodes: `detail_lines_for()`, `build_detail_lines()`, `push_common_lines()`, `dim_style()` — TX detail rendering core.
Cross-community bridges: `RpcClient`, `run()` — connect XRPL client ↔ TUI panels.
Agent-relevant communities: branching/commits, deploy/directory/troubleshooting, execution-contract/rules.
Full graph: [`graphify-out/GRAPH_REPORT.md`](graphify-out/GRAPH_REPORT.md) (built from commit `0cd3f568`). If `HEAD` differs or `graphify-out/needs_update` exists, run `graphify update .` before trusting structure queries.
