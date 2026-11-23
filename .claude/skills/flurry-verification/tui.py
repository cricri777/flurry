#!/usr/bin/env python3
"""Drives the flurry TUI in a pseudo-terminal: python3 tui.py <flurry-binary> [keys...]; prints the final screen. TUI_ROWS sets the terminal height (default 14)."""
import fcntl
import os
import pty
import re
import select
import struct
import sys
import termios
import time

ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
CLEAR = "\x1b[2J"


def read(fd: int, wait: float) -> str:
    out, end = b"", time.time() + wait
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.1)
        if ready:
            try:
                out += os.read(fd, 65536)
            except OSError:
                break
    return out.decode(errors="replace")


def screen(raw: str) -> str:
    frame = raw.split(CLEAR)[-1]
    return "\n".join(line.rstrip() for line in ANSI.sub("", frame).splitlines()).strip("\n")


def main() -> None:
    binary, keys = sys.argv[1], sys.argv[2:]
    pid, fd = pty.fork()
    if pid == 0:
        os.execv(binary, [binary])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", int(os.environ.get("TUI_ROWS", "14")), 60, 0, 0))
    raw = read(fd, 1.0)
    for key in keys:
        os.write(fd, key.encode().decode("unicode_escape").encode())
        raw = read(fd, 0.5)
    print(screen(raw))
    os.write(fd, b"q")
    for _ in range(20):
        if os.waitpid(pid, os.WNOHANG) != (0, 0):
            return
        time.sleep(0.1)
    os.kill(pid, 9)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit("usage: tui.py <flurry-binary> [keys...]")
    main()
