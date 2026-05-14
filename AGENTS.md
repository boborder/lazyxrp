# AGENTS.md

**lazyxrp** — Rust TUI for XRPL. Agents follow this contract on every task.

## Quick reference

| Topic    | Command / note |
|----------|------------------|
| Rust pin | `rust-toolchain.toml` + `Cargo.toml` `rust-version`（詳細は `docs/tech.md` §1） |
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

## Codebase context (graphify)

High-centrality nodes: `detail_lines_for()`, `build_detail_lines()`, `push_common_lines()`, `dim_style()` — TX detail rendering core.
Cross-community bridges: `RpcClient`, `run()` — connect XRPL client ↔ TUI panels.
Agent-relevant communities: branching/commits, deploy/directory/troubleshooting, execution-contract/rules.
For the full graph: `graphify-out/GRAPH_REPORT.md` (built from commit `ad1891c8`).
