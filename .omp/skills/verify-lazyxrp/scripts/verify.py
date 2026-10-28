#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyte>=0.8"]
# ///
"""Drive lazyxrp (CLI + TUI) in an isolated config/data dir and keep evidence.

Run `verify.py -h` or see ../SKILL.md. Scratch: $TMPDIR/lazyxrp-verify-<run>/.
Evidence: <repo>/target/verify/<run>/ (survives `clean`; `cargo clean` deletes it).
"""

import argparse
import os
import pty
import re
import select
import shlex
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import textwrap
import time
from fcntl import ioctl
from pathlib import Path
from termios import TIOCSWINSZ

import pyte

REPO = Path(__file__).resolve().parents[4]
BIN = REPO / "target" / "debug" / "lazyxrp"
COLS, ROWS = 160, 48
# The user's shell/.env may carry a real seed or a private RPC; never leak them into a run.
SCRUB_PREFIXES = ("XRPL_", "FLARE_", "LAZYXRP_", "TUI_")
KEYS = {
    "Tab": "\t", "S-Tab": "\x1b[Z", "Enter": "\r", "Esc": "\x1b", "Up": "\x1b[A",
    "Down": "\x1b[B", "Right": "\x1b[C", "Left": "\x1b[D", "Space": " ",
    "C-c": "\x03", "C-n": "\x0e", "C-s": "\x13", "BSpace": "\x7f",
}


def scratch(run: str) -> Path:
    return Path(tempfile.gettempdir()) / f"lazyxrp-verify-{run}"


def evidence(run: str) -> Path:
    d = REPO / "target" / "verify" / run
    d.mkdir(parents=True, exist_ok=True)
    return d


def child_env(run: str, extra: list[str]) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if not k.startswith(SCRUB_PREFIXES)}
    s = scratch(run)
    (s / "config").mkdir(parents=True, exist_ok=True)
    (s / "data").mkdir(parents=True, exist_ok=True)
    env.update(LAZYXRP_CONFIG=str(s / "config"), LAZYXRP_DATA=str(s / "data"), TERM="xterm-256color")
    for kv in extra:
        k, _, v = kv.partition("=")
        env[k] = v
    return env


def need_bin():
    if not BIN.exists():
        sys.exit(f"missing {BIN}; run: verify.py build")


def cmd_build(_):
    subprocess.run(["cargo", "build", "--locked", "-q", "--bin", "lazyxrp"], cwd=REPO, check=True)
    print(BIN)


def cmd_doctor(a):
    ok = True

    def check(name, cond, detail=""):
        nonlocal ok
        ok &= bool(cond)
        print(f"[{'OK' if cond else 'FAIL'}] {name}{': ' + detail if detail else ''}")

    check("binary exists", BIN.exists(), str(BIN))
    if not BIN.exists():
        sys.exit(1)
    want = re.search(r'^version = "([^"]+)"', (REPO / "Cargo.toml").read_text(), re.M).group(1)
    got = subprocess.run([BIN, "--version"], capture_output=True, text=True).stdout.splitlines()[0]
    check("binary version matches Cargo.toml", f" {want}" in got, f"{got!r} vs {want}")
    newest = max(p.stat().st_mtime for p in (REPO / "src").rglob("*.rs"))
    fresh = BIN.stat().st_mtime >= newest
    check("binary newer than src/", fresh, "" if fresh else "stale → verify.py build")
    env = child_env(a.run, [])
    leaked = sorted(k for k in env if k.startswith(("XRPL_", "FLARE_")))
    check("no XRPL_/FLARE_ env reaches the child", not leaked, ",".join(leaked))
    r = subprocess.run([BIN, "--network", a.network, "-x", "info"], env=env,
                       capture_output=True, text=True, timeout=60)
    ledger = re.search(r'"ledger_index":\s*(\d+)', r.stdout)
    check(f"{a.network} RPC answers `-x info`", r.returncode == 0 and ledger,
          f"ledger {ledger.group(1)}" if ledger else (r.stderr.strip().splitlines() or [""])[-1])
    pids = scratch(a.run) / "pids"
    live = [p for p in pids.read_text().split()] if pids.exists() else []
    live = [p for p in live if _alive(int(p))]
    print(f"[INFO] run={a.run} scratch={scratch(a.run)} live_pids={live or 'none'}")
    sys.exit(0 if ok else 1)


