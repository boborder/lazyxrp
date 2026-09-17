---
version: alpha
name: LazyXRP
description: Dense XRPL trading terminal — royal-blue chrome, turquoise metadata, ledger-semantic green/red.
omitted:
  - section: typography
    reason: "Terminal TUI — no shipped font; hierarchy is ratatui modifiers only (bold/underlined/italic/reversed)."
  - section: spacing
    reason: "Layout is proportional area splits (%), not a px spacing scale — see Layout."
  - section: rounded
    reason: "Panels use a single ratatui BorderType::Rounded; no radius scale."
colors:
  primary: "#1E90FF"
  border: "#4169E1"
  title: "#6495ED"
  secondary: "#40E0D0"
  muted: "#778899"
  success: "#3CB371"
  error: "#DC143C"
  warning: "#FFA500"
  flag: "#64C8FF"
  highlight-fg: "#FFFFFF"
  chart-value-fg: "#0F172A"
components:
  panel-focused:
    textColor: "{colors.title}"
  panel-unfocused:
    textColor: "{colors.muted}"
  table-selected-row:
    backgroundColor: "{colors.border}"
    textColor: "{colors.highlight-fg}"
  chart-value:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.chart-value-fg}"
  flag-chip:
    textColor: "{colors.flag}"
  badge-production:
    backgroundColor: "{colors.error}"
  badge-non-production:
    backgroundColor: "{colors.warning}"
  status-online:
    backgroundColor: "{colors.success}"
  status-offline:
    backgroundColor: "{colors.error}"
  hash-text:
    textColor: "{colors.secondary}"
---
# DESIGN — TUI look & feel

What is **seen and operated**. Behavior, poll, and submit: [`docs/architecture.md`](docs/architecture.md). Guards: [`docs/security.md`](docs/security.md). Tokens: `src/components/shared/theme.rs`.

Doc map: [`AGENTS.md`](AGENTS.md).

---

## 1. Overview

Dense XRPL dashboard in the terminal. Royal-blue chrome + turquoise metadata. Focus is a bright border; everything else stays slate-muted. No shadows, no page canvas — the terminal background is the canvas.

Mood: trading-terminal, not marketing site. One accent family (dodger/cornflower/royal). Green/red are **ledger semantics** (bid/in, ask/out, tesSUCCESS/error), not decoration.

---

## 2. Colors (`theme.rs`)

| Token | RGB | Hex | Role |
|-------|-----|-----|------|
| `ACCENT` | 30,144,255 | `#1E90FF` | Focused border, values, table header, selected-row intent |
| `BORDER` / `HIGHLIGHT_BG` | 65,105,225 | `#4169E1` | Selected row background (focused) |
| `TITLE` | 100,149,237 | `#6495ED` | Focused panel title |
| `SECONDARY` | 64,224,208 | `#40E0D0` | Hashes, metadata |
| `MUTED` | 119,136,153 | `#778899` | Unfocused chrome, dim labels, footer |
| `SUCCESS` | 60,179,113 | `#3CB371` | Bid, inbound ▲, tesSUCCESS, valid preview, ONLINE |
| `ERROR` | 220,20,60 | `#DC143C` | Ask, outbound ▼, errors, OFFLINE, **mainnet/xahau** badge |
| `WARNING` | 255,165,0 | `#FFA500` | Incomplete preview, CONNECTING, **test/dev/xahau-test** badge |
| `FLAG` | 100,200,255 | `#64C8FF` | Account flag chips |
| `HIGHLIGHT_FG` | 255,255,255 | `#FFFFFF` | Text on selected row |
| `CHART_VALUE_FG` | 15,23,42 | `#0F172A` | Text on `ACCENT` fills |

**Frontmatter mapping:** `primary` = `ACCENT`; `border` = `BORDER` = `HIGHLIGHT_BG` (identical RGB). `panel-*` の `textColor` はタイトル色。focused border = `ACCENT` / unfocused = `MUTED`（`borderColor` は非標準プロパティのため frontmatter 外 — Components 表を参照）。

