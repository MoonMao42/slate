"""Check setup output through real TTY routing without real installers.

Pass the compiled --lib test executable, not the installed Slate executable.
With --execute, also configure a disposable profile using simulated installers.
"""
import errno
import fcntl
import itertools
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1]).resolve(strict=True))
execute = '--execute' in sys.argv[2:]
test = ('cli::setup_executor::tests::executor_terminal_fixture' if execute else
        'cli::failure_handler::completion::tests::completion_terminal_fixture')
for language, case, redirect in itertools.product(['zh-CN', 'en'], ['success', 'failure'], ['none', 'stdout', 'stdin']):
    with tempfile.TemporaryDirectory(prefix='slate-receipt-pty-') as directory:
        pid, fd = pty.fork()
        if pid == 0:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 100, 0, 0))
            if redirect != 'none':
                null = os.open(os.devnull, os.O_RDWR)
                os.dup2(null, 1 if redirect == 'stdout' else 0)
                os.close(null)
            os.chdir(directory)
            os.execve(binary, [binary, '--exact', test, '--ignored', '--nocapture'], {
                'HOME': directory, 'SLATE_HOME': directory, 'PATH': '',
                'TERM': 'xterm-256color', 'SLATE_LANGUAGE': language,
                'TERM_PROGRAM': 'ghostty', 'SHELL': '/bin/zsh', 'NO_COLOR': '1',
                'SLATE_RECEIPT_CASE': case,
            })
        reaped = False
        output = bytearray()
        try:
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                if select.select([fd], [], [], .05)[0]:
                    try:
                        data = os.read(fd, 65536)
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                        data = b''
                    output.extend(data)
                    if not data:
                        break
            else:
                raise AssertionError(('fixture timed out', language, case, redirect, output))
            # PTY EOF can arrive just before waitpid observes process exit.
            while time.monotonic() < deadline:
                done, status = os.waitpid(pid, os.WNOHANG)
                if done == pid:
                    break
                time.sleep(.01)
            else:
                raise AssertionError('fixture closed output but did not exit')
            reaped = True
            assert os.waitstatus_to_exitcode(status) == 0, output
            text = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', output.decode()).replace('\r\n', '\n')
            receipt = text.split('RECEIPT-BEGIN\n', 1)[1].split('RECEIPT-END', 1)[0]
            if redirect == 'none':
                title = ('设置完成' if case == 'success' else '设置尚未全部完成') if language == 'zh-CN' else ('Setup complete' if case == 'success' else 'Setup incomplete')
                assert title in receipt, receipt
                assert len(receipt.splitlines()) < 12, receipt
                assert 'Visibility & Activation' not in receipt, receipt
                assert ('隔离配置' if language == 'zh-CN' else 'isolated profile') in receipt
            else:
                assert ('Setup Complete!' if case == 'success' else 'Setup finished with issues.') in receipt, receipt
                assert 'Visibility & Activation' in receipt, receipt
                assert '设置完成' not in receipt, receipt
            if case == 'failure':
                assert 'fixture download failed' in receipt, receipt
                assert 'slate setup --only bat' in receipt, receipt
            assert 'Restart Ghostty' not in receipt and '重启 Ghostty' not in receipt
            if execute:
                assert ('Configuration files updated' in text) == (redirect != 'none'), text
                if redirect == 'none':
                    assert ('正在应用设置' if language == 'zh-CN' else 'Applying your setup') in text, text
                    # Fast work need not render the asynchronous spinner. The
                    # retained final result must be complete without any delay.
                    counts = ('已安装 1 · 失败 0' if case == 'success' else '已安装 0 · 失败 1') if language == 'zh-CN' else ('Installed 1 · Failed 0' if case == 'success' else 'Installed 0 · Failed 1')
                    assert counts in receipt, receipt
                    if case == 'failure':
                        assert ('bat 已安装' if language == 'zh-CN' else 'bat installed') not in text, text
                assert not (Path(directory) / '.local/bin/bat').exists()
            else:
                assert not list(Path(directory).iterdir()), 'rendering created files'
            print(f'PASS: {language} {case} redirect={redirect}; ' + ('private executor only' if execute else 'no files created'))
        finally:
            if not reaped:
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                os.waitpid(pid, 0)
            os.close(fd)
