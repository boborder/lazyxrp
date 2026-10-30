# problems.md

[日本語](problems-ja.md)

**Role:** Pitfalls and gotchas (P-xxx known issues and operational notes).

## Known issues

### P-002: Link failure from unresolved `critical-section` symbols

- Symptom: `Undefined symbols for architecture arm64: __critical_section_1_0_acquire/release`
- Cause: a dependency path such as `embassy-sync` needs `critical-section`, and linking fails when the `std` implementation is not enabled
- Fix: add `critical-section = { version = "1.2.0", features = ["std"] }` to `Cargo.toml`
- Note: even though nothing in the source uses it directly, this dependency is required to enable the `std` implementation. Do not remove it as an unused dependency
- Verification: run `cargo check` and `cargo build --bins` back to back and confirm the failure does not recur

### ~~P-003: EscrowCreate composer/poll still unwired~~ → resolved (2026-10-11)

- In fact signing was also unimplemented, and the `_ => {}` arm in poll silently dropped `EscrowCreateSubmit`. Added signing / `submit_escrow_*` / composer (`9` Create, `0` Finish/Cancel) and removed the catch-all from the poll `PollCommand` match (a missed new command is now a compile error). TC-215–221; testnet E2E is in `docs/test.md`.

### ~~P-005: `config.rs` remains one file~~ → resolved (2026-10-28)

- Split `src/config.rs` (about 1530 lines) into `src/config/mod.rs` (Config, load/merge, env, paths, Flare sections, cross-area tests), `config/defaults.rs` (fallback constants, serde defaults, `Default` for `LedgerConfig` / `FlareFassetsConfig`), `config/keybindings.rs` and `config/styles.rs`. Pure move; `crate::config::*` paths unchanged.

### P-007: Xahau `NetworkID` acceptance by simulate is unverified

- On networks with `network_id > 1024` (Xahau etc.), writes inject `NetworkID` into the tx (simulate uses the id stored by `ensure_network`; signing uses the thread-local scope of `signing::with_network_id`).
- Whether Xahau's `simulate` accepts a tx carrying `NetworkID` is **not verified on a live node** (unit test TC-352 covers only the injection condition). Writes to Xahau are not guaranteed until a live check is done.

### P-008: Public-executor dependency of FXRP mint finalization

- A mainnet read-only scan (2026-10-24) shows public executors exist (905 plain + 1000+ Smart Account mints in ~50 days, 95–177 s after XRPL validation, 8 samples), but their fee floor is unmeasured and no Flare write has been sent on mainnet.
- If nobody picks a payment up, the XRP waits in the Core Vault, and self-finalizing needs an EVM key + ~20 FLR (ROADMAP F4-d, security.md 2026-10-24).

### P-009: Bilingual docs can drift

- There is no automated check. When editing, compare the ID sets (TC/FR/NFR/S/R/I/P/F) of `X.md` vs `X-ja.md` (see maintenance.md).

### P-010: Occasional testnet write expiry (`LastLedgerSequence`)

- Symptom: 2 of about 10 testnet writes on 2026-10-28 (`-x send`, `-x offer-create`) ended with `expired: not validated by LastLedgerSequence`. `rp -t <hash>` showed the tx known but `validated: false`; a retry validated at once. A follow-up run of 12 sends in a row all validated.
- Unconfirmed cause: the Fee comes from `simulate` and `LastLedgerSequence` is `ledger_index + 20` (P-006). A fee spike after simulate could leave the tx queued until it expires. Not reproduced, so the fee logic is unchanged.
- Second hypothesis: both expiries were the first write right after `-x faucet` funded the account (`LastLedgerSequence` anchored on a ledger that had just closed). Weakened, not ruled out, by the 12-send run, whose first send also followed a faucet and validated.
- Next step if it recurs (especially on mainnet): record `-x info`'s `OpenLedgerFee` next to the failing hash before changing anything.

### ~~P-011: RPC errors are a string protocol~~ → resolved (2026-10-30)

- `parse::NodeError` is the node's own error reply (`result.error`); `[XRPL-<kind>] <message>` is only its `Display`. Callers ask the type (`node_error`, `is_not_found`, `is_retryable_rpc_error`) instead of `starts_with("[XRPL-")` / substring matches on a formatted string, and a transport error can no longer be taken for "not found".
- **Behaviour change (bug fix).** The old classifiers looked for the node's `error` *token* (`tooBusy`, `entryNotFound`, …) in the formatted message, which holds only the prose `error_message` ("The server is too busy to help you now."). So `tooBusy`, `noNetwork`, `noCurrent`, `noClosed`, `failedToForward` and `amendmentBlocked` were never retried against a real node (they matched only when the prose repeated the token, as the old test fixture did), and `entryNotFound` / `lgrNotFound` / `actMalformed` were labelled `NOT_FOUND` but not treated as such. Both now follow the token; the displayed text is unchanged. Checked on 24 error shapes: exactly those 9 node cases (plus a transport string that used to read as "not found") changed.

