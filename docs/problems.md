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
- Next step if it recurs (especially on mainnet): record `-x info`'s `OpenLedgerFee` next to the failing hash before changing anything.

## Operational notes

- The macOS `ld: ... built for newer 'macOS' version ...` message is currently treated as a warning
- Build stability is complete only when `cargo build --bins` succeeds in addition to `cargo check` (link errors cannot be detected by `check` alone)

## Resolved (history)

- ~~P-001: lifetime bound error from a known Rust limitation (`issue #100013`)~~ → resolved by restructuring `start_poll_task` / `start_ws_task`
- ~~W-001: unused warning for the `SplashScreen` component~~ → integrated into the `splash` field of `App`
- ~~W-002: unused warnings related to Phase 3~~ → `create_payment_tx` removed. The `#[allow(dead_code)]` on `create_and_sign_payment` and the "Phase 3" comment were also removed (production paths: `poll.rs` / `cli_exec.rs`, 2026-09-12)
- ~~P-004: Submit validation split across layers~~ → `signing::validate_payment_input` is now shared by the composer (preview / submit / quote) and poll `PaymentSubmit` (TC-197–198, 2026-10-10)
- ~~P-006: live `simulate` omits `LastLedgerSequence`~~ → rippled 3.x autofills Fee/Sequence only; every write (TUI + CLI) failed on testnet. `parse_simulate_result` fills `ledger_index + 20` when absent (TC-200, 2026-10-10)
