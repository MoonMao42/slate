"""Exercise installed Slate's setup entry/cancel routes in private PTYs.

Use --case=NAME (repeatable) to run only related routes. Confirmation routes
still require --default-no; case selection never bypasses that safety guard.
"""
import errno
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

binary = str(Path(sys.argv[1]).resolve(strict=True))
chinese = '--zh' in sys.argv[2:]
translations = {
    '✓ Preflight Checks': '环境检查',
    'Setup paused until these blockers are fixed:': '设置已暂停，请先处理以下问题：',
    'Switch to zsh, bash, or fish': '请切换到 zsh、bash 或 fish 后重试。',
    'Setup mode:': '设置方式：',
    'Pick a vibe:': '选择风格：',
    'Tools:': '工具：',
    'Font:': '字体：',
    'Theme:': '主题：',
    'Show system info every time you open a terminal?': '每次打开终端时显示系统信息？',
    'Review and confirm:': '确认执行以上设置？',
    '● Cancel': '● 取消',
    'Setup canceled': '已取消设置',
    'Skip (keep current font)': '保留当前字体',
    '● Skip (keep current font)': '● 保留当前字体',
    'Keep current theme': '保留当前主题',
    'Keep current (Personal Fixture Mono)': '保留当前字体 (Personal Fixture Mono)',
    'Ready to apply? This will create backups first.': '确认后先备份配置，再执行设置。',
    'opacity 0.85': '不透明度 0.85',
    'opacity 1.00': '不透明度 1.00',
    'Tool Inventory': '工具列表',
    'System monitor in your theme · reopen btop after changes': '系统监控配色 · 修改后需重新打开',
    'Select tools to install or configure; selection is optional.': '选择要安装或配置的工具；可不选。',
}
modes = ['entry', 'quick', 'quick-back-manual', 'quick-interrupt', 'manual', 'quick-review', 'manual-review', 'blocked-shell', 'output-redirect']
modes += ['manual-font-interrupt', 'manual-theme-interrupt', 'manual-fastfetch-interrupt']
modes += ['quick-direct-interrupt', 'quick-direct-escape']
modes += ['manual-tools-back', 'manual-tools-interrupt']
modes += ['force-entry']
modes += ['manual-tools-reset']
modes += ['manual-tools-tiny-back', 'manual-tools-tiny-interrupt']
# Only run Enter-at-confirmation against a build whose default-No contract is
# being verified; never accidentally exercise an older default-Yes installer.
if '--default-no' in sys.argv[2:]:
    modes += ['quick-direct-back-default-no']
    modes += ['quick-review-back-default-no']
    modes += ['manual-review-back-default-no']
    modes += ['quick-default-no', 'manual-default-no', 'manual-personal-default-no', 'manual-theme-back-default-no', 'manual-font-back-default-no', 'manual-fastfetch-back-default-no', 'manual-theme-light-default-no', 'manual-theme-dark-default-no', 'manual-fastfetch-on-default-no', 'manual-tool-uncheck-default-no']
requested = {arg.split('=', 1)[1] for arg in sys.argv[2:] if arg.startswith('--case=')}
if requested:
    unknown = requested - set(modes)
    if unknown:
        raise SystemExit(f'Unavailable cases (confirmation requires --default-no): {sorted(unknown)}')
    modes = [mode for mode in modes if mode in requested]
