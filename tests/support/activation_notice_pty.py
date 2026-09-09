"""Check installed CLI theme-save output, or the private tmux notice fixture.

All subprocesses use a disposable HOME and executable tripwires; no real tool
or desktop reload is used. --fixture expects the lib-test executable, not Slate.
"""
import argparse
import errno
import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary')
parser.add_argument('--fixture', action='store_true')
args = parser.parse_args()
binary = str(Path(args.binary).resolve(strict=True))

for case in (['inactive', 'failed'] if args.fixture else ['hub', 'redirected']):
    with tempfile.TemporaryDirectory(prefix='slate-activation-notice-') as directory:
        root = Path(directory)
        bin_dir = root / '.local/bin'
        bin_dir.mkdir(parents=True)
        env = {'HOME': directory, 'PATH': str(bin_dir), 'TERM': 'xterm-256color', 'NO_COLOR': '1'}
        if args.fixture:
            reason = 'No such file or directory' if case == 'inactive' else 'Permission denied'
            tool = bin_dir / 'tmux'
            tool.write_text(f"#!/bin/sh\nprintf 'error connecting to /tmp/private-test/default ({reason})\\n' >&2\nexit 1\n")
            tool.chmod(0o755)
            env['SLATE_TMUX_NOTICE_FIXTURE'] = '1'
            command = [binary, '--ignored', '--exact', '--nocapture',
                       'cli::apply::notice_tests::tmux_notice_child']
        else:
            env['SLATE_HOME'] = directory
            for name in ['opencode', 'btop']:
                tool = bin_dir / name
                tool.write_text('#!/bin/sh\nprintf called > "$HOME/UNEXPECTED_TOOL"\nexit 91\n')
                tool.chmod(0o755)
            config = root / '.config/slate'
            config.mkdir(parents=True)
            (config / 'current').write_text('nord\n')
            (config / 'config.toml').write_text('[sound]\nenabled = false\n')
            (root / '.config/opencode').mkdir()
            (root / '.config/opencode/tui.json').write_text('{}\n')
            command = [binary] if case == 'hub' else [binary, 'theme', 'nord']
        if case == 'redirected':
            result = subprocess.run(command, env=env, cwd=root, capture_output=True, timeout=10)
            assert result.returncode == 0, result.stderr
            text = result.stderr.decode()
            assert 'info: opencode:' in text and 'info: btop:' in text, text
        else:
            pid, fd = pty.fork()
            if pid == 0:
                os.chdir(root)
                os.execve(binary, command, env)
            reaped = False
            output = bytearray()
            try:
                fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))

                def drain():
                    if select.select([fd], [], [], .05)[0]:
                        try:
                            output.extend(os.read(fd, 65536))
                        except OSError as error:
                            if error.errno != errno.EIO:
                                raise

                def expect(text, offset=0):
                    deadline = time.monotonic() + 10
                    while text.encode() not in output[offset:]:
                        assert time.monotonic() < deadline, (case, text, output)
                        drain()

                if case == 'hub':
                    expect('想调整什么？')
                    expect('└')
                    os.write(fd, b'\r')
                    expect('s 保存配对')
                    offset = len(output)
                    os.write(fd, b'\r')
                    expect('想调整什么？', offset)
                    expect('└', offset)
                    os.write(fd, b'\x1b[A\r')
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    drain()
                    done, status = os.waitpid(pid, os.WNOHANG)
                    if done:
                        reaped = True
                        assert os.waitstatus_to_exitcode(status) == 0, (case, output)
                        drain()
                        break
                assert reaped, (case, 'did not exit', output)
                text = output.decode(errors='replace')
                assert 'info: ' not in text, text
                if case == 'hub':
                    assert '\x1b[7m' not in text, 'theme confirmation flashed reverse video'
                if case == 'failed':
                    assert 'warning: tmux: Failed to reload tmux' in text, text
                else:
                    assert 'warning: ' not in text, text
                if args.fixture:
                    assert '/tmp/private-test' not in text, text
                    assert not (root / '.config').exists()
            finally:
                if not reaped:
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    os.waitpid(pid, 0)
                os.close(fd)
        if not args.fixture:
            assert (config / 'current').read_text().strip() == 'nord'
            assert (root / '.config/btop/themes/slate-sync.theme').is_file()
            assert 'system' in (root / '.config/opencode/tui.json').read_text()
            assert not (root / 'UNEXPECTED_TOOL').exists()
        print(f'PASS: activation output {case}; private profile only')
