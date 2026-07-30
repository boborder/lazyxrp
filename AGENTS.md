# AGENTS.md

**lazyxrp** — Rust TUI for XRPL. Elm-style unidirectional flow (`Action` → `App` → panels); network I/O under `xrpl/`, UI under `components/`.

## Quick reference

| Topic | Command / note |
|---|---|
| Rust pin | `rust-toolchain.toml` + `Cargo.toml` `rust-version` (see `docs/tech.md`) |
| Domain skills | [`.agents/skills/xrpl-rust/`](.agents/skills/xrpl-rust/SKILL.md) · [`.agents/skills/ratatui-tui/`](.agents/skills/ratatui-tui/SKILL.md) |
| Verify | `cargo check` (minimum after code changes) |
| Format | `cargo fmt` |
| Install | `./install.sh` or `mise run install` (see `.mise.toml`) |
| Tests | [`docs/test.md`](docs/test.md) |

## Execution contract

1. Missing **target**, **reproduction**, or **completion criteria** → ask **1–2** focused questions before implementing.
2. Otherwise implement.
3. After implementation → run **`cargo check`** (minimum).
4. After `cargo check` → sync related **`docs/`**.
5. Changes to **`src/config.rs`** keys or behavior → update every **`docs/`** file that mentions them in the same change.

If assumptions are unavoidable, state them explicitly before proceeding.

## Critical overrides

- Components never call `xrpl/` clients/`poll`/`app` — `Action` flow only.
- Never sign/submit without simulate; mainnet writes require `--yes`.
- Use `secret_seed` only (cleared `seed` is invalid). Never mutate shared `ArcValue` JSON.
- No unrelated refactors; do not remove features to bypass errors; do not contradict `docs/`.
- Do not skip `cargo check` after implementation.

## Detailed instructions

- [Architecture](docs/agent/ARCHITECTURE.md) · [Invariants](docs/agent/INVARIANTS.md) · [Data model](docs/agent/DATA_MODEL.md)
- [Dependency rules](docs/agent/DEPENDENCY_RULES.md) · [Change guide](docs/agent/CHANGE_GUIDE.md) · [Risk register](docs/agent/RISK_REGISTER.md)
- [Repo inventory](docs/agent/REPO_INVENTORY.md) · [Design issues](docs/agent/DESIGN_ISSUES.md) · [ADR 0001](docs/agent/adr/0001-observed-architecture.md)
- Graphify: [`graphify-out/GRAPH_REPORT.md`](graphify-out/GRAPH_REPORT.md) — if `HEAD` differs or `graphify-out/needs_update` exists, run `graphify update .` before structure queries.