for mode in modes:
    direct_quick = mode.startswith('quick-direct-')
    tool_return = mode in ['manual-font-back-default-no', 'manual-tool-uncheck-default-no']
    display_chinese = chinese and not mode.endswith('-redirect')
    with tempfile.TemporaryDirectory(prefix='slate-setup-entry-') as directory:
        root = Path(directory)
        config = root / '.config/slate'
        config.mkdir(parents=True)
        (config / 'current').write_text('nord\n')
        if mode in ['manual-personal-default-no', 'manual-theme-back-default-no']:
            ghostty = root / '.config/ghostty/config'
            ghostty.parent.mkdir(parents=True)
            ghostty.write_text('font-family = "Personal Fixture Mono"\n')
        def settings():
            return {str(p.relative_to(root)): (p.read_bytes(), p.stat().st_mode)
                    for p in root.rglob('*') if p.is_file() and '.cache' not in p.parts}
        before = settings()
        env = {'HOME': directory, 'SLATE_HOME': directory, 'PATH': '',
               'TERM': 'xterm-256color', 'TERM_PROGRAM': 'ghostty', 'SHELL': '/bin/zsh', 'NO_COLOR': '1',
               'SLATE_LANGUAGE': 'zh-CN' if chinese else 'en'}
        if mode.startswith('blocked-shell'):
            env['SHELL'] = '/fixture/unsupported-shell'
        pid, fd = pty.fork()
        if pid == 0:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 140, 0, 0))
            if mode.endswith('-redirect'):
                null = os.open(os.devnull, os.O_WRONLY)
                os.dup2(null, 1)
                os.close(null)
            os.chdir(directory)
            os.execve(binary, [binary, 'setup'] + (['--quick'] if direct_quick else []) + (['--force'] if mode == 'force-entry' else []), env)
        reaped = False
        output = bytearray()
        try:
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 140, 0, 0))
            def expect(text):
                if display_chinese:
                    text = translations.get(text, text)
                needle = text.encode()
                deadline = time.monotonic() + 10
                while needle not in output:
                    if time.monotonic() >= deadline:
                        raise AssertionError(f'{mode}: waiting for {text!r}: {output!r}')
                    if select.select([fd], [], [], .1)[0]:
                        try:
                            chunk = os.read(fd, 65536)
                        except OSError as error:
                            if error.errno != errno.EIO:
                                raise
                            chunk = b''
                        assert chunk, f'{mode}: exited before {text!r}: {output!r}'
                        output.extend(chunk)
                end = output.index(needle) + len(needle)
                matched = bytes(output[:end]).decode()
                del output[:end]
                return matched
            if mode.endswith('-redirect'):
                expect('Non-interactive setup requires --quick for explicit consent.')
            else:
                expect('✓ Preflight Checks')
            if display_chinese:
                expect('写入权限: 可写')
                expect('具体目标文件稍后检查')
                expect('个受管工具')
            if mode.startswith('blocked-shell'):
                expect('Setup paused until these blockers are fixed:')
                expect('Switch to zsh, bash, or fish')
            elif not mode.endswith('-redirect') and not direct_quick:
                if mode == 'force-entry':
                    expect('强制设置：重新选择字体和主题。' if chinese else 'Force setup: choose the font and theme again.')
                entry = expect('Setup mode:')
                plain_entry = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', entry)
                assert not re.search(r'✦\s+✦\s+slate', plain_entry), plain_entry
            if mode.startswith('quick'):
                if not direct_quick:
                    os.write(fd, b'\r')
                expect('Pick a vibe:')
                if mode == 'quick':
                    names = ['现代深色', '北欧简约', '复古暖色', '明亮浅色'] if chinese else ['Modern Dark', 'Minimal Frost', 'Retro Warm', 'Clean Light']
                    descriptions = ['柔和深色 · JetBrains Mono 字体', '清爽冷色 · Hack 字体', '温暖复古 · Iosevka Term 字体', '明亮浅色 · Fira Code 字体'] if chinese else ['Sleek dark palette with JetBrains Mono', 'Clean Nordic aesthetic with Hack font', 'Warm vintage palette with Iosevka Term', 'Bright palette with Fira Code']
                    for index, (name, description) in enumerate(zip(names, descriptions)):
                        if index:
                            os.write(fd, b'\x1b[B')
                        expect('● ' + name)
                        expect(description)
                        expect('└')
            elif mode.startswith('manual'):
                os.write(fd, b'\x1b[B\r')
                inventory = expect('Select tools to install or configure; selection is optional.')
                assert ('工具检测：' if chinese else 'Tool detection:') in inventory, inventory
                assert 'Tool Inventory' not in inventory and '工具列表' not in inventory, inventory
                assert 'System monitor in your theme' not in inventory, inventory
                expect('Tools:')
                if mode in ['manual-tools-tiny-back', 'manual-tools-tiny-interrupt']:
                    expect('└')
                    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 2, 40, 0, 0))
                    expect('窗口太小' if chinese else 'Window too small')
                    # Hidden choices must ignore both selection and submission.
                    os.write(fd, b' \r')
                    if mode == 'manual-tools-tiny-back':
                        os.write(fd, b'\x1b')
                        expect('窗口太小' if chinese else 'Window too small')
                        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 140, 0, 0))
                        expect('Setup mode:')
                        expect('● 逐项设置' if chinese else '● Manual (customize each)')
                        expect('└')
                        os.write(fd, b'\r')
                        expect('Tools:')
                        restored = expect('└')
                        assert '◼' not in restored and '■' not in restored, restored
                if mode in ['manual-tools-back', 'manual-tools-reset']:
                    expect('└')
                    os.write(fd, b'\x1b[F')
                    expect('└')
                    os.write(fd, b' ')
                    checked = expect('└')
                    os.write(fd, b'\x1b')
                    expect('Setup mode:')
                    expect('● 逐项设置' if chinese else '● Manual (customize each)')
                    expect('└')
                    if mode == 'manual-tools-reset':
                        os.write(fd, b'\x1b[H\r')
                        expect('Pick a vibe:')
                        expect('└')
                        os.write(fd, b'\x1b')
                        expect('Setup mode:')
                        expect('● 快速设置' if chinese else '● Quick (pick a vibe)')
                        expect('└')
                        os.write(fd, b'\x1b[B')
                        expect('└')
                    os.write(fd, b'\r')
                    expect('Tools:')
                    restored = expect('└')
                    # The checked row must survive leaving the page before Enter.
                    strip = lambda value: re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', value)
                    checked_rows = [line.strip() for line in strip(checked).splitlines() if '◼' in line or '◻' in line or '■' in line]
                    assert checked_rows, checked
                    if mode == 'manual-tools-reset':
                        assert '◼' not in strip(restored) and '■' not in strip(restored), restored
                    else:
                        for row in checked_rows:
                            assert row in strip(restored), (row, restored)
                        # Returning from the next page must retain focus too.
                        os.write(fd, b'\r')
                        expect('Font:')
                        expect('└')
                        os.write(fd, b'\x1b')
                        expect('Tools:')
                        expect('└')
                        os.write(fd, b' ')
                        unchecked = expect('└')
                        assert '◼' not in strip(unchecked) and '■' not in strip(unchecked), unchecked
            reviewed = mode.endswith('-review') or mode.endswith('-default-no')
            if reviewed:
                if tool_return:
                    expect('└')
                    os.write(fd, b' ')
                os.write(fd, b'\r')
                if mode.startswith('manual'):
                    submitted = expect('Font:')
                    if tool_return:
                        plain = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', submitted)
                        chosen = re.search(r'◇  (?:工具：|Tools:)\r?\n│  ([^\r\n]+)', plain)
                        assert chosen, submitted
                        selected_label = chosen.group(1).strip()
                    expect('● Skip (keep current font)')
                    expect('└')
                    if tool_return:
                        for index, key in enumerate([b'\x1b', b'\x1b[F\r']):
                            os.write(fd, key)
                            expect('Tools:')
                            expect('└')
                            uncheck = mode == 'manual-tool-uncheck-default-no' and index == 1
                            if uncheck:
                                os.write(fd, b' ')
                            os.write(fd, b'\r')
                            # The submitted receipt must retain the checkbox,
                            # not merely show this label among available rows.
                            if uncheck:
                                submitted = expect('Font:')
                                plain = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', submitted)
                                # Ignore the final active frame redraw; only
                                # inspect the committed checkbox receipt.
                                committed = plain.split('◇  工具：' if chinese else '◇  Tools:', 1)[1]
                                assert selected_label not in committed, committed
                            else:
                                expect(selected_label)
                                expect('Font:')
                            expect('● Skip (keep current font)')
                            expect('└')
                    if mode == 'manual-theme-back-default-no':
                        os.write(fd, b'\x1b[H\r')
                        expect('Theme:')
                        expect('└')
                        os.write(fd, b'\x1b')
                        expect('Font:')
                        expect('● JetBrains Mono Nerd Font')
                        expect('└')
                        # Keep the pending font once, then use the explicit
                        # Back item to return again and clear it with Skip.
                        os.write(fd, b'\r')
                        expect('Theme:')
                        expect('└')
                        os.write(fd, b'\x1b[F\r')
                        expect('Font:')
                        expect('● JetBrains Mono Nerd Font')
                        expect('└')
                        os.write(fd, b'\x1b[F\x1b[A')
                    os.write(fd, b'\r')
                    expect('Theme:')
                    expect('Keep current theme')
                    expect('└')
                    if mode in ['manual-theme-light-default-no', 'manual-theme-dark-default-no']:
                        light_target = mode == 'manual-theme-light-default-no'
                        first_name = 'Catppuccin Frappé' if light_target else 'Catppuccin Latte'
                        final_name = 'Catppuccin Latte' if light_target else 'Catppuccin Frappé'
                        # Keep-current is followed by Catppuccin variants in
                        # stable ID order: frappe, latte, macchiato, mocha.
                        os.write(fd, b'\x1b[H' + b'\x1b[B' * (1 if light_target else 2))
                        expect('● ' + first_name)
                        expect('└')
                    os.write(fd, b'\r')
                    expect('Show system info every time you open a terminal?')
                    if mode in ['manual-theme-light-default-no', 'manual-theme-dark-default-no']:
                        expect('└')
                        os.write(fd, b'\x1b')
                        expect('Theme:')
                        expect('● ' + first_name)
                        expect('└')
                        os.write(fd, b'\x1b[H' + b'\x1b[B' * (2 if light_target else 1))
                        expect('● ' + final_name)
                        expect('└')
                        os.write(fd, b'\r')
                        expect('Show system info every time you open a terminal?')
                    if mode == 'manual-fastfetch-back-default-no':
                        for key in [b'\x1b', b'\x1b[F\r']:
                            expect('└')
                            os.write(fd, key)
                            expect('Theme:')
                            expect('Keep current theme')
                            expect('└')
                            os.write(fd, b'\r')
                            expect('Show system info every time you open a terminal?')
                    os.write(fd, b'y' if mode in ['manual-fastfetch-on-default-no', 'manual-review-back-default-no'] else b'n')
                if mode in ['manual-theme-light-default-no', 'manual-theme-dark-default-no']:
                    review = expect('opacity 1.00' if light_target else 'opacity 0.85')
                    assert final_name in review, review
                if mode in ['manual-personal-default-no', 'manual-theme-back-default-no']:
                    expect('Keep current (Personal Fixture Mono)')
                if mode.startswith('quick'):
                    expect('磨砂效果' if chinese else 'frosted glass')
                review_tail = expect('Ready to apply? This will create backups first.')
                if mode == 'manual-font-back-default-no':
                    configured = re.search(r'(?:计划配色|Configure colors): ([^\r\n]+)', review_tail)
                    assert configured, review_tail
                    assert selected_label.split(' (', 1)[0] in configured.group(1), configured.group(1)
                if mode == 'manual-tool-uncheck-default-no':
                    assert selected_label.split(' (', 1)[0] not in review_tail, review_tail
                if mode.startswith('manual'):
                    startup_on = mode in ['manual-fastfetch-on-default-no', 'manual-review-back-default-no']
                    startup = ('启动系统信息: 显示' if startup_on else '启动系统信息: 不显示') if chinese else ('Startup system info: On' if startup_on else 'Startup system info: Off')
                    assert startup in review_tail, review_tail
                else:
                    assert 'Startup system info' not in review_tail and '启动系统信息' not in review_tail
                expect('Review and confirm:')
                if mode == 'quick-direct-back-default-no':
                    for index, key in enumerate([b'\x1b', b'\x1b[F\r']):
                        expect('● Cancel')
                        expect('└')
                        os.write(fd, key)
                        expect('Pick a vibe:')
                        name = ('现代深色' if index == 0 else '明亮浅色') if chinese else ('Modern Dark' if index == 0 else 'Clean Light')
                        expect('● ' + name)
                        expect('└')
                        # --quick has no parent mode page: the last preset is
                        # Clean Light, not a Back item leading to a hidden page.
                        if index == 0:
                            os.write(fd, b'\x1b[F')
                            expect('● 明亮浅色' if chinese else '● Clean Light')
                            expect('└')
                        os.write(fd, b'\r')
                        revised = expect('Ready to apply? This will create backups first.')
                        assert 'Catppuccin Latte' in revised, revised
                        assert ('不透明度 1.00' if chinese else 'opacity 1.00') in revised, revised
                        expect('Review and confirm:')
                if mode == 'quick-review-back-default-no':
                    expect('● Cancel')
                    expect('└')
                    os.write(fd, b'\x1b')
                    expect('Pick a vibe:')
                    expect('● 现代深色' if chinese else '● Modern Dark')
                    expect('└')
                    os.write(fd, b'\x1b[F\x1b[A')
                    expect('● 明亮浅色' if chinese else '● Clean Light')
                    expect('└')
                    os.write(fd, b'\r')
                    revised = expect('Ready to apply? This will create backups first.')
                    assert 'Catppuccin Latte' in revised, revised
                    assert ('不透明度 1.00' if chinese else 'opacity 1.00') in revised, revised
                    expect('Review and confirm:')
                    expect('● Cancel')
                    expect('└')
                    os.write(fd, b'\x1b[F\r')
                    expect('Pick a vibe:')
                    expect('● 明亮浅色' if chinese else '● Clean Light')
                    expect('└')
                    os.write(fd, b'\x1b')
                    expect('Setup mode:')
                    expect('└')
                    os.write(fd, b'\x1b[B\r')
                    expect('Tools:')
                    expect('└')
                    os.write(fd, b'\r')
                    expect('Font:')
                    expect('● Skip (keep current font)')
                    expect('└')
                    os.write(fd, b'\r')
                    expect('Theme:')
                    expect('Keep current theme')
                    expect('└')
                    os.write(fd, b'\r')
                    expect('Show system info every time you open a terminal?')
                    os.write(fd, b'n')
                    revised = expect('Ready to apply? This will create backups first.')
                    assert 'Catppuccin Latte' not in revised and 'Fira Code' not in revised, revised
                    assert 'Nord' in revised, revised
                    assert ('不透明度 0.85' if chinese else 'opacity 0.85') in revised, revised
                    expect('Review and confirm:')
                if mode == 'manual-review-back-default-no':
                    for index, key in enumerate([b'\x1b', b'\x1b[F\r']):
                        expect('● Cancel')
                        expect('└')
                        os.write(fd, key)
                        expect('Show system info every time you open a terminal?')
                        expect(('● 显示' if index == 0 else '● 不显示') if chinese else ('● On' if index == 0 else '● Off'))
                        expect('└')
                        os.write(fd, b'n')
                        revised = expect('Ready to apply? This will create backups first.')
                        assert ('启动系统信息: 不显示' if chinese else 'Startup system info: Off') in revised, revised
                        assert 'fastfetch' not in revised.lower(), revised
                        expect('Review and confirm:')
                if mode.endswith('-default-no'):
                    # Fail before Enter if the renderer ever defaults to Yes.
                    expect('● Cancel')
                os.write(fd, b'\r' if mode.endswith('-default-no') else b'n')
                ending = expect('Setup canceled')
                plain_ending = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', ending)
                assert ('└─ ★  已取消设置' if chinese else '└─ ★  Setup canceled') in plain_ending, plain_ending
            elif not mode.startswith('blocked-shell') and not mode.endswith('-redirect'):
                if mode in ['manual-font-interrupt', 'manual-theme-interrupt', 'manual-fastfetch-interrupt']:
                    expect('└')
                    os.write(fd, b'\r')
                    expect('Font:')
                    expect('● Skip (keep current font)')
                    expect('└')
                    if mode != 'manual-font-interrupt':
                        os.write(fd, b'\r')
                        expect('Theme:')
                        expect('Keep current theme')
                        expect('└')
                    if mode == 'manual-fastfetch-interrupt':
                        os.write(fd, b'\r')
                        expect('Show system info every time you open a terminal?')
                        expect('└')
                    os.write(fd, b'\x03')
                elif mode == 'quick-back-manual':
                    # Explicit Back returns without committing a preset. The
                    # user can then choose manual setup in the same session.
                    expect('└')
                    os.write(fd, b'\x1b[F\r')
                    expect('Setup mode:')
                    expect('└')
                    os.write(fd, b'\x1b[B\r')
                    expect('Tools:')
                    os.write(fd, b'\x1b')
                    expect('Setup mode:')
                    os.write(fd, b'\x1b')
                elif mode in ['quick-interrupt', 'quick-direct-interrupt', 'manual-tools-interrupt', 'manual-tools-tiny-interrupt']:
                    os.write(fd, b'\x03')
                else:
                    os.write(fd, b'\x1b')
                    if mode in ['quick', 'manual', 'manual-tools-back', 'manual-tools-reset', 'manual-tools-tiny-back']:
                        expect('Setup mode:')
                        os.write(fd, b'\x1b')
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                # Continue draining final redraws while waiting. A resized
                # terminal can otherwise fill the PTY and block child exit.
                if select.select([fd], [], [], .02)[0]:
                    try:
                        output.extend(os.read(fd, 65536))
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                done, status = os.waitpid(pid, os.WNOHANG)
                if done:
                    reaped = True
                    assert os.waitstatus_to_exitcode(status) == (1 if mode.startswith('blocked-shell') or mode.endswith('-redirect') else 0 if reviewed else 130), (mode, status, output)
                    break
                time.sleep(.02)
            assert reaped, f'{mode}: cancellation did not exit'
            flags = termios.tcgetattr(fd)[3]
            assert flags & termios.ECHO, f'{mode}: terminal echo was not restored'
            assert flags & termios.ICANON, f'{mode}: canonical input was not restored'
            after = settings()
            changed = [name for name in sorted(before.keys() | after.keys())
                       if before.get(name) != after.get(name)]
            assert not changed, f'{mode}: cancellation changed settings: {changed}'
            if mode.endswith('-redirect'):
                assert not (root / '.cache').exists(), 'redirected setup created a cache/lock'
            print(f'PASS: setup {mode} cancellation; settings preserved')
        finally:
            if not reaped:
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                # Release the terminal before reaping a failing child, too.
                os.close(fd)
                os.waitpid(pid, 0)
            else:
                os.close(fd)

with tempfile.TemporaryDirectory(prefix='slate-setup-no-tty-') as directory:
    result = subprocess.run([binary, 'setup'], env={
        'HOME': directory, 'SLATE_HOME': directory, 'PATH': '',
        'SLATE_LANGUAGE': 'zh-CN' if chinese else 'en',
    }, input=b'', capture_output=True, timeout=5, cwd=directory)
    assert result.returncode == 1, result
    assert b'Non-interactive setup requires --quick' in result.stderr, result
    assert not list(Path(directory).iterdir()), 'noninteractive setup created files'
    print('PASS: noninteractive setup rejected without creating files')
