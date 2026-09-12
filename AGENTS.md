# AGENTS.md

**lazyxrp** — Rust TUI for XRPL. Elm-style unidirectional flow (`Action` → `App` → panels); network I/O under `xrpl/`, UI under `components/`.

## Quick reference

| Topic | Command / note |
|---|---|
| Rust pin | `rust-toolchain.toml` + `Cargo.toml` `rust-version` (see `docs/tech.md`) |
| Domain skills | [`.agents/skills/xrpl-rust/`](.agents/skills/xrpl-rust/SKILL.md) · Flare: `flare-general` / `flare-ftso` / `flare-fassets` |
| Human docs | [`README.md`](README.md) § Documentation — **do not duplicate that index here** |
| Verify | `cargo check` (minimum after code changes) |
| Format | `cargo fmt` |
| Install | `./install.sh` or `mise run install` (see `.mise.toml`) |
| Tests | [`docs/test.md`](docs/test.md) |

## Optional: OpenSpec

`openspec/` is **optional** agent change-tracking. It may reference `docs/`; **`docs/` never links to `openspec/`** (one-way).

| Use | Path |
|-----|------|
| Active change checklists | `openspec/changes/<name>/tasks.md` |
| Archived changes | `openspec/changes/archive/` |
| Agent runtime contracts | `openspec/specs/<cap>/spec.md` |
| Hub config | `openspec/config.yaml` |

When shipping behavior: update `docs/` (`roadmap.md`, `architecture.md`, `test.md`, …) in the same change. If OpenSpec was used: `openspec archive <name> -y`.

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
- Use `SigningCredential` via `credential_from_secrets` (family seed **or** BIP39 mnemonic, not both). Cleared plaintext `seed`/`mnemonic` fields are invalid. Never mutate shared `ArcValue` JSON.
- No unrelated refactors; do not remove features to bypass errors; do not contradict `docs/`.
- Do not skip `cargo check` after implementation.

## Detailed instructions

- Graphify: [`graphify-out/GRAPH_REPORT.md`](graphify-out/GRAPH_REPORT.md) — if `HEAD` differs or `graphify-out/needs_update` exists, run `graphify update .` before structure queries.