### ~~P-012: `(sequence, fee_drops, last_ledger_sequence)` travels as three bare `u32`s~~ → resolved (2026-10-30)

- `signing::SimulatedFill { sequence, fee_drops, last_ledger_sequence }` is what `sequence_fee_ledger_from_simulate` returns and what every `create_and_sign_*`, the `txspec` `sign` pointers and the CLI / submit `sign` closures take, so a swapped pair no longer compiles. `PaymentSpec` and `AccountSetChanges` carry the Payment and AccountSet fields shared by the simulate request and the signed transaction (each flow builds it once and passes it to both). Verified against the previous code: 31 signed blobs, simulate JSON and refusals are byte-identical for distinct values per field. `signing.rs` has no `#[allow(clippy::too_many_arguments)]` left.

### ~~P-013: Functions over 150 lines remain~~ → resolved (2026-10-30)

- No production function is 150 lines or longer (the longest are `run_flare_command`, `poll_xrpl`, `submit_payment_transaction`, `submit_smart_account_instruction` at 144–147; `field_validation_tables` in `txspec/identity.rs` is a test). Split by job, not by size: `poll.rs` (`drive_poll_loop` 366 → 111 + `run_command` / `poll_xrpl` / `poll_oracle_prices` / `poll_flare`, TC-372), `app.rs` (`drain_and_dispatch_actions` 280 → 11 + `handle_action` / `fan_out` / `poll_command_for`, TC-373), `tx_detail` and `validator_detail_lines` by section, the Server / Account `draw`s by region, the wallet `update` (14 success arms → the `submit_success` table), `submit_fxrp_direct_mint_payment` (`plan_direct_mint`), and `lib.rs::run_flare_command` (324 → 144, one function per Flare subcommand, TC-374).
- Each split was checked against the old code, not only by the existing tests: temporary differential tests (not committed) compared the output before and after for 12 transaction shapes, 1,200 validator-detail combinations, 360 server and 1,008 account panel states (styled buffers), 448 wallet success-action cases and 25,920 Direct Mint Payment inputs, all byte-identical. The Flare subcommand bodies need a live Flare node, so they were moved verbatim (a line-multiset comparison against the old function shows only the intended differences: borrowed instead of owned `url`, and one shared `send_direct_mint` for `sa-memo` / `sa-call`); only the refusals before any network I/O have a test (TC-374).

### ~~P-014: Seven `too_many_arguments` allows remain outside `signing.rs`~~ → resolved (2026-10-30)

- No `#[allow(clippy::too_many_arguments)]` is left in the crate. `poll::Backoff { secs, until }` (`record(succeeded)`, `is_active()`) replaces the paired `&mut` backoff variables; `submit::SubmitPlan { submit_err, build, sign, on_ok }` names the three closures `run_submit` / `run_submit_with_wallet` took by position; `app::Endpoints { network, rpc_url, ws_url }` is the resolved connection that `App::new` takes; `server_detail::TomlFetch` is the TOML fetch outcome; `ComposerPhase::payment_edit_key(&mut self, key)` works on the Payment phase itself instead of five `&mut String` fields.
- Found on the way: the second `Action::XrplTomlFetched ... if self.detail.visible` arm in `ServerPanel::update` was unreachable (the `if self.detail.visible { match … }` above it returns on every arm), so it was deleted rather than ported.

## Operational notes

- The macOS `ld: ... built for newer 'macOS' version ...` message is currently treated as a warning
- Build stability is complete only when `cargo build --bins` succeeds in addition to `cargo check` (link errors cannot be detected by `check` alone)

## Resolved (history)

- ~~P-001: lifetime bound error from a known Rust limitation (`issue #100013`)~~ → resolved by restructuring `start_poll_task` / `start_ws_task`
- ~~W-001: unused warning for the `SplashScreen` component~~ → integrated into the `splash` field of `App`
- ~~W-002: unused warnings related to Phase 3~~ → `create_payment_tx` removed. The `#[allow(dead_code)]` on `create_and_sign_payment` and the "Phase 3" comment were also removed (production paths: `poll.rs` / `cli_exec.rs`, 2026-09-12)
- ~~P-004: Submit validation split across layers~~ → `signing::validate_payment_input` is now shared by the composer (preview / submit / quote) and poll `PaymentSubmit` (TC-197–198, 2026-10-10)
- ~~P-006: live `simulate` omits `LastLedgerSequence`~~ → rippled 3.x autofills Fee/Sequence only; every write (TUI + CLI) failed on testnet. `parse_simulate_result` fills `ledger_index + 20` when absent (TC-200, 2026-10-10)
