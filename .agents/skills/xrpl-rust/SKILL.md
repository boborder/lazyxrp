---
name: xrpl-rust
description: "XRPL in Rust (xrpl crate): XRPLAsyncClient (RPC request) XRPLAsyncWebsocketIO (WS send/receive) AsyncJsonRpcClient AsyncWebSocketClient, RPC vs WS, tokio timeouts, 429/backoff, not-found→empty Vec, tec/tef/tem, Currency vs CurrencyAmount, Cow/signing. Triggers: 429, WS reconnect, lazyxrp panels, book_offers, XRPL errors in Rust."
---

# xrpl-rust Skill

**Target version**: This repo’s `Cargo.toml` pins `xrpl-rust = "1.1"`. For `Subscribe::new` and similar, **match argument arity/order to that crate version** (other versions: check rustdoc / source).

**Version bump (before merging a crate upgrade)** — quick gate: (1) `cargo check` on this repo; (2) grep `Subscribe::new` / `Submit::new` / request constructors for arity changes; (3) skim `xrpl` changelog or diff for renamed fields on types you touch; (4) run the app against testnet and hit one RPC path + one WS subscribe path; (5) if compile-only, still verify `CommonFields` / amount types did not move in `Payment`.

## When to use this skill

- Writing or fixing Rust that uses the `xrpl` crate
- Choosing between JSON-RPC and WebSocket
- Wiring XRPL into ratatui
- Handling XRPL-specific errors (`actNotFound`, `tec*`, etc.)

## How to read references (full pass)

**Reference load contract**

- **MANDATORY**: If you open a file under `references/`, read it **from top to bottom** for that task. These files are short on purpose.
- **NEVER**: Satisfy the task using only a **line-range excerpt** or “middle chunk” of a reference, or **paraphrase without having read the whole file** when that reference is in scope. That mirrors evaluator “range limits” failure: you miss cross-links (`submit` RPC-only, trait mixups at the end of another section, etc.).

| Focus | Read end-to-end first | Skip for now if… |
|------|----------------------|------------------|
| RPC/WS, timeout, join | [references/client-patterns.md](references/client-patterns.md) | Pure headless RPC with no signing yet → [transactions.md](references/transactions.md) can wait until you submit a tx |
| Sign, fee, sequence, submit | [references/transactions.md](references/transactions.md) + client-patterns | No TUI → [tui-integration.md](references/tui-integration.md) optional |
| Poll / WS / `Action` / shutdown | [tui-integration.md](references/tui-integration.md) + client-patterns | Headless only → skip TUI ref |
| Cow lifetimes, Currency mixups, empty vs Err, 429 | [references/troubleshooting.md](references/troubleshooting.md) | — |

## NEVER (if you skip references, at least follow these)

1. **Do not mix** RPC `.request` on `AsyncWebSocketClient` **or** WS `xrpl_send` / `xrpl_receive` on `AsyncJsonRpcClient` — traits differ (`XRPLAsyncClient` vs `XRPLAsyncWebsocketIO`); you **fail at compile time or at runtime**.
2. **Do not return not-found as `Err` in list UIs** — normalize `actNotFound` etc. to an **empty `Vec`** (“none” display). Only propagate real failures.
3. **Do not use `Cow::Borrowed` with short-lived strings** — if the request outlives them you hit **lifetime errors like #100013**. Dynamic data → `Cow::Owned`.
4. **Do not confuse `Currency` and `CurrencyAmount`** — `book_offers` uses the former; amount-bearing `Payment` uses the latter. **Typical compile / tx failure**.
5. **Do not hardcode seeds or log them** — use env + `secrecy` (see transactions ref). **Key leak risk**.

## One-page: RPC vs WS

| Use case | Prefer | Why |
|----------|--------|-----|
| One-shot / periodic poll | JSON-RPC | Simpler connection state; easy timeouts |
| Subscriptions, ledger/tx push | WebSocket | Lower latency; `subscribe` |
| **submit** | **JSON-RPC only** | API lives on RPC side |

`AsyncWebSocketClient` implements **`XRPLAsyncWebsocketIO`**. It is a **different type and trait** from the RPC client.

## Default decisions

- **errors**: `color-eyre` + string match to classify not-found.
- **empty**: “no data” → **`Vec::new()`**; many XRPL “missing” cases are **empty, not `Err`**, so the UI stays consistent.
- **Currency**: dynamic → `Cow::Owned`; XRP → `Currency::XRP(XRP::new())` (details in troubleshooting).

## Minimal shell patterns (details in references)

Keep on the SKILL only **timeout + join result shape** (each slot `Result<Result<T, E>, Elapsed>`) and **not-found normalization**. Connection code, subscriptions, and panel wiring live **only in references**.

```rust
// Example: parallel RPC (full wiring in client-patterns)
let (a, b) = tokio::join!(
    tokio::time::timeout(RPC_TIMEOUT, rpc.account_info(&addr)),
    tokio::time::timeout(RPC_TIMEOUT, rpc.book_offers(gets, pays, limit)),
);

fn is_not_found_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("actnotfound")
        || lower.contains("entrynotfound")
        || lower.contains("lgrnotfound")
        || lower.contains("object not found")
        || lower.contains("account not found")
}
```

## Progressive disclosure

Each link is one file — **read the whole file** when you follow it (see Reference load contract).

- [references/client-patterns.md](references/client-patterns.md) — connect, `.request` / `xrpl_send`, join, **429 / WS backoff skeleton**, reconnect
- [references/transactions.md](references/transactions.md) — Payment, sign, submit, engine result
- [references/tui-integration.md](references/tui-integration.md) — `Action`, poll, WS task, `Component`, cancel
- [references/troubleshooting.md](references/troubleshooting.md) — Cow, Currency, 429 policy, empty vs error
