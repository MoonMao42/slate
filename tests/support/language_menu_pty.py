"""First-run language consent and later changes, using a disposable profile."""
import errno
import fcntl
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
import tomllib

binary = str(Path(sys.argv[1]).resolve(strict=True))

def run(root, scenario):
    pid, fd = pty.fork()
    if pid == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 100, 0, 0))
        os.chdir(root)
        arguments = [binary, 'prompt'] if scenario.startswith('direct-') else [binary, '--quiet']
        os.execve(binary, arguments, {'HOME': str(root), 'SLATE_HOME': str(root),
            'TERM': 'xterm-256color', 'PATH': '', 'NO_COLOR': '1'})
    pending = bytearray()
    reaped = False
    def expect(text):
        needle = text.encode()
        deadline = time.monotonic() + 8
        while needle not in pending:
            assert time.monotonic() < deadline, (text, pending)
            if select.select([fd], [], [], .05)[0]:
                data = os.read(fd, 65536)
                assert data, (text, pending)
                pending.extend(data)
        end = pending.index(needle) + len(needle)
        seen = bytes(pending[:end])
        del pending[:end]
        return seen
    try:
        if scenario.startswith('direct-'):
            title = 'Choose Prompt Style' if scenario == 'direct-English' else '选择提示符样式'
            intro = expect(title)
            assert '语言 / Language'.encode() not in intro
            expect('└')
            os.write(fd, b'\x1b')
        elif scenario == 'about-interrupt':
            expect('想调整什么？')
            expect('└')
            os.write(fd, b'\x1b[F\x1b[A\r')
            expect('● 返回首页')
            expect('└')
            os.write(fd, b'\x1b[A\r')
            expect('完整报告：slate about')
            expect('返回关于')
            expect('└')
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 10, 40, 0, 0))
            expect('诊断详情 · ↑↓')
            expect('└')
            os.write(fd, b'\x1b[F')
            expect('完整报告：slate about')
            expect('└')
            os.write(fd, b'\x03')
            expect('\x1b[?25h')
            expect('\x1b[?1049l')
        elif scenario == 'quit-English':
            intro = expect('What would you like to change?')
            assert b'Enter Open' not in intro and b'Esc Quit' not in intro
            expect('└')
            os.write(fd, b'\x1b[F\r')
            expect('Goodbye')
        elif scenario.startswith('recovery-'):
            intro = expect('An unfinished preview needs review')
            assert b'slate' in intro and b'Preview Recovery' in intro
            assert b'PRIVATE_BROKEN_RECORD' not in intro
            menu = expect('└')
            assert b'Keep Current Files' in menu and b'Quit' in menu
            assert b'Restore Pre-Preview Files' not in menu
            assert b'Review File Changes' not in menu
            assert '● Quit'.encode() in menu
            os.write(fd, b'\x1b' if scenario == 'recovery-escape' else b'\r')
        elif scenario.startswith('repair-'):
            expect('想调整什么？')
            expect('└')
            os.write(fd, b'\x1b[B' * 5 + b'\r')
            expect('语言 / Language')
            expect('└')
            os.write(fd, b'\x1b[B' * 3 + b'\r')
            expect('● 中文')
            expect('└')
            before = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            if scenario == 'repair-cancel':
                os.write(fd, b'\x1b')
                expect('想调整什么？')
                expect('└')
                assert before == {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            else:
                os.write(fd, b'\x1b[B\r')
                expect('What would you like to change?')
                expect('└')
                path = root / '.config/slate/config.toml'
                saved = path.read_text()
                assert tomllib.loads(saved)['preferences']['language'] == 'en'
                assert '# personal' in saved and '# retain' in saved
                assert "style = 'focus'" in saved
                after = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
                assert {key for key in before.keys() | after.keys() if before.get(key) != after.get(key)} == {str(path)}
            os.write(fd, b'\x1b')
        elif scenario == 'invalid':
            intro = expect('想调整什么？')
            assert '语言设置无法读取或识别'.encode() in intro
            assert '语言 / Language'.encode() not in intro
            assert b'PRIVATE_UNKNOWN_LANGUAGE' not in intro
            expect('└')
            os.write(fd, b'\x1b[B' * 6 + b'\r')
            summary = expect('● 返回主菜单')
            assert '语言设置无法读取或识别'.encode() in summary
            assert b'PRIVATE_UNKNOWN_LANGUAGE' not in summary
            expect('└')
            os.write(fd, b'\x1b')
            expect('● 检查配置')
            expect('└')
            os.write(fd, b'\x1b')
        elif scenario == 'first-stale':
            expect('语言 / Language')
            expect('└')
            path = root / '.config/slate/config.toml'
            path.parent.mkdir(parents=True, exist_ok=True)
            replacement = "# changed elsewhere\n[preferences]\nlanguage = 'en'\n"
            path.write_text(replacement)
            os.write(fd, b'\r')
            expect('Configuration changed; choose the language again')
            assert path.read_text() == replacement
            assert not (root / '.cache').exists()
        elif scenario == 'cancel':
            expect('语言 / Language')
            expect('└')
            os.write(fd, b'\x1b')
        elif scenario == 'choose-and-change':
            expect('语言 / Language')
            expect('└')
            assert not list(root.iterdir()), 'opening picker created files'
            os.write(fd, b'\x1b[B\r')
            intro = expect('What would you like to change?')
            assert b'Enter Open' not in intro and b'Esc Quit' not in intro
            expect('Quit')
            expect('└')
            path = root / '.config/slate/config.toml'
            assert tomllib.loads(path.read_text())['preferences']['language'] == 'en'
            direct_before = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            run(root, 'direct-English')
            assert direct_before == {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            os.write(fd, b'\r')
            expect('Enter save')
            os.write(fd, b'\x1b[200~nord\r\n\x1b[201~')
            expect('Paste ignored')
            os.write(fd, b'\t')
            expect('↑↓ theme · Tab return')
            os.write(fd, b'\x1b')
            expect('What would you like to change?')
            expect('└')
            after_preview = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            changed = {key for key in direct_before.keys() | after_preview.keys() if direct_before.get(key) != after_preview.get(key)}
            # Full preview renders from this dedicated, derived Starship file;
            # it is not the user's active starship.toml or saved preferences.
            assert changed <= {str(root / '.config/slate/managed/starship/picker-preview.toml')}, changed
            os.write(fd, b'\x1b[B' * 8 + b'\r')
            overview = expect('Built-in support, not installed or active tools')
            assert '内置支持'.encode() not in overview
            expect('● Back')
            expect('└')
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 2, 100, 0, 0))
            expect('Window too small; enlarge. Esc back.')
            os.write(fd, b'\r ')
            time.sleep(.1)
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 100, 0, 0))
            expect('● Back')
            expect('└')
            os.write(fd, b'\x1b[A\r')
            details = expect('Full report: slate about')
            assert b'Executable' in details and b'Source tag' in details
            assert '源码标识'.encode() not in details
            expect('Back to About')
            expect('└')
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 10, 40, 0, 0))
            expect('Build Details · ↑↓')
            expect('└')
            os.write(fd, b'\x1b[F')
            expect('Full report: slate about')
            expect('└')
            os.write(fd, b'\x1b[H')
            expect('This Slate build')
            expect('└')
            os.write(fd, b'\x1b[200~\r\x1b\x1b[201~')
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 100, 0, 0))
            expect('Full report: slate about')
            expect('└')
            os.write(fd, b'\x1b')
            expect('● Back')
            expect('└')
            os.write(fd, b'\x1b')
            expect('● About Slate')
            expect('└')
            os.write(fd, b'\x1b[A' * 2 + b'\r')
            expect('Saved Configuration')
            summary = expect('Saved settings only; confirm live appearance in each tool.')
            assert b'Theme  Not set' in summary and b'Auto-Theme  Off' in summary
            assert '未设置'.encode() not in summary
            expect('● Back')
            expect('└')
            os.write(fd, b'\x1b[B\r')
            expect('Saved Configuration')
            expect('● Refresh')
            expect('└')
            os.write(fd, b'\x1b')
            expect('● Check Configuration')
            expect('└')
            font_before = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            os.write(fd, b'\x1b[A' * 4 + b'\r')
            expect('Select font:')
            expect('└')
            os.write(fd, b'\r')
            expect('● Cancel')
            expect('└')
            os.write(fd, b'\x1b[B\r')
            expect('Font Configuration Preview')
            expect('Preview only: no installs or writes.')
            expect('● Cancel')
            expect('└')
            os.write(fd, b'\x1b')
            expect('Select font:')
            expect('└')
            os.write(fd, b'\x1b')
            expect('● Change Font')
            expect('└')
            assert font_before == {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            os.write(fd, b'\x1b[B\r')
            expect('Choose Prompt Style')
            expect('Back')
            expect('└')
            os.write(fd, b'\x1b[B' * 4 + b'\r')
            preview = expect('Illustration only; browsing changes no files and runs no commands.')
            assert b'Focus one-line' in preview
            assert '仅样式示意'.encode() not in preview
            expect('Style Preview')
            expect('└')
            for cancel in (b'\r', b'\x1b'):
                files_before = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
                os.write(fd, b'\r')
                scope = expect('Open Global Theme Preview?')
                assert b'Preview temporarily changes detected tools' in scope
                assert b'does not save the prompt style' in scope
                expect('● Cancel')
                expect('└')
                os.write(fd, cancel)
                expect('Style Preview')
                expect('└')
                assert files_before == {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            os.write(fd, b'\x1b[F\r')
            expect('● Prompt Style')
            expect('└')
            os.write(fd, b'\x1b[A' * 2 + b'\r')
            expect('Choose a Tool')
            expect('└')
            os.write(fd, b'\x1b[F' + b'\x1b[A' * 4 + b'\r')
            expect('Choose a Workflow')
            expect('● Terminal Windows')
            expect('└')
            os.write(fd, b'\r')
            expect('Terminal Windows · Tools')
            expect('└')
            os.write(fd, b'\r')
            details = expect('Ghostty · Choose an Action')
            assert b'Live colors unverified' in details
            assert b'Saved Slate theme: Not selected' in details
            assert '未选择'.encode() not in details
            expect('└')
            os.write(fd, b'\x1b')
            expect('Terminal Windows · Tools')
            expect('● Ghostty')
            expect('└')
            os.write(fd, b'\x1b')
            expect('Choose a Workflow')
            expect('● Terminal Windows')
            expect('└')
            os.write(fd, b'\x1b')
            expect('Choose a Tool')
            expect('● Browse by Workflow')
            expect('└')
            os.write(fd, b'\x1b')
            expect('● Tool Themes')
            expect('└')
            auto_before = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            os.write(fd, b'\x1b[B' * 3 + b'\r')
            pairing = expect('● Turn On Auto-Theme')
            assert b'Saved Pairing' in pairing and b'Automatic (not pinned)' in pairing
            assert '未固定'.encode() not in pairing
            expect('└')
            for cancel in (b'\r', b'\x1b'):
                os.write(fd, b'\x1b[H\x1b[B\r')
                expect('Choose Dark Theme')
                expect('● Automatic (not pinned)')
                expect('└')
                os.write(fd, b'\r')
                expect('Choose Light Theme')
                expect('└')
                os.write(fd, b'\x1b')
                expect('Choose Dark Theme')
                expect('● Automatic (not pinned)')
                expect('└')
                os.write(fd, b'\r')
                expect('Choose Light Theme')
                expect('└')
                os.write(fd, b'\r')
                review = expect('Save this pairing?')
                assert b'Dark Mode' in review and b'Light Mode' in review
                assert '模式'.encode() not in review
                expect('● Cancel')
                expect('└')
                os.write(fd, cancel)
                expect('Pairing not saved.')
                expect('● Choose Dark and Light Themes')
                expect('└')
                assert auto_before == {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            os.write(fd, b'\x1b[B\r')
            expect('Auto-Theme Diagnostics')
            expect('● Back to Auto-Theme')
            expect('└')
            os.write(fd, b'\x1b')
            expect('● Check Auto-Theme')
            expect('└')
            os.write(fd, b'\x1b')
            expect('● Auto-Theme: Off')
            expect('└')
            assert auto_before == {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
            os.write(fd, b'\x1b[B\r')
            expect('语言 / Language')
            expect('└')
            os.write(fd, b'\x1b[B' * 3 + b'\r')
            expect('● English')
            expect('└')
            os.write(fd, b'\x1b[A\r')
            expect('想调整什么？')
            expect('└')
            assert tomllib.loads(path.read_text())['preferences']['language'] == 'zh-CN'
            os.write(fd, b'\x1b')
        else:
            intro = expect('想调整什么？')
            assert '语言 / Language'.encode() not in intro, 'saved language prompted again'
            assert 'Enter 打开'.encode() not in intro and '也可以按'.encode() not in intro
            expect('└')
            os.write(fd, b'\x1b[F\r')
            expect('已退出')
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            finished, status = os.waitpid(pid, os.WNOHANG)
            if finished:
                reaped = True
                assert os.waitstatus_to_exitcode(status) == (1 if scenario == 'first-stale' else 130 if scenario == 'about-interrupt' else 0)
                break
            if select.select([fd], [], [], .05)[0]:
                try:
                    os.read(fd, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
        assert reaped, 'language flow did not exit'
        flags = termios.tcgetattr(fd)[3]
        assert flags & termios.ECHO and flags & termios.ICANON
    finally:
        if not reaped:
            os.kill(pid, signal.SIGKILL)
            os.close(fd)
            os.waitpid(pid, 0)
        else:
            os.close(fd)

with tempfile.TemporaryDirectory(prefix='slate-language-stale-') as directory:
    run(Path(directory), 'first-stale')

with tempfile.TemporaryDirectory(prefix='slate-language-') as directory:
    root = Path(directory)
    run(root, 'cancel')
    assert not list(root.iterdir())
    run(root, 'direct-Chinese')
    assert not list(root.iterdir())
    run(root, 'choose-and-change')
    before = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
    run(root, 'reopen')
    run(root, 'about-interrupt')
    run(root, 'direct-Chinese')
    assert before == {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
    assert not (root / '.config/slate/current').exists()
    assert not (root / '.zshrc').exists()
    (root / '.config/slate/config.toml').write_text('[preferences]\nlanguage = "PRIVATE_UNKNOWN_LANGUAGE"\n')
    before = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
    run(root, 'invalid')
    assert before == {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
    (root / '.config/slate/config.toml').write_text("# personal\n[preferences]\nlanguage = 42 # retain\n[prompt]\nstyle = 'focus'\n")
    run(root, 'repair-cancel')
    run(root, 'repair-save')
    run(root, 'direct-English')
    run(root, 'quit-English')
    (root / '.config/slate/config.toml').write_text('[preferences]\nlanguage = "en"\n')
    record = root / '.cache/slate/preview-session.json'
    record.parent.mkdir(parents=True, exist_ok=True)
    record.write_text('PRIVATE_BROKEN_RECORD')
    before = {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
    for scenario in ('recovery-escape', 'recovery-quit'):
        run(root, scenario)
        assert before == {str(p): p.read_bytes() for p in root.rglob('*') if p.is_file()}
    print('PASS: language selection, persistence, diagnostics and recovery exits; no theme or shell changes')
