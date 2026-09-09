"""Exercise circular navigation on real Slate menus in disposable profiles."""
import argparse
import errno
import fcntl
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

parser = argparse.ArgumentParser()
parser.add_argument('binary')
cases = ['hub', 'tools', 'prompt', 'font', 'setup', 'tiny', 'resize', 'paste', 'ctrl-c', 'about', 'about-interrupt', 'auto', 'auto-refresh', 'auto-pair']
parser.add_argument('--case', choices=cases)
parser.add_argument('--columns', type=int, choices=(20, 40, 80, 120), default=120)
args = parser.parse_args()
binary = str(Path(args.binary).resolve(strict=True))
UP, DOWN, ENTER = b'\x1b[A', b'\x1b[B', b'\r'
ansi = re.compile(r'\x1b\[[0-?]*[ -/]*[@-~]')

for case in [args.case] if args.case else cases:
    with tempfile.TemporaryDirectory(prefix='slate-menu-wrap-') as directory:
        root = Path(directory)
        config = root / '.config/slate'
        config.mkdir(parents=True)
        (config / 'current').write_text('nord\n')
        (config / 'config.toml').write_text('[sound]\nenabled = false\n')
        if case == 'font':
            fonts = root / ('Library/Fonts' if sys.platform == 'darwin' else '.local/share/fonts')
            fonts.mkdir(parents=True)
            (fonts / 'AAASlateMenuNerdFont-Regular.ttf').write_bytes(b'\x00\x01\x00\x00private-menu-font-fixture')
            (config / 'current-font').write_text('AAASlateMenu Nerd Font\n')

        def settings():
            return {str(p.relative_to(root)): (p.read_bytes(), p.stat().st_mode)
                    for p in root.rglob('*') if p.is_file() and '.cache' not in p.parts}

        before = settings()
        env = {'HOME': directory, 'SLATE_HOME': directory, 'PATH': '', 'SLATE_LANGUAGE': 'zh-CN',
               'TERM': 'xterm-256color', 'SHELL': '/bin/zsh', 'NO_COLOR': '1'}
        pid, fd = pty.fork()
        if pid == 0:
            # Set geometry before Slate's first frame, not in a racing parent.
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 40, args.columns, 0, 0))
            os.chdir(directory)
            os.execve(binary, [binary, *([] if case in ['hub', 'tiny', 'resize', 'paste', 'ctrl-c', 'about', 'about-interrupt', 'auto', 'auto-refresh', 'auto-pair'] else [case])], env)
        reaped = False
        output = bytearray()
        transcript = bytearray()
        try:
            def resize(rows):
                fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', rows, args.columns, 0, 0))

            resize(40)

            def read_output():
                try:
                    chunk = os.read(fd, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    chunk = b''
                transcript.extend(chunk)
                return chunk

            def expect(text):
                needle = text.encode()
                deadline = time.monotonic() + 8
                while needle not in output:
                    if time.monotonic() >= deadline:
                        raise AssertionError(f'{case}: waiting for {text!r}: {output!r}')
                    if select.select([fd], [], [], .1)[0]:
                        chunk = read_output()
                        assert chunk, f'{case}: exited before {text!r}: {output!r}'
                        output.extend(chunk)
                end = output.index(needle) + len(needle)
                result = output[:end].decode(errors='replace')
                del output[:end]
                return ansi.sub('', result)

            def move(key, label):
                # Consume the previous frame before checking the new selection.
                os.write(fd, key)
                frame = expect('● ' + label)
                return frame + expect('└')

            if case in ['tiny', 'resize']:
                expect('想调整什么？')
                expect('└')
                resize(2)
                # Enter delivered after resize must redraw, not activate the
                # formerly visible first choice using stale dimensions.
                os.write(fd, ENTER)
                expect('窗口太小')
                if case == 'tiny':
                    # If Enter secretly opens the theme page, Esc returns to
                    # the hub instead of exiting, so the exit check fails.
                    os.write(fd, ENTER + b'\x1b')
                else:
                    resize(12)
                    move(b'\x1b[F', '退出')
                    resize(40)
                    frame = expect('└')
                    assert '○ 切换主题' in frame and '● 退出' in frame, frame
                    move(b'\x1b[H', '切换主题')
                    # Navigation after resize is allowed; confirmation is not.
                    move(UP, '退出')
                    move(DOWN, '切换主题')
                    move(UP, '退出')
                    os.write(fd, ENTER)
            elif case == 'ctrl-c':
                expect('想调整什么？')
                expect('└')
                os.write(fd, b'\x03')
            elif case == 'paste':
                expect('想调整什么？')
                expect('└')
                # Pasted navigation + Enter must not quit or launch a subpage.
                os.write(fd, b'\x1b[200~' + UP + ENTER + b'\n hjkl\t' + b'\x1b[201~')
                move(UP, '退出')
                move(DOWN, '切换主题')
                move(b'\x1b[F', '退出')
                move(b'\x1b[H', '切换主题')
                move(UP, '退出')
                os.write(fd, ENTER)
            elif case == 'about-interrupt':
                expect('想调整什么？')
                expect('└')
                move(UP, '退出')
                move(UP, '关于 Slate')
                os.write(fd, ENTER)
                expect('返回首页')
                expect('└')
                os.write(fd, b'\x03')
            elif case == 'about':
                header = expect('想调整什么？')
                assert re.search(r'│  主题    Nord\r?\n│  透明度  [^\r\n]+\r?\n│  字体    ', header), header
                assert '◆ 主题' not in header and '◆ 透明度' not in header, header
                expect('└')
                move(UP, '退出')
                move(UP, '关于 Slate')
                os.write(fd, ENTER)
                page = expect('└')
                assert '主题    ' in page and '内置支持数量' in page, page
                assert 'Source tag:' not in page and 'Executable:' not in page, page
                move(DOWN, '诊断详情')
                os.write(fd, ENTER)
                details = expect('返回关于')
                assert '源码标识' in details and '程序路径' in details, details
                assert '不代表已同步远端' in details and 'slate about' in details, details
                assert 'Cargo profile' not in details and 'Prompt styles:' not in details, details
                expect('└')
                os.write(fd, b'\x1b')
                expect('返回首页')
                expect('└')
                os.write(fd, b'\x1b')
                expect('想调整什么？')
                expect('● 关于 Slate')
                expect('└')
                os.write(fd, b'\x1b')
            elif case == 'auto-pair':
                expect('想调整什么？')
                expect('└')
                move(DOWN * 4, '自动换色：关')
                os.write(fd, ENTER)
                expect('返回首页')
                expect('└')
                move(DOWN, '选择深浅主题')
                os.write(fd, ENTER)
                expect('选择深色主题')
                expect('└')
                os.write(fd, ENTER)
                expect('选择浅色主题')
                expect('└')
                os.write(fd, b'\x1b')
                expect('选择深色主题')
                expect('└')
                os.write(fd, b'\x1b')
                expect('● 选择深浅主题')
                expect('└')
                assert settings() == before, 'pairing cancellation changed settings'
                os.write(fd, b'\x1b')
                expect('想调整什么？')
                expect('● 自动换色：关')
                expect('└')
                os.write(fd, b'\x1b')
            elif case in ('auto', 'auto-refresh'):
                expect('想调整什么？')
                expect('└')
                move(DOWN * 4, '自动换色：关')
                os.write(fd, ENTER)
                expect('返回首页')
                expect('└')
                move(DOWN * 2, '检查自动换色')
                for step in range(2):
                    os.write(fd, ENTER)
                    summary = expect('自动换色诊断') + expect('● 返回自动换色')
                    expect('└')
                    assert settings() == before, 'inspection changed settings'
                    if case == 'auto':
                        assert 'Helper directory:' not in summary, 'technical details leaked into normal summary'
                        # Full paths and technical details are opt-in. Switching
                        # views always defaults to Back rather than another action.
                        move(DOWN, '查看完整诊断')
                        os.write(fd, ENTER)
                        expect('Auto-theme doctor')
                        expect('Helper directory:')
                        expect('● 返回自动换色')
                        expect('└')
                        assert settings() == before, 'full diagnostics changed settings'
                        move(DOWN, '查看摘要')
                        os.write(fd, ENTER)
                        expect('自动换色诊断')
                        expect('● 返回自动换色')
                        expect('└')
                        assert settings() == before, 'summary refresh changed settings'
                    if case == 'auto-refresh':
                        # Hold the full report open while another writer changes
                        # preferences; changing views must recapture, not cache it.
                        move(DOWN, '查看完整诊断')
                        os.write(fd, ENTER)
                        expect('Auto-theme doctor')
                        expect('● 返回自动换色')
                        expect('└')
                        assert settings() == before, 'full report changed settings'
                        (config / 'config.toml').write_text(
                            '[auto_theme]\nenabled = true\n' if step == 0 else '[broken')
                        before = settings()
                        move(DOWN, '查看摘要')
                        os.write(fd, ENTER)
                        refreshed = expect('自动换色诊断') + expect('● 返回自动换色')
                        expect('└')
                        # Both fixtures require diagnostic detail: the isolated
                        # enabled profile lacks a launcher; the second is corrupt.
                        assert ('Enabled: yes' if step == 0 else 'Enabled: unknown') in refreshed, refreshed
                        assert 'Warning:' in refreshed, refreshed
                        if step == 1:
                            assert 'preference cannot be read; no default is inferred' in refreshed, refreshed
                            assert '已保存设置：已关闭' not in refreshed
                        assert settings() == before, 'view switch rewrote external settings'
                    os.write(fd, ENTER if step == 0 else b'\x1b')
                    frame = expect('● 检查自动换色')
                    expect('└')
                    if case == 'auto-refresh':
                        assert settings() == before, 'inspection rewrote external settings'
                        if step == 0:
                            assert '关闭自动换色' in frame, frame
                        else:
                            assert '无法读取自动换色设置' in frame, frame
                            assert '开启自动换色' not in frame and '关闭自动换色' not in frame, frame
                os.write(fd, b'\x1b')
                expect('想调整什么？')
                expect('└')
                os.write(fd, b'\x1b')
            elif case == 'hub':
                expect('想调整什么？')
                expect('└')
                move(UP, '退出')
                move(DOWN, '切换主题')
                move(b'\x1b[6~', '退出')
                move(b'\x1b[5~', '切换主题')
                move(UP, '退出')
                os.write(fd, ENTER)
            elif case == 'tools':
                expect('选择工具或查看全部支持')
                frame = expect('└')
                first = re.search(r'● ([^\r\n]+)', frame).group(1).split(' (')[0].strip()
                move(UP, '退出工具菜单')
                move(DOWN, first)
                move(b'\x1b[F', '退出工具菜单')
                move(b'\x1b[H', first)
                resize(12)
                frame = move(UP, '退出工具菜单')
                positions = re.findall(r'选择工具或查看全部支持 · (\d+)/(\d+)', frame)
                assert positions and positions[-1][0] == positions[-1][1], frame
                os.write(fd, ENTER)
            elif case == 'font':
                expect('选择字体：')
                frame = expect('└')
                assert not (root / '.cache/slate').exists(), 'font browsing created cache/lock files'
                rows = re.findall(r'[●○] ([^\r\n]+)', frame)
                assert re.search(r'● [^\r\n]*AAASlateMenu', frame), 'saved installed font was not selected'
                index, row = next((i, row) for i, row in enumerate(rows) if '(下载)' in row)
                label = row.split('(下载)')[0] + '(下载)'
                move(b'\x1b[H' + DOWN * index, label)
                for cancel in [ENTER, b'\x1b']:
                    os.write(fd, ENTER)
                    expect('● 暂不下载')
                    expect('└')
                    move(DOWN, '预览配置改动')
                    os.write(fd, ENTER)
                    expect('字体配置预览')
                    expect('仅预览：未安装、未写文件')
                    expect('● 暂不下载')
                    expect('└')
                    assert settings() == before, 'font preview changed settings'
                    move(cancel, label)
                    assert not (root / '.cache/slate').exists(), 'declining created cache/lock files'
                installed_index, row = next((i, row) for i, row in enumerate(rows) if 'AAASlateMenu' in row)
                installed_label = row.split(' (')[0].strip()
                move(b'\x1b[H' + DOWN * installed_index, installed_label)
                for cancel in [ENTER, b'\x1b']:
                    os.write(fd, ENTER)
                    expect('● 暂不更换')
                    expect('└')
                    move(DOWN, '预览配置改动')
                    os.write(fd, ENTER)
                    expect('字体配置预览')
                    expect('仅预览：未安装、未写文件')
                    expect('● 暂不更换')
                    expect('└')
                    assert settings() == before, 'installed-font preview changed settings'
                    move(cancel, installed_label)
                    assert not (root / '.cache/slate').exists(), 'installed-font decline created cache/lock files'
                os.write(fd, b'\x1b')
            elif case == 'prompt':
                expect('选择提示符样式')
                expect('└')
                move(UP, '返回上级')
                move(DOWN, '彩虹分段')
                os.write(fd, ENTER)
                expect('样式预览')
                expect('└')
                move(UP, '返回上级')
                move(DOWN, '查看改动并确认')
                move(UP, '返回上级')
                os.write(fd, ENTER)
            else:
                expect('Setup mode:')
                expect('└')
                move(UP, 'Manual')
                move(DOWN, 'Quick')
                move(UP, 'Manual')
                os.write(fd, ENTER)
                expect('Tools:')
                frame = expect('└')
                labels = re.findall(r'│  ◻ ([^\r\n]+)', frame)
                assert len(labels) >= 2, frame
                first, last = (label.split(' (')[0].strip() for label in (labels[0], labels[-1]))
                # A framed paste must neither toggle nor submit the multiselect.
                os.write(fd, b'\x1b[200~' + UP + b' ' + ENTER + b'\x1b[201~')
                os.write(fd, UP + b' ')
                expect('◼ ' + last)
                expect('└')
                os.write(fd, DOWN + b' ')
                expect('◼ ' + first)
                frame = expect('└')
                assert '◼ ' + last in frame, 'wrapping lost the last checked item'
                os.write(fd, b'\x1b')

            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                done, status = os.waitpid(pid, os.WNOHANG)
                if done:
                    reaped = True
                    assert os.waitstatus_to_exitcode(status) == (130 if case in ['setup', 'ctrl-c', 'about-interrupt'] else 0), (case, status)
                    break
                if select.select([fd], [], [], .05)[0]:
                    read_output()
            assert reaped, f'{case}: did not exit'
            # Drain final cleanup output even if waitpid won the race above.
            while select.select([fd], [], [], .05)[0]:
                if not read_output():
                    break
            flags = termios.tcgetattr(fd)[3]
            assert flags & termios.ECHO and flags & termios.ICANON, f'{case}: raw mode leaked'
            for enabled, disabled, name in [
                (b'\x1b[?2004h', b'\x1b[?2004l', 'bracketed paste'),
                (b'\x1b[?25l', b'\x1b[?25h', 'hidden cursor'),
            ]:
                assert transcript.rfind(enabled) >= 0, f'{case}: {name} was not enabled'
                assert transcript.rfind(disabled) > transcript.rfind(enabled), f'{case}: {name} leaked'
            assert settings() == before, f'{case}: browsing changed settings'
            if case in ('about', 'auto-pair'):
                assert b'Operation cancelled' not in transcript, 'Back must not render cancellation as failure'
            if case in ('about', 'about-interrupt'):
                assert transcript.count(b'\x1b[?1049h') == 1, 'read-only screen not entered exactly once'
                assert transcript.count(b'\x1b[?1049l') == 1, 'read-only screen not restored exactly once'
                start = transcript.index(b'\x1b[?1049h')
                end = transcript.index(b'\x1b[?1049l')
                assert start < end
                assert transcript[start:end].count(b'\x1b[2J') == (3 if case == 'about' else 1), 'read-only pages were not replaced'
            behavior = ('previews installed/catalog fonts, declines on Enter/Esc and retains selection' if case == 'font' else
                        'returns from pairing through auto-theme to the hub without saving' if case == 'auto-pair' else
                        'rejects hidden confirmation' if case == 'tiny' else
                        'refreshes external auto-theme state without writes' if case == 'auto-refresh' else
                        'retains auto-theme inspection and returns on Esc' if case == 'auto' else
                        'ignores framed paste' if case == 'paste' else
                        'keeps overview compact and details opt-in' if case == 'about' else
                        'cancels cleanly' if case == 'ctrl-c' else 'wraps both ways')
            print(f'PASS: {case} {behavior}; {args.columns} columns; terminal modes restored; settings preserved')
        finally:
            if not reaped:
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                os.waitpid(pid, 0)
            os.close(fd)
