"""Run an isolated generated loader in a real Zsh PTY; never load user rc files."""
import errno
import os
import pty
import select
import signal
import sys
import time

pid, fd = pty.fork()
if pid == 0:
    os.execv(sys.argv[1], [sys.argv[1], '-dfi', '-c',
        'source "$1"; for attempt in {1..100}; do '
        '[[ -s "$SLATE_TEST_PROBES/notify" ]] && break; /bin/sleep 0.02; done; '
        '/bin/sleep 0.05; jobs; print SLATE_PTY_DONE',
        'slate-private-startup', sys.argv[2]])
output = bytearray()
status = None
try:
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if select.select([fd], [], [], 0.1)[0]:
            try:
                chunk = os.read(fd, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break
            if not chunk:
                break
            output.extend(chunk)
    else:
        raise RuntimeError('Zsh startup timed out')
    _, status = os.waitpid(pid, 0)
    assert os.waitstatus_to_exitcode(status) == 0, output
    sys.stdout.buffer.write(output)
finally:
    if status is None:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        os.waitpid(pid, 0)
    os.close(fd)
