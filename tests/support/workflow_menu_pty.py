"""Usage: python3 workflow_menu_pty.py /absolute/path/to/slate

Exercise workflow selection, group Back, retained selection and Tools exit in a
private PTY. No personal config, package installation or native tool execution.
"""
import errno
import argparse
import fcntl
import json
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

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary")
parser.add_argument("scenario", nargs="?", choices=("workflow", "ghostty", "non-path", "non-path-en", "tool-error", "tool-error-en", "tool-error-sync", "tool-error-sync-en", "inventory", "inventory-en", "kitty-check", "kitty-check-en", "alacritty-check", "alacritty-check-en"), default="workflow")
parser.add_argument("--columns", type=int, choices=(20, 40, 80, 140), default=140)
args = parser.parse_args()
binary = str(Path(args.binary).resolve(strict=True))
ghostty_case = args.scenario == "ghostty"
non_path = args.scenario.startswith("non-path")
error_case = args.scenario.startswith("tool-error")
english = args.scenario.endswith("-en")
sync_error = args.scenario.startswith("tool-error-sync")
terminal_case = args.scenario.startswith(('kitty-check', 'alacritty-check'))
checked_tool = 'alacritty' if args.scenario.startswith('alacritty-check') else 'kitty'
checked_label = checked_tool.capitalize()
with tempfile.TemporaryDirectory(prefix="slate-workflow-pty-") as directory:
    if ghostty_case or error_case or non_path or terminal_case:
        config = Path(directory) / ".config/slate"
        config.mkdir(parents=True)
        (config / "current").write_text("nord\n")
    if non_path:
        (Path(directory) / ".config/starship.toml").write_text('[character]\nsuccess_symbol = ">"\n')
    if terminal_case:
        bin_dir = Path(directory) / 'bin'
        bin_dir.mkdir()
        tool = bin_dir / checked_tool
        tool.write_text('#!/bin/sh\nprintf called > "$HOME/UNEXPECTED_KITTY"\nexit 91\n')
        tool.chmod(0o755)
        managed = config / ('managed/kitty/theme.conf' if checked_tool == 'kitty' else 'managed/alacritty/colors.toml')
        managed.parent.mkdir(parents=True)
        managed.write_text('foreground #ffffff\n' if checked_tool == 'kitty' else '[colors.primary]\nforeground = "#ffffff"\n')
        terminal_config = Path(directory) / ('.config/kitty/kitty.conf' if checked_tool == 'kitty' else '.config/alacritty/alacritty.toml')
        terminal_config.parent.mkdir()
        terminal_config.write_text(f'include {managed.parent}/\n\\theme.conf\nallow_remote_control socket-\n\\only\nlisten_on unix:/fixture/kitty\n' if checked_tool == 'kitty' else f"[general]\nimport = ['{managed}']\n")
    if error_case:
        bin_dir = Path(directory) / "bin"
        bin_dir.mkdir()
        tool = bin_dir / "btop"
        tool.write_text('#!/bin/sh\nprintf called > "$HOME/UNEXPECTED_TOOL"\nexit 91\n')
        tool.chmod(0o755)
        personal = Path(directory) / ".config/btop/btop.conf"
        personal.parent.mkdir()
        personal.write_text("PRIVATE_OVERSIZED_CONFIG\n")
        with personal.open("r+b") as fixture:
            fixture.truncate(8 * 1024 * 1024 + 1)
    def snapshot():
        return {str(path.relative_to(directory)): (path.read_bytes() if path.is_file() else None, path.stat().st_mode)
                for path in Path(directory).rglob("*")}
    before = snapshot()
    env = {"HOME": directory, "SLATE_HOME": directory, "PATH": "", "TERM": "xterm-256color", "NO_COLOR": "1"}
    if english:
        env["SLATE_LANGUAGE"] = "en"
    if terminal_case:
        env['PATH'] = str(bin_dir)
    if non_path:
        env["SLATE_LANGUAGE"] = "en" if english else "zh-CN"
    if error_case:
        env["PATH"] = str(bin_dir)
        env["SLATE_LANGUAGE"] = "en" if english else "zh-CN"
    inventory = json.loads(subprocess.check_output(
        [binary, "tools", "list", "--json"], env=env, cwd=directory, timeout=8
    ))
    detected = sum(tool["available"] is True for tool in inventory["tools"])
    pid, terminal = pty.fork()
    if pid == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 40, args.columns, 0, 0))
        os.chdir(directory)
        os.execve(binary, [binary, "tools"], env)
    output = bytearray()
    transcript = bytearray()
    reaped = False
    try:
        fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 40, args.columns, 0, 0))

        def expect(text, wrapped=True):
            needle = text.encode()
            pattern = re.compile(b'(?:\r\n)?'.join(re.escape(bytes([byte])) for byte in needle)) if wrapped else re.compile(re.escape(needle))
            deadline = time.monotonic() + 8
            while pattern.search(output) is None:
                if time.monotonic() > deadline:
                    raise AssertionError(f"timeout waiting for {text!r}: {output!r}")
                if select.select([terminal], [], [], 0.1)[0]:
                    try:
                        chunk = os.read(terminal, 65536)
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                        chunk = b""
                    if not chunk:
                        raise AssertionError(f"early exit waiting for {text!r}: {output!r}")
                    output.extend(chunk)
                    transcript.extend(chunk)
            del output[:pattern.search(output).end()]

        def choose_down(count):
            os.write(terminal, b"\x1b[B" * count + b"\r")

        def choose_from_top(index):
            # Do not assume that returning from a report resets selection.
            os.write(terminal, b"\x1b[H")
            choose_down(index)

        expect("Choose a Tool" if english else "选择工具或查看全部支持")
        if terminal_case:
            ids = [tool['id'] for tool in inventory['tools'] if tool['available']]
            choose_down(ids.index(checked_tool))
            for state in (('connected', 'missing', 'invalid') if checked_tool == 'alacritty' else ('connected', 'missing')):
                connected = state == 'connected'
                expect('● Check Theme Configuration' if english else '● 检查配色配置')
                expect('└')
                if not connected:
                    terminal_config.write_text(f'include {managed}\n\\-different\nallow_remote_control socket-only\nlisten_on unix:/fixture/kitty\n' if checked_tool == 'kitty' else '[general]\nimport = []\n' if state == 'missing' else 'secret = "PRIVATE_TOML"\ninvalid [')
                    before = snapshot()
                screen_start = len(transcript)
                os.write(terminal, b'\r')
                expect(f'{checked_tool} doctor')
                errors, warnings = (1, 0) if state == 'invalid' else (0, 0 if connected else 1)
                expect(f'File checks: {errors} errors · {warnings} to review' if english else f'文件检查：{errors} 项错误 · {warnings} 项待确认')
                if not connected:
                    expect(('No direct Slate color include found' if english else '未找到直接引用 Slate 配色的 include') if checked_tool == 'kitty' else ('Invalid Alacritty TOML' if english else 'Alacritty TOML 语法错误') if state == 'invalid' else ('No Slate colors in the effective import list' if english else '有效导入列表中未找到 Slate 配色文件'))
                expect(('Full report: slate doctor ' if english else '完整结果与补充说明：slate doctor ') + checked_tool)
                expect('Back to Tool' if english else '返回工具页面')
                expect('└')
                assert transcript[screen_start:].count(b'\x1b[?1049h') == 1
                fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack('HHHH', 10, args.columns, 0, 0))
                expect('↑↓')
                expect('└')
                os.write(terminal, b'\x1b[F')
                expect(('Full report: slate doctor ' if english else '完整结果与补充说明：slate doctor ') + checked_tool)
                expect('└')
                os.write(terminal, b'\x1b[H')
                expect(f'{checked_tool} doctor')
                expect('└')
                fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack('HHHH', 40, args.columns, 0, 0))
                expect('└')
                assert snapshot() == before
                assert b'PRIVATE_TOML' not in transcript
                os.write(terminal, b'\x1b')
                expect('\x1b[?1049l')
            expect('● Check Theme Configuration' if english else '● 检查配色配置')
            expect('└')
            os.write(terminal, b'\x1b')
            expect('Choose a Tool' if english else '选择工具或查看全部支持')
            choose_from_top(detected + 2)
            expect('Choose a Theme Check' if english else '选择要检查的配色配置')
            choose_from_top(8 if checked_tool == 'kitty' else 9)
            expect(f'{checked_tool} doctor')
            expect('Back to Checks' if english else '返回检查列表')
            expect('└')
            os.write(terminal, b'\x1b')
            expect(f'● {checked_label}')
            expect('└')
            os.write(terminal, b'\x1b')
            expect('Choose a Tool' if english else '选择工具或查看全部支持')
        elif args.scenario.startswith('inventory'):
            for back in (b'\r', b'\x1b'):
                offset = len(transcript)
                choose_from_top(detected + 3)
                expect('Tool Inventory · No theme selected' if english else '工具总览 · 未选择主题', wrapped=True)
                expect('Back to Tools' if english else '返回工具菜单', wrapped=True)
                expect('└')
                report = transcript[offset:].decode()
                if args.columns >= 40:
                    for tool in inventory['tools']:
                        assert tool['label'] in report, tool['id']
                    assert ('Detection only; active colors are not verified.' if english else '仅检测结果，不代表配色已生效。') in report.replace('\r\n', '')
                assert 'Discover one tool and its next steps' not in report
                assert '\x1b[?1049h' in report
                assert snapshot() == before
                # Every row remains reachable when the report is taller than
                # the terminal, including after shrinking an already open page.
                offset = len(transcript)
                fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack('HHHH', 10, args.columns, 0, 0))
                expect('↑↓')
                expect('└')
                os.write(terminal, b'\x1b[B' * 50)
                deadline = time.monotonic() + 8
                while select.select([terminal], [], [], 0.2)[0]:
                    assert time.monotonic() < deadline, 'scroll output did not settle'
                    chunk = os.read(terminal, 65536)
                    assert chunk, 'inventory exited while scrolling'
                    transcript.extend(chunk)
                output.clear()
                scrolled = transcript[offset:].decode().replace('\r\n', '')
                for tool in inventory['tools']:
                    assert tool['label'] in scrolled, f"unreachable inventory row: {tool['id']}"
                offset = len(transcript)
                # At the bottom these inputs are inert. Up is the final
                # observable acknowledgement: exactly one redraw is expected.
                os.write(terminal, b'\x1b[B' * 4 + b'x\x1b[200~\r\x1b\x1b[201~\x1b[A')
                expect('└')
                assert transcript[offset:].count(b'\x1b[2J') == 1, 'inert input repainted the report'
                assert snapshot() == before
                fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack('HHHH', 40, args.columns, 0, 0))
                expect('└')
                offset = len(transcript)
                os.write(terminal, back)
                expect('● Tool Inventory' if english else '● 查看工具总览')
                expect('└')
                assert b'\x1b[?1049l' in transcript[offset:]
        elif non_path:
            item = next(tool for tool in inventory["tools"] if tool["id"] == "starship")
            assert item["available"] is True, item
            if item["detection"]["kind"] == "configuration":
                assert item["detection"]["executable_in_path"] is None, item
                status_label = "config found" if english else "仅找到配置"
            else:
                # Host fallback locations are legitimately searched despite
                # empty PATH. Never hide or execute the user's installed tool.
                assert item["detection"]["kind"] == "executable", item
                assert item["detection"]["executable_in_path"] is False, item
                status_label = "outside PATH" if english else "找到程序，但不在 PATH 中"
            ids = [tool["id"] for tool in inventory["tools"] if tool["available"]]
            choose_down(ids.index("starship"))
            for refreshed in [False, True]:
                expect(status_label)
                expect("Starship · Choose an Action" if english else "Starship · 选择操作")
                expect("Back to Tools" if english else "返回工具菜单")
                expect("└")
                assert b"Install This Tool" not in transcript
                assert "安装此工具".encode() not in transcript
                assert snapshot() == before, "non-PATH inspection changed files"
                if not refreshed:
                    choose_from_top(3)  # Preview, Sync, Check, Refresh.
            choose_from_top(4)  # Details is read-only, including outside-PATH tools.
            expect('Full report: slate tools info starship' if english else '完整说明：slate tools info starship', wrapped=True)
            expect('Back to Tool' if english else '返回工具页面', wrapped=True)
            expect('└')
            assert snapshot() == before
            os.write(terminal, b'\x1b')
            expect('● Details' if english else '● 查看详细信息')
            expect('└')
            os.write(terminal, b"\x1b")
            expect("Choose a Tool" if english else "选择工具或查看全部支持")
        elif error_case:
            ids = [tool["id"] for tool in inventory["tools"] if tool["available"]]
            choose_down(ids.index("btop"))
            expect("btop · Choose an Action" if english else "btop · 选择操作")
            expect("Back to Tools" if english else "返回工具菜单")
            expect("└")
            choose_from_top(1 if sync_error else 0)
            expect("Tool action stopped" if english else "工具操作已停止")
            expect("no automatic retry" if english else "不自动重试")
            if sync_error:
                expect("Partial changes may remain" if english else "部分改动可能保留")
            else:
                expect("read-only check" if english else "此次为只读检查")
            expect("btop · Choose an Action" if english else "btop · 选择操作")
            if sync_error:
                # Failed writes return to a safe recommended inspection, not Sync.
                expect("● Check Theme Configuration" if english else "● 检查配色配置")
            else:
                expect("● Preview Sync" if english else "● 预览同步改动")
            expect("└")
            assert b"PRIVATE_OVERSIZED_CONFIG" not in transcript
            assert b"file size limit exceeded" in transcript
            warning = "Tool action stopped" if english else "工具操作已停止"
            assert transcript.count(warning.encode()) == 1, "action was retried"
            assert snapshot() == before, "failed preview changed files or ran a tool"
            # A recovery menu may outlive a resize. Hidden choices must not
            # accept Enter/Space or unexpectedly run the highlighted check.
            fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 2, args.columns, 0, 0))
            os.kill(pid, signal.SIGWINCH)
            expect("Window too small" if english else "窗口太小")
            os.write(terminal, b"\r ")
            time.sleep(0.1)
            fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 40, args.columns, 0, 0))
            os.kill(pid, signal.SIGWINCH)
            expect("btop · Choose an Action" if english else "btop · 选择操作")
            expected_choice = ("● Check Theme Configuration" if english else "● 检查配色配置") if sync_error else ("● Preview Sync" if english else "● 预览同步改动")
            expect(expected_choice)
            expect("└")
            assert transcript.count(warning.encode()) == 1, "hidden menu input retried the action"
            assert snapshot() == before, "hidden menu input changed files or ran a tool"
            os.write(terminal, b"\x1b")
            expect("Choose a Tool" if english else "选择工具或查看全部支持")
        elif ghostty_case:
            ids = [tool["id"] for tool in inventory["tools"] if tool["available"]]
            choose_down(ids.index("ghostty"))
            expect("Ghostty · 选择操作")
            expect("返回工具菜单")
            for unwanted in [b"Installation:", b"Detected via", b"Next steps:", b"Open Guided Setup",
                             b"sync terminal config", b"may reload running windows and reapply"]:
                assert unwanted not in transcript, f"unrequested detail leaked: {unwanted!r}"
            assert "查看详细信息".encode() in transcript
            assert "同步可能重载终端窗口".encode() in transcript
            choose_from_top(0)  # Preview: no sync confirmation or writes.
            expect("同步预览")
            expect("● 返回工具页面")
            assert "可能刷新正在运行的窗口".encode() in transcript
            assert b"sync terminal config" not in transcript
            assert snapshot() == before, "preview changed profile files"
            os.write(terminal, b"\x1b")
            expect("Ghostty · 选择操作")
            expect("● 预览同步改动")
            expect("返回工具菜单")
            choose_from_top(1)  # Sync, independently of the recommended row.
            expect("同步预览")
            expect("完整说明：slate tools sync ghostty --dry-run")
            expect("确认同步上述工具的配色？")
            expect("● 暂不同步")
            expect("○ 确认同步")
            assert b"Compatibility checks run only after confirmation" not in transcript
            assert "中途失败不会自动回滚".encode() in transcript
            assert snapshot() == before, "sync review changed profile files"
            os.write(terminal, b"\x1b")
            expect("Ghostty · 选择操作")
            expect("返回工具菜单")
            choose_from_top(2)  # Read-only configuration check.
            expect("Ghostty · 文件检查")
            expect("未修改文件、启动校验或重载窗口")
            expect("完整文件检查：slate doctor ghostty --files-only")
            expect("● 返回工具页面")
            assert snapshot() == before, "configuration check changed profile files"
            os.write(terminal, b"\x1b")
            expect("Ghostty · 选择操作")
            expect("● 检查配色配置")
            expect("返回工具菜单")
            choose_from_top(4)  # Details.
            expect("完整说明：slate tools info ghostty", wrapped=True)
            expect("● 返回工具页面")
            assert snapshot() == before, "diagnostic report changed profile files"
            os.write(terminal, b"\r")
            expect("Ghostty · 选择操作")
            expect("● 查看详细信息")
            expect("返回工具菜单")
            choose_from_top(5)  # Back.
            expect("选择工具或查看全部支持")
        else:
            choose_down(detected + 1)
            expect("你想改善哪一部分？")
            expect("返回工具菜单")
            for index, label in enumerate([
                "终端窗口", "命令提示符与 Shell", "文件与系统", "开发工具", "分屏会话"
            ]):
                choose_from_top(index)
                expect(f"{label} · 工具")
                expect("返回用途分类")
                assert snapshot() == before, f"{label}: browsing wrote files"
                os.write(terminal, b"\x1b")
                expect("你想改善哪一部分？")
                expect(f"● {label}")
                expect("返回工具菜单")
            choose_from_top(4)
            expect("分屏会话 · 工具")
            expect("返回用途分类")
            group_tool = next(tool for tool in inventory["tools"] if tool["id"] in ("tmux", "zellij"))
            detail = json.loads(subprocess.check_output(
                [binary, "tools", "info", group_tool["id"], "--json"], env=env, cwd=directory, timeout=8
            ))
            actions = []
            if detail["theme"] is None and detail["theme_selection_available"]:
                actions.append("theme")
            if detail["sync_review_available"]:
                actions.extend(["preview", "sync"])
            if group_tool["id"] == "zellij":
                actions.append("check")
            if group_tool["available"] is False and detail["installation"]["guided_install"]:
                actions.append("install")
            actions.extend(["refresh", "details", "back"])
            choose_down(0)
            expect(f'{group_tool["label"]} · 选择操作')
            expect("返回分屏会话")
            choose_down(actions.index("back") - actions.index(detail["recommended_action"]["action"]))
            expect("分屏会话 · 工具")
            expect("返回用途分类")
            choose_down(2)
            expect("你想改善哪一部分？")
            # Previous group is index four; one Down must now select Back.
            choose_down(1)
            expect("选择工具或查看全部支持")
        if args.scenario == 'workflow':
            expect("● 按用途找工具")
        # Exit relative to a known position, not an assumed reset after Back.
        os.write(terminal, b"\x1b[H")
        choose_down(detected + 5)
        deadline = time.monotonic() + 8
        while True:
            finished, status = os.waitpid(pid, os.WNOHANG)
            if finished:
                reaped = True
                assert os.waitstatus_to_exitcode(status) == 0
                break
            if time.monotonic() > deadline:
                raise AssertionError("Tools did not exit")
            if select.select([terminal], [], [], 0.05)[0]:
                try:
                    os.read(terminal, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
        assert snapshot() == before, "browsing changed profile files"
        flags = termios.tcgetattr(terminal)[3]
        assert flags & termios.ECHO and flags & termios.ICANON, "terminal raw mode leaked"
        print((f"PASS: {checked_label} file checks {'English' if english else 'Chinese'}; both menu entries, retained selection, no native calls or writes" if terminal_case else f"PASS: inventory {'English' if english else 'Chinese'}; Enter/Esc restore screen and selection; no writes" if args.scenario.startswith('inventory') else f"PASS: {status_label} stays distinct from PATH commands after refresh; no writes" if non_path else "PASS: tool preview error stays navigable, no retry or writes" if error_case else "PASS: Ghostty concise page, explicit Details, visible Back; no writes" if ghostty_case
               else "PASS: workflow Back retains selection; no profile files created")
              + f"; {args.columns} columns; terminal modes restored")
    finally:
        if not reaped:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.close(terminal)
            os.waitpid(pid, 0)
        else:
            os.close(terminal)
