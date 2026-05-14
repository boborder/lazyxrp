# Design Issues

> **Read**: `REPO_INVENTORY.md`, `ARCHITECTURE.md`, `DATA_MODEL.md`, `INVARIANTS.md`, `DEPENDENCY_RULES.md`. **Scope**: full repository. **Confidence**: medium. **Generated**: 2026-05-14 (Pass 4).

## Issue 1: poll.rs submit logic duplication

**Severity**: Medium
**Evidence**: `src/xrpl/poll.rs` contains 5 submit functions (`submit_account_set_transaction`, `submit_payment_transaction`, `submit_set_regular_key_transaction`, `submit_escrow_create_transaction`, `submit_offer_create_transaction`), each ~150-200 lines with near-identical structure: validate → simulate → extract → sign → submit.
**Impact**: Bug in the common pipeline (e.g., timeout handling) must be fixed in 5 places. New submit types require copy-paste.
**Recommendation**: Extract generic `submit_transaction(tx_json, sign_fn)` helper.

## Issue 2: Config depends on app::Mode

**Severity**: Low
**Evidence**: `src/config.rs` imports `crate::app::Mode` for `KeyBindings` deserialization. If `app::Mode` changes, config deserialization may break silently.
**Impact**: Tight coupling between config layer and app layer. Minor — `Mode` currently has only one variant (`Splash`).
**Recommendation**: Move `Mode` to `action.rs` or create `src/mode.rs`.

## Issue 3: No runtime tab-panel consistency check

**Severity**: Low
**Evidence**: `app.rs` hardcodes `TAB_TITLES: &[&str; 5]` and `panels: Vec<Box<dyn Component>>` with 5 entries. No assertion that lengths match.
**Impact**: If a developer adds a 6th tab but forgets to add a panel, the 6th tab would render the wrong panel or panic.
**Recommendation**: Add `debug_assert_eq!(TAB_TITLES.len(), panels.len())` in `App::new()`.

## Issue 4: Unbounded channels for high-frequency events

**Severity**: Low
**Evidence**: `action_tx`, `poll_tx`, `poll_trigger_tx` all use `UnboundedSender`. If the consumer lags (e.g., slow `process_actions` or poll task blocked on RPC), memory grows.
**Impact**: In extreme cases (rapid ledger closes on busy network, slow RPC), unbounded queue could cause memory pressure.
**Recommendation**: Monitor. Add bounded channels with backpressure if observed in production.

## Issue 5: ArcValue mutability convention is unenforced

**Severity**: Low
**Evidence**: `ArcValue(Arc<Value>)` is shared across components. Convention says "don't mutate", but `Arc::make_mut` is available.
**Impact**: Accidental mutation of shared JSON would cause hard-to-debug visual corruption in other panels.
**Recommendation**: Consider `Arc<Value>` directly (without newtype) or document strictly in `ArcValue` doc comment.

## Issue 6: Submit validation is split across layers

**Severity**: Medium
**Evidence**: Payment validation happens in `poll.rs` (XRP/IU semantics), in `wallet.rs` (UI-level field validation), and in `signing.rs` (signing validation). Each layer re-validates.
**Impact**: Inconsistent validation rules between UI feedback and actual submission. User may see "valid" in UI but get "invalid" on submit.
**Recommendation**: Single validation function shared between wallet panel and poll task.

## Issue 7: Config merge logic is monolithic (~1000 lines)

**Severity**: Medium
**Evidence**: `src/config.rs` handles: built-in defaults merge, file loading, env overrides, keybinding parsing, style parsing, path resolution, and test helpers — all in one file.
**Impact**: Hard to test individual merge steps. Changes risk breaking precedence.
**Recommendation**: Split into `config/defaults.rs`, `config/keybindings.rs`, `config/styles.rs` when config.rs exceeds ~1500 lines.

## Issue 8: WS reconnect backoff interacts with poll timer

**Severity**: Low
**Evidence**: WS disconnection triggers backoff (up to 60s). During this window, `poll_trigger_tx` from WS stops, but periodic poll continues. No explicit coupling between WS state and poll behavior.
**Impact**: During WS outage, poll continues normally — no degradation in UX.
**Recommendation**: Acceptable current behavior. Document that WS and poll are intentionally decoupled.

## Issue 9: tx_detail parsers are 29 functions with manual dispatch

**Severity**: Low
**Evidence**: `src/components/shared/tx_detail/parsers.rs` defines ~29 `try_*_detail_lines` functions, dispatched manually in `detail_lines_for()` in `mod.rs`.
**Impact**: Adding a new TX type requires changes in 2 places. Error-prone.
**Recommendation**: Consider a macro or registration-based dispatch if TX types exceed ~40.
