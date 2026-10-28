# Server overview

The Overview tab's `Server` panel shows the connected rippled endpoint and its live state: URL, rippled version, ledger and host, fee and reserve, quorum and dUNL, followed by a scrollable dUNL validator table. `lazyxrp -x info` prints the same server state as JSON.

## Sub-features

- `srv-panel` — Server panel fields fill in; status bar shows `● ONLINE` and `TESTNET`.
- `srv-dunl` — dUNL table (`manifest N/N · domain …`); `Enter` opens a `Validator #N` overlay and `Enter` again closes it (`Esc` does nothing here).
- `srv-cli` — `-x info` prints `ledger_index`, `hostid`, `build_version`, `validation_quorum`.

## How to get to it (user POV)

- Launch `lazyxrp --network testnet`; Overview is the first tab (`1`).
- Run `lazyxrp --network testnet -x info`.

## Driving it with verify.py

Preconditions:

- Baseline; no credential.

- **Panel.** The snap shows `URL:     https://s.altnet.rippletest.net:51234`, a numeric `Ledger:`, and `TESTNET` in the status bar.

  ```sh
  $H tui --name srv-panel --step "wait:1-4:jump@40" --step "wait:rippled:@20" --step "wait:dUNL validators@30" --step snap:overview --step key:q
  grep -q "s.altnet.rippletest.net" target/verify/$VERIFY_RUN/srv-panel.overview.screen.txt
  grep -q "TESTNET" target/verify/$VERIFY_RUN/srv-panel.overview.screen.txt
  ```

- **dUNL detail.** `Enter` on the first row opens `Validator #1` (manifest / `xrp-ledger.toml` fields); a second `Enter` closes it.

  ```sh
  $H tui --name srv-dunl --step "wait:1-4:jump@40" --step "wait:dUNL validators@30" --step "press:Enter=>Validator #1" --step snap:detail --step "press:Enter=>!Validator #1" --step snap:closed --step key:q
  ```

- **CLI.** Exit `0`; JSON with a numeric `ledger_index`.

  ```sh
  $H cli --name srv-cli -- --network testnet -x info
  grep -qE '"ledger_index": [0-9]+' target/verify/$VERIFY_RUN/srv-cli.stdout
  ```

## Gotchas

- `Reserve:` shows `-` until the first fee poll; wait for a number before asserting it.
- The dUNL list (≈35 validators) changes over time. Assert structure, not domains.
