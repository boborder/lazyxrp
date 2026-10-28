---
name: verify-lazyxrp
description: Drive the real lazyxrp binary (ratatui TUI and `lazyxrp -x` CLI) against XRPL testnet in an isolated config/data dir and capture screen/stdout evidence. Use when proving a lazyxrp feature or bug fix works end to end — tabs, account/server/book panels, CLI reads, testnet writes — instead of trusting unit tests.
---

# Verify lazyxrp

lazyxrp is a terminal app with two surfaces: the **TUI** (`lazyxrp`, 4 tabs) and the **CLI** (`lazyxrp -x <subcommand>`). There is no server to keep alive. Each drive is a fresh process in its own PTY (TUI) or subprocess (CLI), against **public XRPL testnet**. Everything goes through one helper:

```sh
H=.omp/skills/verify-lazyxrp/scripts/verify.py   # run from the repo root; needs `uv` (pulls pyte on first run)
export VERIFY_RUN=$(date +%Y%m%d-%H%M%S)          # one id per verification run
```

| Path | What |
|---|---|
| `$TMPDIR/lazyxrp-verify-$VERIFY_RUN/{config,data}` | Scratch. `LAZYXRP_CONFIG` / `LAZYXRP_DATA` point here, so the user's `~/.config/lazyxrp` and app-support dir are never read or written. `data/lazyxrp.log` is the app log. |
| `target/verify/$VERIFY_RUN/` | **Evidence.** Survives `clean`. `cargo clean` deletes it, so copy it out first if it must outlive the build dir. |

The helper strips every `XRPL_*`, `FLARE_*`, `LAZYXRP_*`, `TUI_*` variable from the child's environment. The binary does not load `.env` itself, but the invoking shell may have exported a real `XRPL_SEED`, `FLARE_EVM_KEY`, or a private `XRPL_RPC_SERVER` / `FLARE_RPC_URL`, and none of these may reach a verification run. To pass a value on purpose (a testnet faucet seed, `LAZYXRP_LOG_LEVEL=debug`), use `--env KEY=VAL`.

## Launch

```sh
$H build            # cargo build --locked -q --bin lazyxrp → target/debug/lazyxrp
```

That is the whole launch. Each `tui` / `cli` call below starts its own process from the empty scratch dir, with no config file. The `no configuration file found … defaults apply` warning on stderr is expected. Defaults include watch account `r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV` (funded on testnet) and book `XRP/RLUSD` with issuer `rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De`.

**Ready (TUI)** means the main shell is drawn: `--step "wait:1-4:jump@40"`. That footer hint is drawn only after the splash ends, which happens when the first `server_info` arrives (`app.rs` `startup_done`).

Always pass `--network testnet`; it is the TUI default in `--args`. Never point a run at `mainnet` or `xahau` with a credential.

## Doctor

Run this first, and again whenever anything looks off. It is read-only.

```sh
$H doctor           # exit 0 = worth driving
```

It checks four things: the binary exists, `--version` matches `Cargo.toml`, the binary is newer than every `src/**/*.rs` (if it is stale, run `$H build`), and no `XRPL_`/`FLARE_` variable leaks into the child. It also runs `-x info` against testnet, which must return `"ledger_index"`, and prints any still-live PIDs this run started. The doctor does not run CI gates; for those use `mise run lint`, `mise run doc`, and `mise run test-serial`.

## Drive

**CLI.** Everything after `--` is lazyxrp's argv. stdin is closed, so a production confirmation prompt refuses instead of hanging. The helper exits with lazyxrp's exit code; with `--expect-exit N` it exits 0 only when lazyxrp exited with N.

```sh
$H cli --name info -- --network testnet -x info
```

**TUI.** The terminal is a 160×48 PTY rendered by `pyte`. Steps run in order:

| Step | Meaning |
|---|---|
| `wait:T[@s]` | Wait until target T holds (default 30 s). On timeout the run stops and fails. |
| `press:K=>T[@s]` | Send key K, wait for T (default 3 s), resend at most 3×; each resend is logged as `retry N`. Fails up front if T already holds before the key, since that would prove nothing. **Use it for every navigation key.** |
| `key:K` | Send K once with no check. Use it only when the next step is a `wait` or the key quits. |
| `sleep:S` | Keep rendering for S seconds. |
| `snap:NAME` | Save the screen text to `<name>.NAME.screen.txt`. |

Targets: `TEXT` = on screen; `!TEXT` = no longer on screen; `log:TEXT` = in this launch's app log (use this for actions that leave no unique screen change, e.g. `log:Got action: RefreshBook`).

Named keys: `Tab S-Tab Enter Esc Up Down Left Right Space BSpace C-c C-n C-s`. Anything else is sent literally (`2`, `q`, `?`, `t`).

```sh
$H tui --name tour \
  --step "wait:1-4:jump@40" \
  --step "press:2=>Tx History" --step snap:account \
  --step key:q
```

