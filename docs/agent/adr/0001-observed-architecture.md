# ADR 0001: Observed Current Architecture

- **Status**: Accepted as observed baseline
- **Date**: 2026-05-14
- **Deciders**: Codebase reconstruction (Pass 9)

## Context

lazyxrp is a Rust TUI application for the XRP Ledger. The architecture was not formally documented as a decision record. This ADR captures the observed architecture as-is to serve as a baseline for future architectural decisions.

## Decision

The codebase follows **Elm-like TEA (The Elm Architecture)** with unidirectional message flow, hand-rolled on `ratatui` + `tokio::mpsc` channels. No formal framework — the `Action` enum is the universal message type, `Component` is the UI abstraction, and `App` is the orchestrator.

### Key architectural choices observed:

1. **Single binary, dual mode**: TUI watch mode and CLI subcommands share the same binary, sharing `xrpl/` integration layer but using separate entry paths (`App::run()` vs `execute_cli_command()`).

2. **Channel-based messaging**: Three `mpsc::unbounded` channels (`action`, `poll`, `poll_trigger`) decouple background I/O from UI rendering. CancellationToken for graceful shutdown.

3. **Component trait**: All UI elements implement a common `Component` trait with `update(&Action)` and `draw()` methods. Tabs compose panels; panels are standalone.

4. **Config merge chain**: `Config` resolves values through: CLI flags → env vars → user `config.toml` → built-in `config.json5`. Not a formal layered config — resolution is split across `main.rs` and `config.rs`.

5. **Simulate-then-sign**: Transaction submits always go through `simulate_tx` first to auto-fill Fee/Sequence, then sign, then `submit`. This avoids manual fee estimation errors.

6. **Seed security**: Signing seeds use `secrecy::SecretString` (zero-on-drop). Plaintext seeds in `config.toml` are cleared after loading. `XRPL_SEED` env var is removed after reading.

## Evidence

- **Source files**: `src/main.rs`, `src/app.rs`, `src/tui.rs`, `src/config.rs`, `src/xrpl/poll.rs`, `src/components/mod.rs`, `src/action.rs`
- **Documentation**: `docs/design.md`, `docs/architecture/c4-containers.md`, `docs/directory.md`
- **Graph**: `graphify-out/GRAPH_REPORT.md` (built from commit `0cd3f568`)
- **Tests**: `src/main.rs` network resolution tests, `src/app.rs` action routing tests, `src/config.rs` merge tests

## Consequences

### Positive
- **Clear message routing**: All state changes flow through `Action`, making behavior easy to trace.
- **Loose coupling**: Components don't know about network I/O; background tasks don't know about UI.
- **Testable**: `App::process_actions()` and individual components can be tested with synthetic `Action` messages.
- **Graceful shutdown**: `CancellationToken` + `Tui::drop()` ensure terminal cleanup even on errors.

### Negative
- **Message bloat**: 70+ `Action` variants in a single enum. Adding features means touching the hub.
- **Unbounded channels**: Memory can grow under load (no backpressure).
- **Split config resolution**: Config merge logic spans two files (`config.rs` + `main.rs`), increasing cognitive load.
- **Submit duplication**: 5 submit functions in `poll.rs` with near-identical structure.

### Neutral (design tradeoffs)
- **Hand-rolled TEA**: No framework means full control but also more boilerplate than using something like `ratatui`'s built-in state management.
- **ArcValue sharing**: Avoids deep clones but relies on convention for immutability.

## Known Issues

See `docs/agent/DESIGN_ISSUES.md` for 9 cataloged issues and `docs/agent/RISK_REGISTER.md` for 10 ranked risks.

## Follow-up Actions

1. **R-006** (Critical): Centralize mainnet write guard — move `--yes` validation to `PollContext`.
2. **R-001** (High): Centralize seed resolution into single function.
3. **Issue 1** (Medium): Extract common submit pipeline from poll.rs.
4. **Issue 3** (Low): Add `debug_assert_eq!` for tab-panel consistency.
5. Consider formal ADRs for any future architectural changes (e.g., moving to bounded channels, splitting config.rs).
