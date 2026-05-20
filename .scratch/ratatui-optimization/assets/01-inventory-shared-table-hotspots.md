# Inventory: shared-table duplication & draw hotspots

Fact sheet for later tickets (03 shared contract / 04 dirty-render / 07 panel split). No production edits.

## File sizes (orientation)

| Lines | File |
|------:|------|
| 1417 | `panels/wallet.rs` |
| 788 | `panels/server.rs` |
| 561 | `shared/tx_detail/mod.rs` |
| 314 | `panels/tx_history.rs` |
| 242 | `tui.rs` |
| 199 | `panels/book.rs` |
| 185 | `panels/ledger_objects.rs` |
| 182 | `shared/widgets.rs` |
| 60 | `shared/selectable_table.rs` |

---

## 1. SelectableTable + Scrollbar + header/block copies

### Already shared (gold pattern)

- `shared/selectable_table.rs` — `SelectableTableState` (`TableState` + `ScrollbarState`, select/clamp)
- `shared/widgets.rs` `render_tx_scroll_table` (~41–77) — header + rows + highlight + horizontal `Fill|Length(1)` + vertical Scrollbar
- Consumers: `tx_history` (and any path that calls the helper)

### Near-identical copies of the gold pattern

Same shape in each: `titled_block(_with_count)` → `header_row_style` → `Table::new` → `selected_row_style` + `"▶ "` → `Layout::horizontal([Fill, Length(1)])` → `render_stateful_widget(table)` + `Scrollbar::VerticalRight`.

| Site | State field | Header | Scrollbar thumb |
|------|-------------|--------|-----------------|
| `panels/book.rs` ~104–152 | `table_state` | Quality/Price/TakerGets/TakerPays | `accent_style` |
| `panels/trust_lines.rs` ~107–155 | `table_state` | Currency/Issuer/Balance/Limit | `accent` |
| `panels/ledger_objects.rs` ~140–181 | `table_state` | Type/Object index/Detail | `accent` |
| `panels/path_find.rs` ~171–221 | `table_state` | #/You send/Hops/Route | `accent` |
| `panels/server.rs` ~664–760 (dUNL) | `dunl_table` | #/Domain·key/Seq/M/Signing | `accent` |
| `tabs/nft.rs` ~101–156 | `table_state` | NFTokenID/dNFT/Taxon/Serial/Fee/URI | `accent` |
| `shared/widgets.rs` `render_tx_scroll_table` | arg | Hash/Dir/Type/Ledger/Result | **`secondary_style`** |

**Drift to note for contract ticket:** thumb style is `accent` everywhere except the shared tx helper (`secondary`). Detail overlays also use `secondary` (`tx_detail/mod.rs` ~110, `server` validator detail ~318).

### Tables without `SelectableTableState` / scrollbar

- `panels/oracle.rs` — `render_widget(Table)` only; custom `Block::bordered` title styling (not `titled_block`)
- `panels/flare_ftso.rs` — same; builds rows in `render_content`, not selection

### Block helpers

- `theme::panel_block` — canonical border/title colors
- `widgets::titled_block` — thin alias
- `widgets::titled_block_with_count` — re-implements border/title styles instead of composing `panel_block` (style duplication)
- Direct `theme::panel_block`: wallet composer/keygen (~558, ~737), help_overlay, splash

---

## 2. Per-frame `Row` / `String` rebuild in `draw()`

All selectable table panels rebuild `Row`s every `draw` (no row cache). Notable allocs:

| Panel | Hot pattern in `draw` |
|-------|------------------------|
| `ledger_objects` | `r.index.chars().take(20).collect::<String>()` (+ detail take 64) every row |
| `trust_lines` | `l.account.chars().take(12).collect` + balance-style `Row` |
| `book` | `o.quality/price/... .clone()` into `Row` |
| `path_find` | `path_find_table_row` per path every frame |
| `nft` | truncate via `chars().count` + `format!("{}…")` / `to_string` per NFT |
| `server` dUNL | `validator_row_label` (width-dependent) + `to_string` seq + hex shorten per validator |
| `tx_history` | via shared helper → `tx_table_row` still `format!` / style branch every row |
| `oracle` / `flare_ftso` | clone price fields / `timestamp.to_string()` |

Non-table but every-frame string work:

- `wallet.draw` (~1035–1183): rebuilds `flag_labels` + summary `Line`s (`format!` balance/seq/owner, domain decode)
- `server.draw` metrics (~540–640): rebuilds ledger/host/fee/`dunl_exp` strings + `Line`s every frame
- Loading paths: `render_loading` on Tick-driven spinner (book/trust/ledger/path/server/wallet)

**Implication for perf ticket:** shared helper alone does not remove allocs; dirty-render reduces how often `draw` runs. Row materialization cache is a separate lever if FPS still hurts after dirty frames.

---

## 3. Logical block boundaries (split candidates)

### `wallet.rs` (~1417)

| Approx lines | Block | Split hint |
|-------------|-------|------------|
| 1–123 | Types: flag choice labels, `SubmitFlash`, composer field enums, `WalletPanel` fields | `wallet/types.rs` or keep |
| 124–382 | Open composers, validate/preview/submit payment & AccountSet, flag/domain helpers | `wallet/submit.rs` / `composer_logic.rs` |
| 383–521 | Modal key handlers (`account_set_edit_keys`, `payment_edit_keys`, modal dispatch) | `wallet/keys.rs` |
| 522–724 | `render_composer` (AccountSet + Payment UI) | `wallet/composer_view.rs` |
| 725–778 | `render_keygen_popup` | with composer_view or small popup mod |
| 779–1034 | `Component`: config / update / handle_key (non-modal) | stay as façade |
| 1035–1183 | `draw` account summary + flash + overlay hooks | façade `draw` |
| 1184–end | unit tests (payment validate/preview/submit) | `#[cfg(test)]` stay or `wallet/tests` |

Natural seams: **composer logic+view**, **keygen popup**, **summary draw**, keep `WalletPanel` as Component shell.

### `server.rs` (~788)

| Approx lines | Block | Split hint |
|-------------|-------|------------|
| 33–325 | `ValidatorDetail`, row label, detail lines, `render_validator_detail` (+ own Scrollbar) | `server/validator_detail.rs` |
| 326–349 | `dunl_expiry_tag`, `quorum_match_tag` | with dUNL or `server/dunl_fmt.rs` |
| 351–385 | `ServerPanel` struct / `new` / fee history | façade |
| 386–516 | `Component::update` (ServerInfo / Dunl / Tick / keys) | façade |
| 517–~640 | `draw` metrics paragraph (URL/ledger/host/fee/quorum/Node UNL) | `server/metrics_view.rs` or inline |
| ~640–760 | dUNL titled table + scrollbar (same shared-table shape) | candidate first consumer of shared helper |
| ~760–788 | fee sparkline footer | small helper |

Natural seams: **validator detail overlay**, **dUNL table**, **metrics header**, keep `ServerPanel` as shell.

---

## Takeaways for blocked tickets

1. **03 Shared selectable-table contract** — extract the duplicated Table+Scrollbar block; decide thumb style (`accent` vs `secondary`); decide whether oracle/FTSO join (selection?) or stay stateless.
2. **04 Dirty-render** — highest leverage vs constant `Event::Render`; row-cache is optional follow-on after measuring.
3. **07 Wallet/server split** — boundaries above are concrete; dUNL table may move to shared helper *before* file split.
