# Order book

The Market tab's `Book Offers` table shows the DEX book for the configured pair, XRP against RLUSD issued by `rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De`. Each row has quality, price, TakerGets and TakerPays. The `AMM Pool`, `Trust Lines` and `FTSOv2` panels sit next to the book. `-x book` prints the offers as JSON.

## Sub-features

- `book-tui`: `Book Offers (i/N)` rows. `b` refreshes the book (`RefreshBook`).
- `book-detail`: `Enter` on a row opens the `Transaction Detail` overlay (offer fields). `Enter` again closes it; `Esc` does not.
- `book-cli`: `-x book --base --quote --issuer --limit` prints JSON.

## How to get to it (user POV)

- Launch the TUI and press `3`.
- `lazyxrp --network testnet -x book --base XRP --quote <CUR> --issuer rISSUER`

## Driving it with verify.py

Preconditions:

- Baseline, with no credential.
- The testnet XRP/RLUSD (`rMxCK…`) book has at least one offer. That issuer address is RLUSD's mainnet issuer, and on testnet these are a handful of stray offers. If the book is empty, report the precondition as unmet. Do not treat an empty table as a pass.

- **TUI book and refresh.** Open Market and wait until rows arrive. Press `b`. The proof that `b` refreshed is that this launch's app log contains `Got action: RefreshBook`. The `bk:` age alone cannot prove it, because the 5 s scheduled poll also resets that age.

  ```sh
  $H tui --name book-tui --step "wait:1-4:jump@40" --step "press:3=>Book Offers (@20" --step snap:book --step "press:b=>log:Got action: RefreshBook" --step snap:refreshed --step key:q
  grep -q "524C5553" target/verify/$VERIFY_RUN/book-tui.book.screen.txt
  grep -q "Got action: RefreshBook" target/verify/$VERIFY_RUN/book-tui.app.log
  ```

- **Row detail.** Press `Enter` on the first row to open `Transaction Detail` (`Flags:`, `PreviousTxnLgrSeq:` …). A second `Enter` closes it.

  ```sh
  $H tui --name book-detail --step "wait:1-4:jump@40" --step "press:3=>Book Offers (@20" --step "press:Enter=>Transaction Detail" --step snap:detail --step "press:Enter=>!Transaction Detail" --step key:q
  grep -q "PreviousTxnLgrSeq" target/verify/$VERIFY_RUN/book-detail.detail.screen.txt
  ```

- **CLI.** `--quote RLUSD` (the symbol a user types) and the 40-hex code both exit `0` and return offers whose `taker_pays` carries the `524C5553…` code. The recipe does not diff the two outputs, because they are separate live reads and the book can change between them.

  ```sh
  $H cli --name book-cli -- --network testnet -x book --base XRP --quote RLUSD --issuer rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De
  grep -q '"taker_pays": ".* 524C555344000000000000000000000000000000"' target/verify/$VERIFY_RUN/book-cli.stdout
  $H cli --name book-cli-hex -- --network testnet -x book --base XRP --quote 524C555344000000000000000000000000000000 --issuer rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De
  grep -q '"taker_pays": ".* 524C555344000000000000000000000000000000"' target/verify/$VERIFY_RUN/book-cli-hex.stdout
  ```

## Gotchas

- Symbols of 4–20 bytes become zero-padded 160-bit hex (`currency_code_for`, TC-368). Before that fix, the CLI sent them raw and failed with `bad currency`.
- Each `press:` step fails up front if its target already holds before the key is sent. A needle that is visible before the press proves nothing.
