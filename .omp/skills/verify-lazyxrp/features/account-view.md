# Account view

The Account tab shows one XRPL account's balance, sequence, owner count, reserve gauge and transaction history. Without a credential, or with `--watch`, it is the read-only `Account` panel; with a credential, the `Wallet` panel replaces it. The CLI reads the same data as JSON (`-x account`) and gives a funded / unfunded verdict (`-x account-status`).

## Sub-features

- `acct-watch` — `--watch --account rADDR` shows that account read-only (no `t:tx` / `g:keygen` hints).
- `acct-cli` — `-x account rADDR` prints `account`, `balance_xrp`, `sequence`, `owner_count`.
- `acct-status` — `-x account-status rADDR` prints `Status: Activated` or `Status: Not Activated`.

## How to get to it (user POV)

- `lazyxrp --network testnet --watch --account rADDR`, then `2`.
- `lazyxrp --network testnet -x account rADDR`.
- `lazyxrp --network testnet -x account-status rADDR`.

## Driving it with verify.py

Preconditions:

- Baseline. `r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV` is funded on testnet; `rrrrrrrrrrrrrrrrrrrrrhoLvTp` is not.

- **Watch.** The snap shows the address, `Balance:`, `Spendable:`, `Reserve:`, and no `t:tx` in the footer.

  ```sh
  $H tui --name acct-watch --args "--network testnet --watch --account r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV" --step "wait:1-4:jump@40" --step "press:2=>Spendable:@20" --step snap:account --step key:q
  grep -q "r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV" target/verify/$VERIFY_RUN/acct-watch.account.screen.txt
  ! grep -q "t:tx" target/verify/$VERIFY_RUN/acct-watch.account.screen.txt
  ```

- **CLI read.** Exit `0`; `balance_xrp` matches the TUI `Balance:` from the same run (barring a payment in between).

  ```sh
  $H cli --name acct-cli -- --network testnet -x account r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV
  grep -q '"balance_xrp"' target/verify/$VERIFY_RUN/acct-cli.stdout
  ```

- **Status.** Funded → `Activated`; unfunded → `Not Activated`, both exit `0`.

  ```sh
  $H cli --name acct-status-funded -- --network testnet -x account-status r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV
  grep -q "Status: Activated" target/verify/$VERIFY_RUN/acct-status-funded.stdout
  $H cli --name acct-status-unfunded -- --network testnet -x account-status rrrrrrrrrrrrrrrrrrrrrhoLvTp
  grep -q "Status: Not Activated" target/verify/$VERIFY_RUN/acct-status-unfunded.stdout
  ```

## Gotchas

- `Tx History` shows `⠸ loading tx history...` for several seconds; wait for `Tx History (` before asserting rows.
- `--watch` never loads the signing key even when `XRPL_SEED` is set. Without `--watch`, a seed swaps the top panel to `Wallet` (`Balance 98.49 XRP  seq …`, different layout).