---

## 3. Typography

No shipped font — the terminal provides it. Hierarchy is ratatui modifiers only: header `bold+underlined` (`header_row_style`); focused title `bold`; selected row `bold`; splash connecting line `italic`; unfocused chrome `MUTED`; help keys `ACCENT+bold`.

---

## 4. Layout

**Shell** (`app.rs` draw): tabs 1 row → main `Fill` → hints 1 row → status 1 row. FPS counter: 10×1 at top-right of main.

| Tab | Title (shipped) | Split | Focus targets |
|-----|-----------------|-------|---------------|
| 0 | `󰖟 Overview` | Flare on: **44 / 56** horizontal; right = combined `Fill` + Flare wallet **7 rows** (not a focus target). Flare off: server full width | 2 (server, combined) or 1 |
| 1 | `󰀉 Account` | **46 / 54** vertical. Top = wallet **or** account (seed present → wallet). Bottom = tx history | 2 (top / history) |
| 2 | `󰠿 Market` | Top **70%**: book **62%** \| path/AMM/lines **38%** (34/33/33). Bottom **30%**: FTSO+oracle 50/50, or oracle full when Flare off | 6, or 5 (FTSO skipped; index 4 → oracle) |
| 3 | `󰒍 Assets` | Vertical **30 / 30 / 20 / 20** (NFT, objects, pay channels, escrows) | 4 |

Guard: `TAB_TITLES.len() == panels.len() == 4` (`app.rs`, TC-060).

**Flare `[flare] display`** (pixels only; poll rules: architecture §6.7):

| Level | Overview | Market |
|-------|----------|--------|
| `full` | 44/56 + full FTSO/FXRP tables + wallet strip | Full FTSO table + oracle |
| `compact` | Same split; 1-line FTSO + 1-line FXRP | 1-line FTSO + oracle |
| `off` | Server only | FTSO hidden; oracle keeps the bottom strip |

## 5. Components

| Piece | Source | States |
|-------|--------|--------|
| Panel chrome | `panel_block` / `titled_block` | Focused: `ACCENT` rounded border + `TITLE`. Unfocused: `MUTED`. Optional `(i/n)` count via `titled_block_with_count` |
| Tabs | `app.rs` `TAB_TITLES` | 4 titles with nerd-font glyphs. Active: `HIGHLIGHT_BG` bold underline. Index prefix `MUTED`. Divider ` │ ` muted |
| Table | `render_selectable_table` | Header accent underline. Focused selection: white on royal. Unfocused selection: muted reversed |
| TX row | `tx_table_row` | Hash `SECONDARY` (16-char + `…`); ▼ `ERROR` / ▲ `SUCCESS` / · `MUTED`; type `ACCENT`; ledger muted; result success/error |
| Status bar | `status_bar.rs` | Left: connection chip reversed (`● ONLINE` green / `✖ OFFLINE` red / `○ CONNECTING` orange) + `acct:` / `srv:` / `acc:` / `bk:` freshness + optional mid price + spinner while refresh. Right: `format!(" {} ", display_name())` reversed bold — `ERROR` if `is_production()` (mainnet **or** xahau), else `WARNING` |
| Footer hints | `footer_line` | Keys bold, labels dim. Always `?` `Tab` `1-4` `jk` `hl` `^Z` `q`. Tab extras: Overview `t` `g` `r` `Enter`; Account `t` `f` `r`; Market `b`; Assets `o` |
| Loading | `render_loading` | Accent braille spinner (`⠋…`) + muted message |
| Empty | `render_empty` | Muted paragraph in titled block |
| Error | `render_error` | `error: ` in `ERROR` + message |
| Help overlay | `help_overlay.rs` | Centered ~58-col popup, `Clear` + `panel_block("Keybindings", true)` |
| TX / dUNL overlay | `centered_popup_rect` | ~80% of area, clamped; never panics on tiny terminals |
| Composer preview | `payment_preview` | Green `▸ Send/Pay …` when valid; orange need-/invalid copy when not |
| Splash | `splash.rs` | Focused `LazyXRP` block; spinner + italic `Connecting` |

