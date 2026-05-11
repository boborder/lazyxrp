# Client patterns (xrpl-rust)

**Load contract**: If your task uses this file, read it **entirely** in order. **Do not** implement from a middle section alone — `Subscribe` arity and trait rules appear in specific sections.

**Version bump verification** (when `xrpl-rust` changes): (1) `cargo check`; (2) fix constructor arity (`Subscribe::new`, `Submit::new`, …); (3) confirm `Cargo.toml` pin matches docs in [../SKILL.md](../SKILL.md); (4) smoke: one `.request` + one WS session; (5) re-grep `XRPLAsyncClient` / `XRPLAsyncWebsocketIO` call sites for wrong trait usage.

**Baseline**: `AsyncJsonRpcClient` → trait `XRPLAsyncClient` + `.request(...)`. `AsyncWebSocketClient` → `XRPLAsyncWebsocketIO` + `xrpl_send` / `xrpl_receive`. Do not mix them.

## Connect

```rust
use xrpl::asynch::clients::{AsyncJsonRpcClient, XRPLAsyncClient};
let rpc = AsyncJsonRpcClient::connect("https://…".parse()?)?;

use xrpl::asynch::clients::{AsyncWebSocketClient, XRPLAsyncWebsocketIO};
let mut ws = AsyncWebSocketClient::open("wss://…".parse()?).await?;
```

- RPC `connect` is synchronous (mostly URL parse). WS `open` is async handshake.
- WS needs `mut` + a receive loop; feed the main task via `mpsc`.

## `.request` and JSON

```rust
let resp = rpc.request(req.into()).await?;
let value = serde_json::to_value(resp)?;
```

Thin response types are common — **serde_json::Value then parse only what you need** is the reliable pattern in practice.

## Parallel + timeout

```rust
let (r1, r2) = tokio::join!(
    tokio::time::timeout(RPC_TIMEOUT, rpc.account_info(&addr)),
    tokio::time::timeout(RPC_TIMEOUT, rpc.book_offers(gets, pays, limit)),
);
```

Per slot: `Ok(Ok(v))` success / `Ok(Err(e))` XRPL error / `Err(Elapsed)` timeout.  
**429 and disconnect → backoff** (skeleton in the section *429 / disconnect: exponential backoff skeleton* below; policy in [troubleshooting.md](troubleshooting.md)).

## 429 / disconnect: exponential backoff skeleton

Use an **outer loop** that sleeps with increasing delay between full retries (new TCP session, new subscribe). Cap the delay so a bad config cannot stall forever. **Log** each retry at `warn` with `delay_ms` and a short error reason (ops need to see 429 vs disconnect vs parse).

```rust
use std::time::Duration;

async fn ws_outer_with_backoff() -> color_eyre::Result<()> {
    let mut delay = Duration::from_millis(250);
    let cap = Duration::from_secs(30);
    loop {
        match run_inner_ws_session().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                // use `log::warn!` / `tracing::warn!` to match your crate
                log::warn!(
                    "xrpl retry delay_ms={} err={}",
                    delay.as_millis(),
                    e
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(cap);
            }
        }
    }
}

async fn run_inner_ws_session() -> color_eyre::Result<()> {
    // open → Subscribe::new(...) → xrpl_receive loop until disconnect / fatal Err
    todo!()
}
```

Wrap your **WS session** inside `run_inner_ws_session` (or equivalent), or **restart a burst of RPC** after 429 / connection closed — **do not** spin backoff around a request that already returned `Ok`.

For **429-only** on RPC, you can retry **that request** with the same delay state before rebuilding the client, if the error exposes HTTP 429 or `"too many requests"`.

## WebSocket: subscribe → read

`Subscribe::new(...)` **arity and order follow the crate** (this repo pins `xrpl-rust = "1.1"`).

```rust
use xrpl::models::requests::subscribe::{StreamParameter, Subscribe};
let req = Subscribe::new(
    None, None, None, None,
    vec![StreamParameter::Ledger, StreamParameter::Transactions],
    None, None,
);
ws.xrpl_send(req.into()).await?;

loop {
    tokio::select! {
        _ = cancel.cancelled() => break,
        msg = ws.xrpl_receive() => { /* Ok(Some) / Ok(None) disconnect / Err */ }
    }
}
```

Wrap in **exponential-backoff reconnect** (for TUI see [tui-integration.md](tui-integration.md)).

## Parse

Thin response structs from `xrpl` are common — after `serde_json::to_value(resp)?`, drill with `Value`:

- Navigate `value.get("result")` then domain keys (`lines`, `account_nfts`, `amm`, `offers`, nested `hash`/`Account` fields, …). Use `Value::as_str`, `as_u64`, `as_array`, `.and_then`, and **`parse::<T>()` for numeric strings**.
- XRPL-RPC errors often sit under **`result`** as `error` / `error_code` / `error_message` — branch before trusting success-shaped JSON (especially `submit`; lazyxrp: `check_xrpl_error` + `parse_submit_success` on the same `Value` path).
- **`engine_result`** on submits: classify `tes`/non-`tes` separately from transport errors (`transactions.md`).

**lazyxrp (this repo) — copy/trace targets**

| Role | Location |
|------|----------|
| `Value` → row types | `src/xrpl/mod.rs`: `parse_server_info_value`, `parse_fee_value`, `parse_book_offers_value`, `parse_account_nfts_value`, `parse_submit_success`, … |
| Ledger-driven re-polls | WS path: `book_offer_best_price`, `serde_json::to_value` on WS payloads in same file |
| RPC batch → UI | `poll_batch`: `dispatch!` macro maps `Result<Result<_,_>, Elapsed>` → `Action::…` variants |

Panels consume **`Action`** (see `src/action.rs`), not raw `Value` — keep parsing in **`xrpl` integration**, not ratatui components.
