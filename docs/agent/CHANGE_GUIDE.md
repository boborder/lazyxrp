# Change Guide

> **Read**: `ARCHITECTURE.md`, `DATA_MODEL.md`, `INVARIANTS.md`, `DEPENDENCY_RULES.md`. **Scope**: full repository. **Generated**: 2026-05-14 (Pass 7).

## Before Coding

1. **Identify change type**: Is it a new feature, bug fix, refactor, or config change?
2. **Check invariants**: Read `docs/agent/INVARIANTS.md` — will your change violate any invariant?
3. **Check risks**: Read `docs/agent/RISK_REGISTER.md` — does your change touch a high-risk area?
4. **Find tests**: Check `docs/test.md` for existing TC-IDs covering the area.

## How to Locate Affected Slice/Module

| Change type | Start here |
|-------------|------------|
| New CLI subcommand | `src/cli.rs` (Cmd enum) → `src/xrpl/cli_exec.rs` (handler) |
| New TUI panel | `src/components/panels/` (new panel) → `src/components/tabs/` (compose into tab) → `src/app.rs` (add to panels vec) |
| New XRPL data source | `src/xrpl/client.rs` (RPC call) → `src/xrpl/types.rs` (row struct) → `src/action.rs` (new Action variant) → polling/WS to emit → component to consume |
| New transaction submit type | `src/xrpl/types.rs` (params struct) → `src/action.rs` (Submit/Ok/Err variants) → `src/xrpl/poll.rs` (submit function) → `src/components/panels/wallet.rs` (UI form) |
| Config key change | `src/config.rs` (LedgerConfig) → `config.json5` (defaults) → `main.rs` (env/CLI overrides) |
| Keybinding change | `config.json5` (add binding) or `src/config.rs` (parse new Action) |
| New TX detail parser | `src/components/shared/tx_detail/parsers.rs` (new parser) → `mod.rs` (add to `detail_lines_for()` dispatch) |
| Style/theme change | `src/components/shared/theme.rs` (colors/styles) |

## How to Trace Data Flow

1. **Inbound data**: WS or RPC response → `Action::Xrpl*` variant → `action_tx` → `App::drain_and_dispatch_actions()` → component `update(&action)` → internal state mutation → `draw()`.
2. **Outbound (submit)**: UI form → `Action::*Submit(params)` → `App` forwards to `poll_tx` as `PollCommand` → poll task validates → `simulate_tx` → `sign` → `submit` → `Action::*SubmitOk/Err` → UI displays result.
3. **Refresh triggers**: User keystroke → `Action::Refresh*` → `App` sends `PollCommand` via `poll_tx` → poll task executes RPC → dispatches `Action::Xrpl*`.

## How to Add/Modify Data Structures

### New XRPL row type
1. Add struct to `src/xrpl/types.rs` (derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`)
2. Add `Action::Xrpl*` variant in `src/action.rs`
3. Add RPC call in `src/xrpl/client.rs` if needed
4. Add dispatch in `poll.rs:poll_batch()` or WS handler
5. Add `update()` case in receiving component(s)

### New Action variant
1. Add to `src/action.rs` enum (maintain alphabetic-ish order)
2. If XRPL data: update `poll.rs` or `ws.rs` to emit it
3. If navigation: handle in `app.rs:drain_and_dispatch_actions()`
4. If UI: handle in appropriate component `update()`
5. If keybindable: add to `config.json5` keybinding defaults

### Config key
1. Add field to `LedgerConfig` or appropriate struct in `src/config.rs`
2. Add `#[serde(default = "...")]` or `#[serde(default)]`
3. Add to `config.json5` for built-in default (if new key)
4. Verify merge precedence: CLI > env > file > built-in
5. Update every `docs/` file mentioning that config key

## How to Add Side Effects Safely

### New RPC call
- Add method to `RpcClient` in `src/xrpl/client.rs`
- Wrap with `tokio::time::timeout(RPC_TIMEOUT, ...)` (invariant I-11)
- Handle `not found` → empty result (invariant I-7)
- Handle JSON parse errors gracefully (return `Result`)

### New file I/O
- Use `config.resolved_config_dir()` or `config.resolved_data_dir()` for paths
- Never hardcode `~/.config` — use `directories::ProjectDirs`

### New environment variable
- Add constant to `src/config.rs`
- Document in `docs/tech.md` §4 and `README.md`
- Consider `XRPL_` prefix convention

## How to Add Tests

1. **Unit tests**: Add `#[cfg(test)] mod tests {}` in the same file
2. **Integration tests**: Add to `tests/` directory (if applicable)
3. **Use existing helpers**: `TestEnvGuard`, `env_lock()`, `minimal_config_toml()`
4. **TC-ID pattern**: Add `/// TC-NNN` doc comment to each test function
5. **Run**: `cargo test` — see `docs/test.md` for full policy

## How to Avoid Architecture Drift

- **Never** import `xrpl/client` or `xrpl/poll` from `components/`
- **Never** call RPC from a component — always use `Action` + `PollCommand`
- **Never** mutate `ArcValue` — treat as read-only
- **Never** sign a transaction without simulation first (invariant I-3)
- **Never** add mainnet write paths without `--yes` guard (invariant I-2)
- **Never** use `config.xrpl.signing.seed` (cleared after `Config::new()`) — always use `secret_seed`
- **Never** increase `Config` coupling beyond current imports
- **Never** remove features just to bypass compile errors
- **Never** mix unrelated refactors/renames into the same change

## Development workflow

- Follow Rust 2024 conventions; channel is in `rust-toolchain.toml` (`stable`), MSRV in `Cargo.toml`.
- Commit `Cargo.lock` and use `cargo … --locked` in CI.
- Use Conventional Commits (e.g. `fix(xrpl): avoid tokio spawn lifetime issue`).
- Follow GitHub Flow: branch from `main`, merge via PR.
- Do not run destructive git operations (e.g. `reset --hard`) without explicit agreement.
- Project pointers: directory layout in `docs/directory.md`; install/distribution in `README.md`; troubleshooting in `docs/problems.md`.

## Documentation Update Checklist

- [ ] `src/config.rs` key changed? → Update all `docs/` references
- [ ] New `Action` variant? → Consider `docs/agent/ARCHITECTURE.md` data flow update
- [ ] New TX type parser? → Update `docs/agent/DESIGN_ISSUES.md` note
- [ ] New invariant discovered? → Add to `docs/agent/INVARIANTS.md`
- [ ] New risk identified? → Add to `docs/agent/RISK_REGISTER.md`
- [ ] Behavior change? → Update `docs/design.md` if architectural
- [ ] CLI change? → Update `docs/README.md` and inline CLI help

## Final Verification Checklist

- [ ] `cargo check` passes
- [ ] `cargo fmt` passes
- [ ] `cargo test` passes (relevant tests)
- [ ] No invariant violations (check `INVARIANTS.md`)
- [ ] Mainnet guard intact for any new write paths
- [ ] RPC timeout on all new network calls
- [ ] Config merge precedence not broken
- [ ] No `components/` → `xrpl/client` imports
- [ ] Related `docs/` updated
