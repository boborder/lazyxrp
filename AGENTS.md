# AGENTS.md

**lazyxrp** — Rust TUI for XRPL. Agents follow this contract on every task.

## Quick reference

| Topic    | Command / note |
|----------|------------------|
| Verify   | `cargo check` (minimum after code changes) |
| Format   | `cargo fmt` |
| Install  | `./install.sh` or [`mise run install`](https://mise.jdx.dev/) (see `.mise.toml`) |

Index of linked agent docs: [`docs/agents/README.md`](docs/agents/README.md).

## [CRITICAL] Execution contract (summary)

1. If the request is missing target, reproduction, or completion criteria → ask **1–2** focused questions first.
2. If sufficient → implement.
3. After implementation → run **`cargo check`** (minimum).
4. After `cargo check` → sync related **`docs/`**.
5. Changes to **`src/config.rs`** keys or behavior → update every **`docs/`** file that mentions them, in the same change.

Full rules and the missing-information template: [Execution contract](docs/agents/execution-contract.md).

## [REQUIRED] Prohibitions

- No unrelated refactors or renames in the same change.
- Do not remove features just to bypass errors.
- Do not leave behavior that contradicts `docs/`.

## Detailed guidelines

- [Execution contract](docs/agents/execution-contract.md) — decision flow, urgency/assumptions, `config.rs` + docs sync, missing-info template.
- [Development policy](docs/agents/development-policy.md) — Rust style, `cargo fmt` / `cargo check`, commits, branching, Git safety.
- [Testing and TDD](docs/agents/testing.md) — unit tests, `cargo check`, pointer to `docs/test.md`.
- [Project reference](docs/agents/project-reference.md) — `docs/directory.md`, key files, deploy, troubleshooting.