**Recipes.** Every feature file's `sh` blocks are self-checking: each ends in `grep -q` assertions, so a failed assertion fails the block. Run them verbatim, one bash process per file:

```sh
$H recipes .omp/skills/verify-lazyxrp/features/order-book.md      # or features/*.md for the whole map
```

`--args "…"` replaces the lazyxrp argv; the default is `--network testnet`. For watch mode use `--args "--network testnet --watch --account rADDR"`. When the steps finish, the helper sends `Ctrl-c` if the app is still running, and kills it after 3 s.

Stable handles:
- Tab bar: `1 … Overview  │  2 … Account  │  3 … Market  │  4 … Assets`.
- Panel titles: `Server`, `Wallet` / `Account`, `Tx History`, `Book Offers`, `AMM Pool`, `Trust Lines`, `FTSOv2`, `NFTs`, `Objects`.
- Status bar chip: `● ONLINE` / `✖ OFFLINE` / `○ CONNECTING` / `● STALE`; the network name is at the right edge (`TESTNET`).
- Footer: per-tab hints, e.g. `t:tx  g:keygen  f:filter` on Account with a credential, `b:book` on Market.

Source of truth for keys: `DESIGN.md` §6. Per-feature recipes: [`features/README.md`](features/README.md).

## Evidence

Each run writes these files to `target/verify/$VERIFY_RUN/`:
- **CLI:** `<name>.cmd`, `<name>.stdout`, `<name>.stderr`, `<name>.exit`.
- **TUI:** `<name>.steps.log` (timestamped steps, `term-reply` lines for the terminal-query answers pyte sent, any `retry` lines, then `exit=` and `PASS` or `FAIL <reason>`), `<name>.<snap>.screen.txt`, and `<name>.final.screen.txt`.

Proof standards:
- Drive the user path: real keys in the TUI, real argv in the CLI. Never call internal functions or edit config to fake a state.
- Capture the action and the resulting state: a snap before and after the key, or the write's stdout followed by a second, read-only view (`-x account`, `-x tx-history`, or the TUI `Tx History` row).
- Writes go to testnet only, from a faucet wallet created in the same run. Prove the side effect on the ledger with a read, not just the `Validated: tesSUCCESS` line.
- Screen text is the proof format: pyte renders text, not pixels, so colours and images are not captured. When a claim is visual (theme colours, NFT image), say that it is unverified.

## Cleanup

```sh
$H clean            # SIGTERM to PIDs this run started (from scratch/pids), then rm the scratch dir
ls target/verify/$VERIFY_RUN   # evidence is still here
```

`clean` kills only PIDs it recorded and never matches by process name. Testnet ledger state (faucet wallets, sent XRP) cannot be removed and does not need to be. Run `clean` after failed attempts too.

## Gotchas

- **Dropped keys = regression.** Before the stdin-race fix, `tui.enter()` started crossterm's `EventStream` before `NftTab::init` ran `Picker::from_query_stdio()`. crossterm consumed the terminal's replies to the picker's query, and the picker's leaked reader thread then stole about 1 in 3 keystrokes. Now `Tui::start` runs after every `init` (8/8 runs with no `retry`, and `sample <pid>` shows no `query_stdio_capabilities` thread). A `retry` line in `steps.log` therefore points to a real regression: `sample` the pid and report it.
- pyte draws nerd-font glyphs as one cell, so right-hand borders sometimes show `�` or shift by a column. Match on words, not box geometry.
- **The mainnet `--yes` prompt cannot be reached live.**
  - With a fresh wallet, `--network mainnet -x send …` fails on `[XRPL-NOT_FOUND] Account not found.`
  - With a testnet-funded seed and `--network mainnet --server https://s.altnet.rippletest.net:51234`, the run is refused earlier with `endpoint network_id 1 is not MAINNET (network_id 0); refusing to sign`. That is a real guard; see `send-xrp.md` `send-netid-guard`.

  The `[y/N]` prompt itself is covered only by TC-047/TC-048/TC-088. Never fund a mainnet account to prove it.
- `faucet.stdout` contains the generated **testnet** seed, so the evidence dir holds a secret. Evidence lives under the gitignored `target/`; never copy it into a commit or PR.
- The Flare panels (FTSO, FXRP) read Flare **mainnet** RPC regardless of `--network`; that traffic is read-only.
- **Public-testnet flakes.** Two failures seen in the 2026-10-28 runs passed on rerun:
  - A launch stayed on the `LazyXRP` splash for over 40 s, so `wait:1-4:jump` timed out.
  - A `-x send` failed with `expired: not validated by LastLedgerSequence`.

  Rerun the whole file once. If it fails again, rerun with `--env LAZYXRP_LOG_LEVEL=debug` and read `<name>.app.log`; the default log level shows no RPC errors. Report a failure that repeats, and never retry it until it passes.
- The splash may show `i=31,s=1,…;AAAA` in its first line. That is ratatui-image's kitty-graphics query, which pyte prints as text, and it is harmless.
