# lazyxrp verification map

This directory is the maintained source for verifying lazyxrp's user-facing behavior. Read the index, then use the matching feature file as the recipe. The harness is `verify.py` (see [`../SKILL.md`](../SKILL.md)); every recipe assumes:

```text
H=.omp/skills/verify-lazyxrp/scripts/verify.py   # `recipes` exports H and VERIFY_RUN itself
export VERIFY_RUN=<unique id>
```

## Baseline preconditions

- `$H build` done; `$H doctor` exits 0.
- Empty scratch config/data (no config file → built-in defaults). Network: testnet.
- Default watch account `r3kmLJN5D28dHuH8vZNUZpMC43pEHpaocV` exists on testnet.
- No credential in the child env unless a recipe passes a faucet seed with `--env XRPL_SEED=…`.
- Never drive an instance this run did not start.

## Driving conventions

- TUI: start with `wait:1-4:jump@40`, which proves the main shell is drawn. Make every navigation key a `press:K=>TARGET` step, where TARGET only becomes true after the key.
- Recipes are `sh` blocks that end in `grep -q` assertions. `$H recipes <file>…` runs them verbatim, so keep them runnable whenever you edit one.
- CLI: `$H cli --name <id> -- --network testnet -x <subcommand> …`.
- Commands are literal; keep quoted text and flags unchanged.
- Name evidence `<feature-id>-<entry>` so artifacts map back to this file.

## Proof and skip reporting

- TUI proof: `steps.log` (with `PASS`) + snaps before and after the action.
- CLI proof: `.cmd` / `.stdout` / `.stderr` / `.exit`.
- Write proof: the write's output **and** a read-only second view of the ledger.
- An unreachable entry point is reported with the command tried and the unmet precondition — never as verified through another entry point.
- Any `retry` line in a TUI `steps.log` is reported (see Gotchas in SKILL.md).

## Feature entry contract

Each feature file: H1 title, one paragraph of user-visible behavior, then exactly `Sub-features`, `How to get to it (user POV)`, `Driving it with verify.py`, `Gotchas`.

## Features

- [Tab navigation and help](./tab-navigation.md) — tab bar, `1`–`4`, `Tab`, help overlay, quit.
- [Server overview](./server-overview.md) — Overview Server panel and `-x info`.
- [Account view](./account-view.md) — Account tab (watch mode) and `-x account` / `account-status`.
- [Order book](./order-book.md) — Market tab Book Offers and `-x book`.
- [Send XRP](./send-xrp.md) — faucet wallet, `-x send`, ledger confirmation in CLI and TUI history.
