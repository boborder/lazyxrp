# Send XRP

A user with a signing credential sends XRP to another account. `-x send` runs `simulate`, then submits and waits for validation. The sender's balance drops by the amount plus the fee, and the payment appears in the TUI `Tx History`. Writes are refused when the endpoint's network does not match `--network`. On testnet, `-x faucet` creates a funded throwaway sender.

## Sub-features

- `send-faucet`: `-x faucet` with no address generates a wallet locally, funds it with 100 XRP, and prints the seed once.
- `send-cli`: `-x send DEST --amount X` prints `Simulate: tesSUCCESS`, a hash, and `Validated: tesSUCCESS`.
- `send-ledger`: the sender's `balance_xrp` drops by the amount plus the fee.
- `send-history`: the Account tab, opened with the same seed, lists the Payment (`▼`) at the top of `Tx History`.
- `send-netid-guard`: with `--network mainnet` against a testnet endpoint, the send is refused before signing.
- `send-tui`: Account tab `t` → picker `[1] Payment` → fill the composer → `Ctrl-s`. The flash reads `Payment sent · <hash>`, and the wallet balance drops.

## How to get to it (user POV)

- `XRPL_SEED=… lazyxrp --network testnet -x send rDEST --amount 1.5`
- `XRPL_SEED=… lazyxrp --network testnet`, then `2` → `t` → `Enter` on `[1] Payment`

## Driving it with verify.py

Preconditions:

- Baseline. The public testnet faucet is reachable. The destination is `r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV`.

- **Faucet wallet.** Expect `Funded rADDR with 100 XRP` and a `Seed: s…` line.

  ```sh
  $H cli --name send-faucet -- --network testnet -x faucet
  E=target/verify/$VERIFY_RUN; SEED=$(sed -n 's/^Seed: //p' $E/send-faucet.stdout); ADDR=$(sed -nE 's/^Funded (r[^ ]+).*/\1/p' $E/send-faucet.stdout)
  test -n "$SEED" && test -n "$ADDR"
  ```

- **Send.** Expect exit `0`, `Simulate: tesSUCCESS`, `Transaction Hash: <H>` and `Validated: tesSUCCESS`.

  ```sh
  $H cli --name send-cli --env XRPL_SEED=$SEED -- --network testnet -x send r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV --amount 1.5
  grep -q "Simulate: tesSUCCESS" $E/send-cli.stdout && grep -q "Validated: tesSUCCESS" $E/send-cli.stdout
  HASH=$(sed -n 's/^Transaction Hash: //p' $E/send-cli.stdout)
  ```

- **Ledger side effect.** `balance_xrp` must be `98.499990` (100 − 1.5 − 0.00001).

  ```sh
  $H cli --name send-ledger -- --network testnet -x account $ADDR
  grep -q '"balance_xrp": "98.499990"' $E/send-ledger.stdout
  ```

- **History.** The first `Tx History` row starts with the first 16 hex characters of `<H>` and shows `▼  Payment … tesSUCCESS`.

  ```sh
  $H tui --name send-history --env XRPL_SEED=$SEED --step "wait:1-4:jump@40" --step "press:2=>Tx History (@30" --step snap:history --step key:q
  grep -q "${HASH:0:16}" $E/send-history.history.screen.txt
  ```

- **Network-id guard.** The run must exit `1` with `endpoint network_id 1 is not MAINNET (network_id 0); refusing to sign`. Nothing is signed, and the sender's balance stays unchanged.

  ```sh
  $H cli --name send-netid-guard --expect-exit 1 --env XRPL_SEED=$SEED -- --network mainnet --server https://s.altnet.rippletest.net:51234 -x send r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV --amount 0.1
  grep -q "is not MAINNET" $E/send-netid-guard.stderr && ! grep -q "NOT_FOUND" $E/send-netid-guard.stderr
  $H cli --name send-netid-after -- --network testnet -x account $ADDR
  grep -q '"balance_xrp": "98.499990"' $E/send-netid-after.stdout
  ```

- **TUI composer.** The form opens with editing off. `Enter` starts editing the focused row (`Destination`). Type the address, then `Tab` to `Amount XRP`, which stays in editing mode and accepts digits and `.` only. `BSpace` the default `1`, type `0.5`, then `Ctrl-s` (a bare `s` would be text while editing). Expect the flash `Payment sent · <hash>` and the balance going from `98.49999` to `97.99998`.

  ```sh
  $H tui --name send-tui --env XRPL_SEED=$SEED --step "wait:1-4:jump@40" --step "press:2=>g:keygen@10" --step "wait:Balance 98.49999 XRP@20" --step "press:t=>Choose action" --step "press:Enter=>Payment (XRP)" --step key:Enter --step key:r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV --step key:Tab --step key:BSpace --step key:0.5 --step "wait:▸ Send 0.5 XRP@5" --step snap:filled --step key:C-s --step "wait:Payment sent ·@60" --step "wait:Balance 97.99998 XRP@30" --step snap:sent --step key:q
  TUI_HASH=$(grep -oE "Payment sent · [0-9A-F]{64}" $E/send-tui.sent.screen.txt | cut -d' ' -f4)
  $H cli --name send-tui-ledger -- --network testnet -x account $ADDR
  grep -q '"balance_xrp": "97.999980"' $E/send-tui-ledger.stdout && test -n "$TUI_HASH"
  ```

## Gotchas

- **The mainnet `[y/N]` / `--yes` prompt cannot be driven live.** The network-id check above refuses before the prompt, and a fresh mainnet wallet fails earlier still, with `Account not found`. Report the prompt as covered only by TC-047/TC-048/TC-088. Never fund a mainnet account to reach it.
- `send-tui`: `Tab` / `Enter` / `[` `]` move between rows. Typing reaches a field only while editing (`Enter` or `e` toggles editing). A key that lands while editing is off is silently ignored, and `BSpace` / text then goes to whichever row has focus. If the `▸ Send …` preview is missing, the `filled` snapshot shows where the keystrokes went.
- `send-faucet.stdout` contains the testnet seed. It stays under the gitignored `target/` folder.
- Each run creates a new faucet wallet. The history therefore always shows the faucet's `▲` Payment under your `▼` send.
