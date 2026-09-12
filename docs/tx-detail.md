# tx-detail.md — TX detail parser pipeline

**Behavioral contract** for the unified transaction-detail overlay. **Overlay keys and scroll UX:** [`DESIGN.md`](../DESIGN.md) § TX detail overlay.

| Concern | SSOT |
|---------|------|
| Parser registry, fallback rules | This file + `src/components/shared/tx_detail/` |
| Open/close keys, scroll while open | [`DESIGN.md`](../DESIGN.md) |
| `Action::TxDetailToggle` wiring | [`architecture.md`](architecture.md) §5.2, `src/app.rs` |

## Entry points

| Symbol | File | Role |
|--------|------|------|
| `TxDetailState` | `tx_detail/mod.rs` | Overlay visibility, scroll offset, line cache |
| `render_tx_detail` | `tx_detail/mod.rs` | Draw centered popup |
| `typed_detail_lines` | `tx_detail/parsers.rs` | Dispatch by `TransactionType` |
| `detail_lines_for` | `tx_detail/mod.rs` | Fallback for unknown fields |
| `TX_DETAIL_PARSERS` | `tx_detail/parsers.rs` | Static registry (29 types) |

Any table row carrying `raw_json: ArcValue` (tx + meta) can open the overlay via `TxDetailToggle` when focused.

## Pipeline

1. **Header** — `Result` always; `hash` / `ledger` / `date` when present in JSON.
2. **Typed branch** — `typed_detail_lines` looks up `TransactionType` in `TX_DETAIL_PARSERS`; parser returns formatted `Line` list or `None`.
3. **Fallback** — `detail_lines_for` walks known field names; unlisted keys via `format_value`. `Amount2` is known for AMM typed sections (not duplicated under “Other fields”).
4. **Cache** — First open per TX builds `cached_lines`; redraw reuses cache; scroll only moves paragraph offset. New TX invalidates cache.

## Overlay guard (`TxDetailState::handle_action`)

- While **closed**: panels handle `TxDetailToggle` with pre-resolved `(tx, meta)` from selected row.
- While **open**: consumes `Quit`, data refresh, `Tick`, and navigation actions so background tables do not change selection under the popup.
- `SelectNext` / `FocusNext` (and prev) scroll detail text with saturating arithmetic.

## Registered types (29)

`Payment`, `AccountSet`, `TrustSet`, `OfferCreate`, `OfferCancel`, `NFTokenMint`, `NFTokenBurn`, `NFTokenCreateOffer`, `NFTokenAcceptOffer`, `NFTokenCancelOffer`, `CheckCreate`, `CheckCash`, `CheckCancel`, `SignerListSet`, `SetRegularKey`, `DepositPreauth`, `EscrowCreate`, `EscrowFinish`, `EscrowCancel`, `PaymentChannelCreate`, `PaymentChannelFund`, `PaymentChannelClaim`, `AMMCreate`, `AMMDeposit`, `AMMWithdraw`, `AMMVote`, `AMMBid`, `AMMDelete`, `TicketCreate`.

Six AMM variants share `amm_detail_lines` (ordered fields, explicit `null`, `TradingFee` formatting on create/vote).

## Adding a parser

1. Implement `fn foo_detail_lines(tx: &Value) -> Option<Vec<Line>>` in `parsers.rs` (prefer `typed_detail` helper).
2. Add one row to `TX_DETAIL_PARSERS`.
3. Add `/// TC-xxx` test in `parsers.rs` or `registry_tests` when behavior is user-visible or regression-prone.
4. Update this file’s type list if the registry grows.

No `DESIGN.md` change unless overlay interaction changes.

## Tests

- **TC-094** — registry has no duplicates; required types dispatch.
- Panel integration — tables open overlay on `Enter` (see `docs/test.md` wallet/history rows).
