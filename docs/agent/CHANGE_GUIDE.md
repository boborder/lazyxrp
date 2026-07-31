[CHANGE_GUIDE.md#8902]
1:# Change Guide
2:
3:> **Read**: `ARCHITECTURE.md`, `DATA_MODEL.md`, `INVARIANTS.md`, `DEPENDENCY_RULES.md`. **Scope**: full repository. **Generated**: 2026-05-14 (Pass 7).
4:
5:## Before Coding
6:
7:1. **Identify change type**: Is it a new feature, bug fix, refactor, or config change?
8:2. **Check invariants**: Read `docs/agent/INVARIANTS.md` — will your change violate any invariant?
9:3. **Check risks**: Read `docs/agent/RISK_REGISTER.md` — does your change touch a high-risk area?
10:4. **Find tests**: Check `docs/test.md` for existing TC-IDs covering the area.
11:
12:## How to Locate Affected Slice/Module
13:
14:| Change type | Start here |
15:|-------------|------------|
16:| New CLI subcommand | `src/cli.rs` (Cmd enum) → `src/xrpl/cli_exec.rs` (handler) |
| New short binary (e.g. `rp`) | `src/bin/<name>.rs` + `[[bin]]` in `Cargo.toml` → shared logic in `src/lib.rs` / `cli_exec.rs` → CD tarball + `install.sh` |
17:| New TUI panel | `src/components/panels/` (new panel) → `src/components/tabs/` (compose into tab) → `src/app.rs` (add to panels vec) |
18:| New XRPL data source | `src/xrpl/client.rs` (RPC call) → `src/xrpl/types.rs` (row struct) → `src/action.rs` (new Action variant) → polling/WS to emit → component to consume |
19:| New transaction submit type | `src/xrpl/types.rs` (params struct) → `src/action.rs` (Submit/Ok/Err variants) → `src/xrpl/poll.rs` (submit function) → `src/components/panels/wallet.rs` (UI form) |
20:| Config key change | `src/config.rs` (LedgerConfig) → `config.json5` (defaults) → `main.rs` (env/CLI overrides) |
21:| Keybinding change | `config.json5` (add binding) or `src/config.rs` (parse new Action) |
22:| New TX detail parser | `src/components/shared/tx_detail/parsers.rs` (new parser) → `mod.rs` (add to `detail_lines_for()` dispatch) |
23:| Style/theme change | `src/components/shared/theme.rs` (colors/styles) |
24:
25:## How to Trace Data Flow
26:
27:1. **Inbound data**: WS or RPC response → `Action::Xrpl*` variant → `action_tx` → `App::drain_and_dispatch_actions()` → component `update(&action)` → internal state mutation → `draw()`.
28:2. **Outbound (submit)**: UI form → `Action::*Submit(params)` → `App` forwards to `poll_tx` as `PollCommand` → poll task validates → `simulate_tx` → `sign` → `submit` → `Action::*SubmitOk/Err` → UI displays result.
29:3. **Refresh triggers**: User keystroke → `Action::Refresh*` → `App` sends `PollCommand` via `poll_tx` → poll task executes RPC → dispatches `Action::Xrpl*`.
30:
31:## How to Add/Modify Data Structures
32:
33:### New XRPL row type
34:1. Add struct to `src/xrpl/types.rs` (derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`)
35:2. Add `Action::Xrpl*` variant in `src/action.rs`
36:3. Add RPC call in `src/xrpl/client.rs` if needed
37:4. Add dispatch in `poll.rs:poll_batch()` or WS handler
38:5. Add `update()` case in receiving component(s)
39:
40:### New Action variant
41:1. Add to `src/action.rs` enum (maintain alphabetic-ish order)
42:2. If XRPL data: update `poll.rs` or `ws.rs` to emit it
43:3. If navigation: handle in `app.rs:drain_and_dispatch_actions()`
44:4. If UI: handle in appropriate component `update()`
45:5. If keybindable: add to `config.json5` keybinding defaults
46:
47:### Config key
48:1. Add field to `LedgerConfig` or appropriate struct in `src/config.rs`
49:2. Add `#[serde(default = "...")]` or `#[serde(default)]`
50:3. Add to `config.json5` for built-in default (if new key)
51:4. Verify merge precedence: CLI > env > file > built-in
52:5. Update every `docs/` file mentioning that config key
53:
54:## How to Add Side Effects Safely
55:
56:### New RPC call
57:- Add method to `RpcClient` in `src/xrpl/client.rs`
58:- Wrap with `tokio::time::timeout(RPC_TIMEOUT, ...)` (invariant I-11)
59:- Handle `not found` → empty result (invariant I-7)
60:- Handle JSON parse errors gracefully (return `Result`)
61:
62:### New file I/O
63:- Use `config.resolved_config_dir()` or `config.resolved_data_dir()` for paths
64:- Never hardcode `~/.config` — use `directories::ProjectDirs`
65:
66:### New environment variable
67:- Add constant to `src/config.rs`
68:- Document in `docs/tech.md` §4 and `README.md`
69:- Consider `XRPL_` prefix convention
70:
71:## How to Add Tests
72:
73:1. **Unit tests**: Add `#[cfg(test)] mod tests {}` in the same file
74:2. **Integration tests**: Add to `tests/` directory (if applicable)
75:3. **Use existing helpers**: `TestEnvGuard`, `env_lock()`, `minimal_config_toml()`
76:4. **TC-ID pattern**: Add `/// TC-NNN` doc comment to each test function
77:5. **Run**: `cargo test` — see `docs/test.md` for full policy
78:
79:## How to Avoid Architecture Drift
80:
81:- **Never** import `xrpl/client` or `xrpl/poll` from `components/`
82:- **Never** call RPC from a component — always use `Action` + `PollCommand`
83:- **Never** mutate `ArcValue` — treat as read-only
84:- **Never** sign a transaction without simulation first (invariant I-3)
85:- **Never** add mainnet write paths without `--yes` guard (invariant I-2)
86:- **Never** use `config.xrpl.signing.seed` (cleared after `Config::new()`) — always use `secret_seed`
87:- **Never** increase `Config` coupling beyond current imports
88:- **Never** remove features just to bypass compile errors
89:- **Never** mix unrelated refactors/renames into the same change
90:
91:## Development workflow
92:
93:- Follow Rust 2024 conventions; channel is in `rust-toolchain.toml` (`stable`), MSRV in `Cargo.toml`.
94:- Commit `Cargo.lock` and use `cargo … --locked` in CI.
95:- Use Conventional Commits (e.g. `fix(xrpl): avoid tokio spawn lifetime issue`).
96:- Follow GitHub Flow: branch from `main`, merge via PR.
97:- Do not run destructive git operations (e.g. `reset --hard`) without explicit agreement.
98:- Project pointers: directory layout in `docs/directory.md`; install/distribution in `README.md`; troubleshooting in `docs/problems.md`.
99:
100:## Documentation Update Checklist
101:
102:- [ ] `src/config.rs` key changed? → Update all `docs/` references
103:- [ ] New `Action` variant? → Consider `docs/agent/ARCHITECTURE.md` data flow update
104:- [ ] New TX type parser? → Update `docs/agent/DESIGN_ISSUES.md` note
105:- [ ] New invariant discovered? → Add to `docs/agent/INVARIANTS.md`
106:- [ ] New risk identified? → Add to `docs/agent/RISK_REGISTER.md`
107:- [ ] Behavior change? → Update `docs/design.md` if architectural
108:- [ ] CLI change? → Update `docs/README.md` and inline CLI help
109:
110:## Final Verification Checklist
111:
112:- [ ] `cargo check` passes
113:- [ ] `cargo fmt` passes
114:- [ ] `cargo test` passes (relevant tests)
115:- [ ] No invariant violations (check `INVARIANTS.md`)
116:- [ ] Mainnet guard intact for any new write paths
117:- [ ] RPC timeout on all new network calls
118:- [ ] Config merge precedence not broken
119:- [ ] No `components/` → `xrpl/client` imports
120:- [ ] Related `docs/` updated
121: