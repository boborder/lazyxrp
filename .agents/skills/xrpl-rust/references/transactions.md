# Transactions (xrpl-rust)

**Load contract**: If you touch signing, fees, or `submit`, read this file **entirely** once — **`submit` is JSON-RPC only** appears here, not in client-patterns’ first page.

**Path**: After signing, **`submit` is JSON-RPC only** (do not send via WS). Track fee / sequence / `last_ledger_sequence` on `CommonFields`.

## Build

```rust
use xrpl::models::transactions::{payment::Payment, CommonFields, TransactionType};

let payment = Payment::new(
    CommonFields {
        account: "r…".into(),
        transaction_type: TransactionType::Payment,
        fee: Some("12".into()),
        sequence: Some(42),
        last_ledger_sequence: Some(ledger_index + 20),
        ..Default::default()
    },
    Some("r…".into()),
    Some("1000000".into()), // drops
    None, None, None, None,
);
```

Issued-currency amounts use `CurrencyAmount::IssuedCurrencyAmount(...)` (don’t mix with plain `Currency` → [troubleshooting.md](troubleshooting.md)).

## Sign

```rust
use xrpl::transaction::sign;
let signed = sign(&payment, &seed, false)?; // adjust bool to your case
```

Load seed from **environment + `secrecy::SecretString`**. No hardcoding.

## submit and engine result

```rust
use xrpl::models::requests::submit::Submit;
let req = Submit::new(None, signed.into(), None);
let value = serde_json::to_value(rpc.request(req.into()).await?)?;
```

| Prefix | Meaning | Rule of thumb |
|--------|---------|---------------|
| `tes*` | Applied-ish | Treat `tesSUCCESS` as success |
| `tec*` | Charged but failed | Inspect reason code |
| `tef*` / `tem*` / `tel*` | Format / local / layer | Rebuild tx |

Also check finality via `validated` etc. (trace `result` in JSON).

## fee and sequence

- Use fee RPC `open_ledger_fee` vs server-default as you choose.
- Next sequence from `account_info` `Sequence`; bursts **increment one at a time**.
- `last_ledger_sequence` caps the validity window.
