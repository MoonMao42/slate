"""Capture real Neovim search/visual output in an isolated, bounded PTY."""
import errno
import fcntl
import os
import pty
import select
import signal
import struct
import sys
import termios
import time

pid, fd = pty.fork()
if pid == 0:
    os.execv(sys.argv[1], [sys.argv[1], '-u', 'NONE', '-i', 'NONE', '-n',
                          '-c', 'luafile ' + sys.argv[2]])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 100, 0, 0))
output = bytearray()
started = time.monotonic()
ready = None
keys = [(0.15, b'/match\r'), (0.3, b'Vj'), (0.55, b'\x1b:q!\r')]
status = None
try:
    while time.monotonic() - started < 5:
        if ready is not None and keys and time.monotonic() - ready >= keys[0][0]:
            os.write(fd, keys.pop(0)[1])
        if select.select([fd], [], [], 0.02)[0]:
            try:
                chunk = os.read(fd, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break
            if not chunk:
                break
            output.extend(chunk)
            if b'\x1b[6n' in chunk:
                os.write(fd, b'\x1b[1;1R')
            if ready is None and b'slate-color-ready' in output:
                ready = time.monotonic()
            if len(output) > 1024 * 1024:
                raise RuntimeError('Neovim output limit exceeded')
        done, result = os.waitpid(pid, os.WNOHANG)
        if done:
            status = result
            break
    else:
        raise RuntimeError('Neovim search/selection test timed out')
    if status is None:
        _, status = os.waitpid(pid, 0)
    assert ready is not None and not keys, 'Search and selection did not run'
    assert os.waitstatus_to_exitcode(status) == 0, output[-2000:]
    sys.stdout.buffer.write(output)
finally:
    if status is None:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        os.waitpid(pid, 0)
    os.close(fd)
