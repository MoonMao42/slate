"""Verify stale sync consent using an explicit binary and a private PTY/profile."""
import errno
import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1]).resolve(strict=True))
mode = sys.argv[2] if len(sys.argv) > 2 else "stale"
assert mode in ("stale", "english-enter", "english-escape")
english = mode.startswith("english-")
with tempfile.TemporaryDirectory(prefix="slate-sync-review-") as directory:
    root = Path(directory)
    config = root / ".config/slate"
    config.mkdir(parents=True)
    (config / "current").write_text("nord\n")
    bin_dir = root / "bin"
    bin_dir.mkdir()
    tool = bin_dir / "btop"
    tool.write_text('#!/bin/sh\nprintf called > "$HOME/UNEXPECTED_TOOL"\nexit 91\n')
    tool.chmod(0o755)
    personal = root / ".config/btop/btop.conf"
    personal.parent.mkdir()
    personal.write_text("color_theme=personal\n")
    env = {"HOME": directory, "SLATE_HOME": directory, "PATH": str(bin_dir), "TERM": "xterm-256color", "NO_COLOR": "1"}
    if english:
        env["SLATE_LANGUAGE"] = "en"
    personal.write_text("PRIVATE_OVERSIZED_CONFIG\n")
    with personal.open("r+b") as fixture:
        fixture.truncate(8 * 1024 * 1024 + 1)
    oversized = subprocess.run([binary, "tools", "sync", "btop", "--dry-run"],
                               env=env, cwd=directory, capture_output=True, timeout=8)
    assert oversized.returncode == 1, oversized.stdout + oversized.stderr
    assert os.fsencode(personal) in oversized.stderr, oversized.stderr
    assert b"file size limit exceeded" in oversized.stderr
    assert b"PRIVATE_OVERSIZED_CONFIG" not in oversized.stdout + oversized.stderr
    assert personal.stat().st_size == 8 * 1024 * 1024 + 1
    assert not (root / ".cache").exists()
    assert not (root / "UNEXPECTED_TOOL").exists()
    personal.write_text("color_theme=personal\n")
    print("PASS: oversized sync preview identifies the file and size limit without contents or writes")
    pid, terminal = pty.fork()
    if pid == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 140, 0, 0))
        os.chdir(directory)
        os.execve(binary, [binary, "tools", "sync", "btop"], env)
    output = bytearray()
    reaped = False
    try:
        fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 140, 0, 0))
        def expect(text):
            deadline = time.monotonic() + 8
            while text.encode() not in output:
                if time.monotonic() > deadline:
                    raise AssertionError(f"missing {text}: {output!r}")
                if select.select([terminal], [], [], 0.1)[0]:
                    try:
                        chunk = os.read(terminal, 65536)
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                        chunk = b""
                    assert chunk, output
                    output.extend(chunk)
        expect("Sync these tools' colors?" if english else "确认同步上述工具的配色？")
        expect("● Cancel" if english else "● 暂不同步")
        expect("└")
        if not english:
            personal.write_text("# PRIVATE_EXTERNAL_EDIT\ncolor_theme=personal\n")
        def snapshot():
            return {str(p.relative_to(root)): (p.read_bytes() if p.is_file() else None, p.stat().st_mode) for p in root.rglob("*")}
        before = snapshot()
        os.write(terminal, (b"\r" if mode == "english-enter" else b"\x1b") if english else b"\x1b[B\r")
        if not english:
            expect("configuration files changed after review")
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            finished, status = os.waitpid(pid, os.WNOHANG)
            if finished:
                reaped = True
                assert os.waitstatus_to_exitcode(status) == (0 if english else 1), status
                break
            time.sleep(0.02)
        assert reaped, "sync did not exit after cancel or stale consent"
        assert b"PRIVATE_EXTERNAL_EDIT" not in output
        assert snapshot() == before, "stale sync changed fixture files or launched a tool"
        flags = termios.tcgetattr(terminal)[3]
        assert flags & termios.ECHO and flags & termios.ICANON
        print(f"PASS: {mode}; no sync writes or tool launch; terminal modes restored")
    finally:
        if not reaped:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
        os.close(terminal)
