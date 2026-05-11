# TUI integration (ratatui + tokio)

**Load contract**: If you change poll or WebSocket behavior, read **[client-patterns.md](client-patterns.md) in full** first (timeouts, traits, backoff skeleton). Then read this file end-to-end.

**Layout**: main (ratatui) + **poll task** (bundle JSON-RPC) + **WS task** (subscribe). Share state via **`UnboundedSender<Action>`** (`Action` carries `Tick`, XRPL results, `XrplError`, etc.).

## lazyxrp (this repo)

Use these paths when wiring or debugging poll/WS/UI — traits and timeouts still follow **`client-patterns.md`** end-to-end.

| Concern | File / symbol |
|---------|----------------|
| **`Action`** variants (`XrplServerInfo`, `XrplError`, refreshes, submit results) | `src/action.rs` — `pub enum Action` |
| **Poll loop**, `poll_batch`, `join!` + timeouts, not-found → empty `Vec` | `src/xrpl/mod.rs` — `start_poll_task`, `run_poll_loop`, `poll_batch`, `poll_wallet_overview`, `PollContext` |
| **WS backoff**, subscribe, ledger close → **`poll_trigger` nudge** | `src/xrpl/mod.rs` — `start_ws_task`, `run_ws_loop`, `connect_and_subscribe` |
| **Spawn**, `CancellationToken`, `PollCommand` `mpsc`, connect RPC/WS URLs | `src/app.rs` — `App::run` (creates `cancel`, calls `start_ws_task` / `start_poll_task`, `cancel.cancel()` on quit) |
| **Global TUI lifecycle** hook (separate helper for async workers) | `src/tui.rs` — owns/cancels ancillary tasks where applicable |

**Panel wiring**: `src/components/` — handlers register via `register_action_handler` from `App::run`; each panel **`update`/draw** reacts to **`Action`** (not XRPL traits directly).

## Action

Tasks send variants to the UI; panels `update` on `Action` and mutate state.

## Poll

`tokio::select!` on `cancel.cancelled_owned()`, interval, and `refresh`. Batch with `tokio::time::timeout` + `join!`.  
RPC init failure often **sends `XrplError` once then returns**.

Match `Result<Result<_,_>, Elapsed>` in stages (success / XRPL / timeout).

## WebSocket

Outer: **backoff reconnect loop**. Inner: `connect` → `Subscribe` → `xrpl_receive` loop.  
Subscribe API details: [client-patterns.md](client-patterns.md).

## Component

- First load: separate loading vs empty with **`received: bool`** (or equivalent).
- Empty data → **“none”** (empty `Vec` after not-found normalization).

```rust
pub trait Component {
    fn update(&mut self, action: Action) -> color_eyre::Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()>;
}
```

## Shutdown

Clone `CancellationToken` into tasks; call `cancel()` on exit. Loops exit on `cancelled_owned()`.
