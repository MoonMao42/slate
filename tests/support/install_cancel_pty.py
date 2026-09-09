"""Cancel a missing Yazi install in a private profile; never confirm installation."""
import errno
import fcntl
import itertools
import os
from pathlib import Path
import pty
import select
import signal
import struct
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1]).resolve(strict=True))
for language, (key, expected) in itertools.product(
        ['zh-CN', 'en'], [(b"\r", 0), (b"\x1b", 0), (b"\x03", 130)]):
    with tempfile.TemporaryDirectory(prefix="slate-install-cancel-") as directory:
        pid, fd = pty.fork()
        if pid == 0:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 100, 0, 0))
            os.chdir(directory)
            os.execve(binary, [binary, "tools", "install", "yazi"], {
                "HOME": directory, "SLATE_HOME": directory, "PATH": "",
                "TERM": "xterm-256color", "NO_COLOR": "1", "SLATE_LANGUAGE": language})
        output = bytearray()
        sent = False
        reaped = False
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if select.select([fd], [], [], 0.05)[0]:
                    try:
                        chunk = os.read(fd, 65536)
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                        chunk = b""
                    output.extend(chunk)
                cancel = '● 取消' if language == 'zh-CN' else '● Cancel'
                if not sent and cancel.encode() in output:
                    if language == 'zh-CN':
                        for phrase in ['安装此工具及所需依赖？', '安装方式:', '安装不等于启用', '没有 Slate 快照或自动回滚', 'SLATE_HOME']:
                            assert phrase.encode() in output, (phrase, output)
                    else:
                        assert b'Install only this tool' in output and b'no Slate snapshot or automatic rollback' in output
                        assert '暂不安装'.encode() not in output
                    assert b'Esc' not in output
                    # Never send Enter unless a default decline was observed.
                    os.write(fd, key)
                    sent = True
                done, status = os.waitpid(pid, os.WNOHANG)
                if done:
                    reaped = True
                    assert sent, "missing-tool confirmation not available in this environment"
                    assert os.waitstatus_to_exitcode(status) == expected, (status, output)
                    assert not list(Path(directory).iterdir()), "cancel created profile files"
                    if expected == 0:
                        assert b"Operation cancelled" not in output
                    flags = termios.tcgetattr(fd)[3]
                    assert flags & termios.ECHO and flags & termios.ICANON
                    print(f"PASS: {language} installation cancel {key!r}, exit {expected}, no profile writes")
                    break
            assert reaped, output
        finally:
            if not reaped:
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                os.waitpid(pid, 0)
            os.close(fd)
