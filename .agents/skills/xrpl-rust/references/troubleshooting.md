# Troubleshooting (xrpl-rust)

**Load contract**: Open this file → read it **whole**. Cow / Currency / empty-vs-Err interact; partial reads cause the wrong fix.

## Cow / lifetimes (#100013 family)

Do not put dynamic strings into requests as **`Cow::Borrowed`** (too short-lived → **compile error**).

```rust
// Bad: Cow::Borrowed(ephemeral)
// Good:
Cow::Owned(addr.to_string())
```

When you need `Currency<'static>`, **close the lifetime at the issuer** with `Cow::Owned` and return **`Currency<'static>`**.

## Currency vs CurrencyAmount

| Type | Typical use |
|------|-------------|
| `Currency` | `book_offers` taker_gets / taker_pays |
| `CurrencyAmount` | `Payment` and other **amount-bearing** fields |

Swapping them → **API mismatch or unintended tx**.

## Empty vs Err

XRPL may return error strings for missing accounts, etc. **In list UIs**, if `is_not_found_error` → **`Vec::new()`**.

```rust
match rpc.account_tx(&addr, n).await {
    Ok(v) => v,
    Err(e) if is_not_found_error(&e.to_string()) => vec![],
    Err(e) => return Err(e.into()),
}
```

Apply the same policy to `account_nfts`, `account_lines`, `book_offers`, etc.

## not-found strings (case-insensitive)

Match substrings: `actnotfound`, `entrynotfound`, `lgrnotfound`, `object not found`, `account not found` (keep aligned with SKILL `is_not_found_error`).

## WebSocket

When the connection drops: **reconnect + backoff** (`Ok(None)` / `Err`). If UI is stuck “loading forever”, **check whether the WS loop died**.

## 429 / rate limits

- Space polls (e.g. **≥ 5s** rule of thumb)
- Batch with `tokio::join!`
- **Copy-paste backoff + logging**: the section *429 / disconnect: exponential backoff skeleton* in [client-patterns.md](client-patterns.md) — outer loop + **warn** lines with delay + error text for ops.
- Production: consider your own `rippled`