def _alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def cmd_cli(a):
    need_bin()
    argv = [str(BIN), *a.args]
    # stdin closed: a production-network confirmation prompt must refuse, never hang or get a "y".
    r = subprocess.run(argv, env=child_env(a.run, a.env), capture_output=True, text=True,
                       stdin=subprocess.DEVNULL, timeout=a.timeout)
    out = evidence(a.run) / a.name
    (out.parent).mkdir(parents=True, exist_ok=True)
    Path(f"{out}.cmd").write_text(shlex.join(["lazyxrp", *a.args]) + "\n")
    Path(f"{out}.stdout").write_text(r.stdout)
    Path(f"{out}.stderr").write_text(r.stderr)
    Path(f"{out}.exit").write_text(f"{r.returncode}\n")
    print(f"$ {shlex.join(['lazyxrp', *a.args])}\nexit={r.returncode}\n--- stdout\n{r.stdout}--- stderr\n{r.stderr}", end="")
    print(f"evidence: {out}.{{cmd,stdout,stderr,exit}}")
    sys.exit(r.returncode if a.expect_exit is None else int(r.returncode != a.expect_exit))


def cmd_tui(a):
    need_bin()
    ev = evidence(a.run)
    screen = pyte.Screen(COLS, ROWS)
    stream = pyte.ByteStream(screen)
    env = child_env(a.run, a.env)  # also creates the scratch dir the pid file lives in
    app_log = scratch(a.run) / "data" / "lazyxrp.log"
    log_start = app_log.stat().st_size if app_log.exists() else 0

    def app_log_tail() -> str:
        """App log lines written by this launch only (the log is shared within a run)."""
        if not app_log.exists():
            return ""
        data = app_log.read_bytes()
        return data[log_start if len(data) >= log_start else 0 :].decode(errors="replace")

    pid, fd = pty.fork()
    if pid == 0:
        # Size the PTY before exec so the app never sees a 0×0 terminal.
        ioctl(0, TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        os.execve(str(BIN), [str(BIN), *shlex.split(a.args)], env)
    log = open(ev / f"{a.name}.steps.log", "w")

    # crossterm/ratatui-image probe the terminal (cursor position, DA1); pyte answers them.
    def reply(data: str):
        log.write(f"{time.strftime('%H:%M:%S')}   term-reply {data!r}\n")
        os.write(fd, data.encode())

    screen.write_process_input = reply
    with open(scratch(a.run) / "pids", "a") as f:
        f.write(f"{pid}\n")
    def pump(seconds: float):
        end = time.time() + seconds
        while time.time() < end:
            r, _, _ = select.select([fd], [], [], max(0.0, min(0.1, end - time.time())))
            if r:
                try:
                    stream.feed(os.read(fd, 65536))
                except OSError:
                    return

    def text() -> str:
        return "\n".join(line.rstrip() for line in screen.display)

    def holds(target: str) -> bool:
        """`log:X` = X in this launch's app log, `!X` = X not on screen, else X on screen."""
        if target.startswith("log:"):
            return target[4:] in app_log_tail()
        if target.startswith("!"):
            return target[1:] not in text()
        return target in text()

    def until(target: str, seconds: float) -> bool:
        end = time.time() + seconds
        while time.time() < end and not holds(target):
            pump(0.2)
        return holds(target)

    def split_timeout(arg: str, default: str) -> tuple[str, float]:
        m = re.search(r"@(\d+(?:\.\d+)?)$", arg)
        return (arg[: m.start()], float(m.group(1))) if m else (arg, float(default))

    failed = None
    for step in a.step:
        kind, _, arg = step.partition(":")
        log.write(f"{time.strftime('%H:%M:%S')} {step}\n")
        if kind == "key":
            os.write(fd, KEYS.get(arg, arg).encode())
            pump(0.3)
        elif kind == "press":
            # Resends are logged: a dropped key is a regression of the TUI stdin-race fix
            # (SKILL.md Gotchas), so it must stay visible in the evidence, never hidden.
            key, _, rest = arg.partition("=>")
            target, t = split_timeout(rest, "3")
            if holds(target):
                failed = f"{step}: {target!r} already holds before the key, so it proves nothing"
                break
            for attempt in range(1, 4):
                os.write(fd, KEYS.get(key, key).encode())
                if until(target, t):
                    break
                log.write(f"{time.strftime('%H:%M:%S')}   retry {attempt}: {target!r} not reached\n")
            else:
                failed = f"{step}: {target!r} not reached after 3 presses"
                break
        elif kind == "sleep":
            pump(float(arg))
        elif kind == "wait":
            target, t = split_timeout(arg, "30")
            if not until(target, t):
                failed = f"{step} timed out after {t}s"
                break
        elif kind == "snap":
            (ev / f"{a.name}.{arg}.screen.txt").write_text(text() + "\n")
        else:
            failed = f"unknown step {step!r}"
            break
    (ev / f"{a.name}.final.screen.txt").write_text(text() + "\n")

    def reap(timeout: float):
        end = time.time() + timeout
        while time.time() < end:
            done, status = os.waitpid(pid, os.WNOHANG)
            if done:
                return os.waitstatus_to_exitcode(status)
            try:
                pump(0.1)
            except OSError:
                time.sleep(0.1)
        return None

    code = reap(1)
    if code is None:
        try:
            os.write(fd, b"\x03")  # Ctrl-c always quits (DESIGN.md §6)
        except OSError:
            pass
        code = reap(3)
    if code is None:
        os.kill(pid, signal.SIGKILL)
        code = f"killed ({reap(3)})"
    (ev / f"{a.name}.app.log").write_text(app_log_tail())
    log.write(f"exit={code}\n{'FAIL ' + failed if failed else 'PASS'}\n")
    log.close()
    print(text())
    print(f"exit={code} {'FAIL: ' + failed if failed else 'PASS'}\nevidence: {ev}/{a.name}.*")
    sys.exit(1 if failed else 0)


def cmd_clean(a):
    s = scratch(a.run)
    pids = s / "pids"
    for p in (pids.read_text().split() if pids.exists() else []):
        if _alive(int(p)):
            os.kill(int(p), signal.SIGTERM)
            print(f"killed {p}")
    shutil.rmtree(s, ignore_errors=True)
    print(f"removed {s}; evidence kept at {REPO / 'target' / 'verify' / a.run}")


def cmd_recipes(a):
    """Run every ```sh block of each feature file verbatim, one bash process per file."""
    env = {**os.environ, "VERIFY_RUN": a.run, "H": str(Path(__file__).resolve())}
    results = []
    for md in a.files:
        blocks = re.findall(r"^[ \t]*```sh\n(.*?)^[ \t]*```", Path(md).read_text(), re.M | re.S)
        script = "set -euo pipefail\n" + "\n".join(
            f"echo '>>> {Path(md).stem} block {i}' >&2\n{textwrap.dedent(b)}" for i, b in enumerate(blocks, 1)
        )
        r = subprocess.run(["bash", "-c", script], cwd=REPO, env=env)
        results.append((md, len(blocks), r.returncode))
    for md, n, rc in results:
        print(f"[{'PASS' if rc == 0 else 'FAIL'}] {md} ({n} blocks)")
    sys.exit(0 if all(rc == 0 for *_, rc in results) else 1)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("build").set_defaults(fn=cmd_build)
    for name, fn in (("doctor", cmd_doctor), ("cli", cmd_cli), ("tui", cmd_tui), ("clean", cmd_clean),
                     ("recipes", cmd_recipes)):
        s = sub.add_parser(name)
        s.add_argument("--run", default=os.environ.get("VERIFY_RUN", "default"))
        s.set_defaults(fn=fn)
        if name == "doctor":
            s.add_argument("--network", default="testnet")
        if name == "recipes":
            s.add_argument("files", nargs="+", help="features/*.md")
        if name in ("cli", "tui"):
            s.add_argument("--name", required=True, help="evidence file prefix")
            s.add_argument("--env", action="append", default=[], help="KEY=VAL for the child")
        if name == "cli":
            s.add_argument("--timeout", type=float, default=180)
            s.add_argument("--expect-exit", type=int, help="pass when exit equals this (e.g. refusal = 1)")
            s.add_argument("args", nargs=argparse.REMAINDER)
        if name == "tui":
            s.add_argument("--args", default="--network testnet", help="lazyxrp argv as one string")
            s.add_argument("--step", action="append", default=[],
                           help="wait:T[@s] | press:K=>T[@s] | key:K | sleep:S | snap:NAME;  T = TEXT on screen, !TEXT gone, log:TEXT in app log")
    a = p.parse_args()
    if getattr(a, "args", None) and isinstance(a.args, list) and a.args[:1] == ["--"]:
        a.args = a.args[1:]
    a.fn(a)


if __name__ == "__main__":
    main()
