#!/usr/bin/env python3
"""M3 check: input routing in the polyfill, with this script as the terminal.

It runs `hotty run -- python3 examples/form.py` on a pty whose size says
cells are 10x20 px, answers the polyfill's probe, then clicks and types the
way a terminal would report them (SGR-pixel mouse; plain keys, and the kitty
keyboard protocol with event types), and reads the events the program
logged. Nothing here renders; it tests who gets what.

    python3 tests/m3_form.py [path/to/hotty]

The form comes from the HOTTY repository: HOTTY_DIR, or a checkout next to
this one.
"""
import fcntl
import json
import os
import select
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HOTTY = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "hotty")


def hotty_dir():
    if os.environ.get("HOTTY_DIR"):
        return os.path.abspath(os.environ["HOTTY_DIR"])
    for c in (os.path.join(ROOT, "..", "hotty"), os.path.join(ROOT, "..", "..", "hotty", "main")):
        if os.path.isfile(os.path.join(c, "SPEC.md")):
            return os.path.abspath(c)
    sys.exit("no HOTTY checkout: set HOTTY_DIR, or clone neuroplastio/hotty next to this repository")


def kitty(code, shifted=None, mods=1):
    key = f"{code}:{shifted}" if shifted else f"{code}"
    return f"\x1b[{key};{mods}u".encode(), f"\x1b[{key};{mods}:3u".encode()


SHIFT = (b"\x1b[57441;2u", b"\x1b[57441;1:3u")
KITTY_EMAIL = b"".join(
    [*kitty(97)]
    + [SHIFT[0], kitty(50, 64, 2)[0], kitty(50, 64, 2)[1], SHIFT[1]]
    + [*kitty(98), *kitty(46), *kitty(99)]
)


def main():
    log = tempfile.NamedTemporaryFile(prefix="hotty-m3-", suffix=".jsonl", delete=False).name
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 1000, 600))
    env = dict(os.environ, HOTTY_LOG=log + ".shim")
    proc = subprocess.Popen(
        [HOTTY, "run", "--scale", "1", "--transport", "direct", "--", "python3", "examples/form.py", "--log", log],
        cwd=hotty_dir(),
        stdin=slave,
        stdout=slave,
        stderr=slave,
        start_new_session=True,
        env=env,
    )
    os.close(slave)
    out = bytearray()
    done = threading.Event()

    def pump():
        # Play the terminal's part in the probe: answer XTVERSION and DA1.
        while not done.is_set():
            r, _, _ = select.select([master], [], [], 0.05)
            if not r:
                continue
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            if not data:
                break
            out.extend(data)
            if b"\x1b[>q" in data:
                os.write(master, b"\x1bP>|harness(1.0)\x1b\\")
            if b"\x1b[c" in data:
                os.write(master, b"\x1b[?62;22c")

    t = threading.Thread(target=pump, daemon=True)
    t.start()

    def events():
        try:
            with open(log) as f:
                return [json.loads(l) for l in f if l.strip()]
        except FileNotFoundError:
            return []

    def wait_for(pred, what, timeout=8.0):
        end = time.time() + timeout
        while time.time() < end:
            if pred(events()):
                return
            time.sleep(0.05)
        raise SystemExit(f"FAIL: timed out waiting for {what}; log: {json.dumps(events(), indent=1)}")

    def send(b, pause=0.15):
        os.write(master, b)
        time.sleep(pause)

    def click(x, y, button=0):
        # SGR-pixels: 1-based pixel coordinates.
        send(f"\x1b[<{button};{x + 1};{y + 1}M".encode(), 0.05)
        send(f"\x1b[<{button};{x + 1};{y + 1}m".encode())

    wait_for(lambda ev: any(e["kind"] == "ready" for e in ev), "the form")
    time.sleep(0.3)
    send(b"hello")
    # Only the primary button clicks (SPEC §10.1): a middle or right click,
    # on the checkbox or outside the form, neither toggles it nor moves the
    # keyboard, so what follows still reaches the form.
    for button in (1, 2):
        click(166, 146, button)
        click(500, 590, button)
    send(b"\t")  # to email: name reports `change`
    # The email as a terminal types it under kitty flags 1|2|4|8, as plx
    # asks: every key a CSI u press and release, Shift a key of its own, '@'
    # Shift+2 with its shifted key. Shift types nothing and reaches the
    # program; a typed key's release goes where its press went.
    send(KITTY_EMAIL)
    click(166, 146)  # the checkbox (row 3): email reports `change`, notify `change`
    click(186, 226)  # Save: `click` and `submit`
    wait_for(lambda ev: any(e.get("e") == "submit" for e in ev), "submit")
    send(b"\x1b", 0.3)  # Esc is the program's: it gives the keyboard back
    # A press with Alt held is the program's (SPEC §9.2): an Alt click on
    # the checkbox toggles nothing, and the surface reports nothing of it.
    n = len(events())
    click(166, 146, 8)
    alt_events = [e for e in events()[n:] if e["kind"] == "event"]
    send(b"q", 0.3)  # now 'q' reaches the program, which quits
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    done.set()

    ev = events()
    got = [(e.get("e"), e.get("t")) for e in ev if e["kind"] == "event"]
    keys = "".join(e["data"] for e in ev if e["kind"] == "keys")
    failures = []

    def expect(cond, msg):
        if not cond:
            failures.append(msg)

    def detail(kind, target):
        for e in ev:
            if e.get("e") == kind and e.get("t") == target:
                return e.get("detail") or {}
        return None

    expect(detail("change", "name") == {"value": "hello"}, f"change name=hello, got {detail('change', 'name')}")
    expect(detail("change", "email") == {"value": "a@b.c"}, f"change email, got {detail('change', 'email')}")
    expect((detail("change", "notify") or {}).get("checked") is True, f"notify checked, got {detail('change', 'notify')}")
    toggles = [e for e in ev if e.get("e") == "change" and e.get("t") == "notify"]
    expect(len(toggles) == 1, f"only the left click toggles notify, got {toggles}")
    expect(alt_events == [], f"an Alt click reached the surface: {alt_events}")
    expect(("click", "save") in got, "click on save")
    sub = detail("submit", "settings") or {}
    expect(sub.get("name") == "hello" and sub.get("email") == "a@b.c", f"submit fields, got {sub}")
    expect(sub.get("theme") == "dark" and sub.get("notify") == "yes", f"submit fields, got {sub}")
    expect("hello" not in keys and "a@b.c" not in keys, f"typed text leaked to the program: {keys!r}")
    kitty_keys = [k for k in keys.split("\x1b") if k.endswith("u")]
    want = [s.decode()[1:] for s in SHIFT]
    expect(kitty_keys == want, f"the program heard {kitty_keys} of the kitty keys, want Shift only: {want}")
    expect("\x1b" in keys and "q" in keys, f"Esc and q must reach the program: {keys!r}")
    expect(any(e["kind"] == "exit" for e in ev), "the program exited on q")
    for f in (log, log + ".shim"):
        try:
            os.unlink(f)
        except OSError:
            pass
    print("events:", got)
    print("program keys:", repr(keys))
    if failures:
        print("FAIL:\n  " + "\n  ".join(failures))
        sys.exit(1)
    print("OK: typing stayed in the surface; the program heard change, click and submit.")


if __name__ == "__main__":
    main()
