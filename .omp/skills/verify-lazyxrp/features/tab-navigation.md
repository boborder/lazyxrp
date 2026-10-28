# Tab navigation and help

The TUI shell shows a tab bar (`Overview`, `Account`, `Market`, `Assets`), a per-tab footer of key hints, and a status bar. Number keys jump to a tab, `Tab` / `Shift-Tab` cycle, `?` toggles the keybinding overlay, and `q` quits.

## Sub-features

- `nav-jump` — `1`–`4` jump straight to a tab.
- `nav-cycle` — `Tab` goes to the next tab, `S-Tab` to the previous one.
- `nav-help` — `?` opens the `Keybindings` overlay; `Esc` closes it.
- `nav-quit` — `q` quits (except while a form owns the keyboard); `Ctrl-c` always quits.

## How to get to it (user POV)

- Launch `lazyxrp --network testnet` and press the keys above.

## Driving it with verify.py

Preconditions:

- Baseline; no credential.

- **Jump.** Press `2`, `3`, `4`. The snaps show `Tx History`, then `Book Offers` + `FTSOv2`, then `NFTs` / `Objects` / `Escrows`.

  ```sh
  $H tui --name nav-jump --step "wait:1-4:jump@40" --step "press:2=>Tx History" --step snap:account --step "press:3=>Book Offers" --step snap:market --step "press:4=>NFTs" --step snap:assets --step key:q
  ```

- **Cycle.** From Account, `Tab` lands on Market and `S-Tab` returns to Account.

  ```sh
  $H tui --name nav-cycle --step "wait:1-4:jump@40" --step "press:2=>Tx History" --step "press:Tab=>Book Offers" --step snap:after-tab --step "press:S-Tab=>Tx History" --step snap:after-backtab --step key:q
  ```

- **Help.** `?` opens `Keybindings` with a `q / Ctrl-c / Ctrl-d  Quit` row; `Esc` removes it.

  ```sh
  $H tui --name nav-help --step "wait:1-4:jump@40" --step "press:?=>Keybindings" --step snap:open --step "press:Esc=>!Keybindings" --step snap:closed --step key:q
  grep -q "q / Ctrl-c / Ctrl-d" target/verify/$VERIFY_RUN/nav-help.open.screen.txt
  ```

- **Quit.** Every recipe above ends in `key:q`; its `steps.log` must end with `exit=0` and `PASS` — the helper only sends `Ctrl-c` if the app is still alive after `q`.

  ```sh
  grep -q "^exit=0$" target/verify/$VERIFY_RUN/nav-jump.steps.log
  ```

## Gotchas

- The help overlay truncates its right column at 160 columns (`Next / prev tab / jump by n…`). Match the left column.
- In the Tx History filter (`f`), `q` and digits are filter text; `Esc` first.