**Surface stack (front → back):** help / TX detail / composer modal → focused panel → unfocused panels → tab strip / footer / status. Overlay `Clear`s its rect. While overlay or composer is open, background tables must not change selection.

---

## 6. Keys (operated)

Defaults: [`config.json5`](config.json5) `keybindings.Splash`. User merge: `~/.config/lazyxrp/config.toml`. `Tab` / `Shift+Tab` / `1`–`4` are hard-coded in `app.rs`.

| Key | Action |
|-----|--------|
| `q` | Quit (disabled while `keymap_suppressed`) |
| `Ctrl-c`, `Ctrl-d` | Quit (always) |
| `Ctrl-z` | Suspend |
| `Ctrl-n` | Cycle network badge: mainnet→testnet→devnet→xahau→xahau-test (session only) |
| `r` / `b` / `o` | Refresh account / book / ledger objects |
| `j` `k` / arrows | Row select |
| `h` `l` / arrows | Panel focus in tab |
| `Enter` | TX detail overlay (dUNL detail on Overview server) |
| `?` | Help overlay; `Esc` or `?` closes |
| `1`–`4` | Jump tab |

### Panel-local (focus / modal)

**Wallet + composer** (`wallet.rs`, `wallet_composer.rs`)

| Context | Key | Effect |
|---------|-----|--------|
| Table, modal closed | `j`/`k`, arrows | Row + scrollbar |
| Table | `Enter` | TX detail |
| Wallet | `t` | Composer picker: Payment, AccountSet, SetRegularKey, OfferCreate, TrustSet, FXRP Mint, Execute |
| Picker | `Tab`, `j`/`k`, arrows, `1`–`7` | Phase select |
| Forms | `Tab`, `[` `]`, `Enter` | Field nav; AccountSet `e` edits |
| Payment | `i` | XRP ↔ IOU |
| Submit | `s`, `Ctrl-s` | Queue; preview color as in §5 |

**Tx history:** `f` filter (`Enter` confirm, `Esc` cancel); `m` next page when marker.

**NFT:** `j`/`k` select (URI → image fetch).

**Tables** (book, lines, objects, path find): `j`/`k` + `Enter` → overlay.

**TX detail:** `j`/`k` scroll; `Enter`/`Esc` close. Consumes actions so tables underneath stay put.

New global binding: `config.json5` + `Action` + `app.rs` dispatch; document here and in [`docs/test.md`](docs/test.md) when user-visible.

---

## 7. Do's and Don'ts

**Do**

- Draw chrome through `theme.rs` helpers.
- Put values on `ACCENT` fills with `chart_value_style()` (`CHART_VALUE_FG`).
- Keep composer/forms behind `SetKeymapSuppression(true)` so bare `q` does not quit.
- Treat mainnet **and** xahau as the red network badge (`Network::is_production()`).

**Don't**

- Hardcode `Color::Black` or raw RGB in panels.
- Invent a fifth tab or `1`–`5` jump (runtime is `1`–`4`).
- Draw Account as three stacked panels — top is wallet **xor** account.
- Persist `Ctrl-n` into `config.toml` from the TUI.
- Change selection under an open overlay.

---

## 8. Agent prompt (when drawing TUI)

Royal-blue ratatui dashboard. Rounded panels. Focus = dodger-blue border + cornflower title; idle = slate. Selected row = white on royal. Hashes turquoise. Bid/in green, ask/out crimson. Status: reversed ONLINE/OFFLINE/CONNECTING chip left; reversed network name right (crimson on mainnet/xahau, orange otherwise). Tabs: `1 󰖟 Overview` … `4 󰒍 Assets`. Spinner `⠋` in accent. Valid composer line green `▸`; incomplete orange. Never black text on blue fills — use slate `#0F172A`.
