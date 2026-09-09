"""Native menu smoke check without rebuilding/replacing the user's Slate CLI.

Usage: python3 tests/support/prompt_menu_pty.py /absolute/path/to/lib-test-binary
Use --cli to exercise an explicitly supplied Slate binary instead of the fixture.
Both modes use a private PTY and temporary HOME.
"""

import argparse
import errno
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time
import fcntl


def check(binary, scenario, cli=False, columns=140, english=False):
    with tempfile.TemporaryDirectory(prefix="slate-prompt-pty-") as directory:
        root = Path(directory)
        config = root / ".config/slate"
        config.mkdir(parents=True)
        if scenario == "theme-unreadable":
            (config / "current").mkdir()
        elif scenario == "theme-unknown":
            (config / "current").write_text("unknown-theme\n")
        elif scenario != "theme-back":
            (config / "current").write_text("nord\n")
        (config / "config.toml").write_text(
            "[broken" if scenario in ("broken", "check-broken") else
            "[prompt]\nstyle = 'classic'\n" if scenario == "apply" else
            "[prompt]\nstyle = 'focus'\n"
        )

        def snapshot():
            return {
                str(path.relative_to(root)): (
                    path.read_bytes() if path.is_file() else None, path.stat().st_mode
                )
                for path in root.rglob("*")
            }

        env = {
            "HOME": directory,
            "SLATE_HOME": directory,
            "SLATE_PROMPT_PTY_ROOT": directory,
            "TERM": "xterm-256color",
            "NO_COLOR": "1",
            "PATH": "",
        }
        if english:
            env["SLATE_LANGUAGE"] = "en"
        if scenario == "noop":
            seed = subprocess.run([binary, "prompt", "focus", "--yes"],
                                  cwd=directory, env=env, capture_output=True,
                                  text=True, timeout=10)
            assert seed.returncode == 0, seed.stderr
            assert "Focus one-line saved." in seed.stdout
            assert "Restore point:" in seed.stderr
        before = snapshot()
        file_times = {path: path.stat().st_mtime_ns for path in root.rglob('*') if path.is_file()}
        pid, terminal = pty.fork()
        if pid == 0:
            # Set geometry before exec so the first frame uses the requested
            # width, rather than racing the parent's resize against rendering.
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 40, columns, 0, 0))
            os.chdir(directory)
            arguments = ["prompt"] if cli else ["--ignored", "--exact",
                "cli::prompt::menu::tests::prompt_menu_pty_fixture", "--nocapture"]
            if scenario.startswith("direct-"):
                arguments = ["prompt", "focus"]
            os.execve(binary, [binary, *arguments], env)
        output = bytearray()
        transcript = bytearray()
        reaped = False
        try:
            def expect(text):
                if english:
                    text = {
                        "选择提示符样式": "Choose Prompt Style",
                        "● 专注单行 · 已保存": "● Focus one-line · Saved",
                        "● 专注单行": "● Focus one-line",
                        "返回上级": "Back",
                        "仅样式示意，并非实时提示符": "Illustration only",
                        "样式预览": "Style Preview",
                        "返回样式预览": "Back to Preview",
                        "● 检查提示符配置": "● Check Prompt Configuration",
                        "完整结果与补充说明：slate doctor starship": "Full report: slate doctor starship",
                        "保存这个提示符样式？": "Save this prompt style?",
                        "● 暂不保存": "● Cancel",
                        "○ 确认保存": "○ Save",
                        "● 彩虹分段": "● Rainbow segments",
                        "已保存：经典双行": "Saved: Classic shell",
                        "不自动重试": "without retrying",
                        "无需修改：专注单行": "No changes: Focus one-line",
                        "未改写文件或新建恢复点": "no new recovery point",
                    }.get(text, text)
                deadline = time.monotonic() + 8
                pattern = re.compile(b'(?:\r\n)?'.join(re.escape(bytes([byte])) for byte in text.encode()))
                while not pattern.search(output):
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
                            raise AssertionError(f"fixture exited before {text!r}: {output!r}")
                        output.extend(chunk)
                        transcript.extend(chunk)
                end = pattern.search(output).end()
                seen = bytes(output[:end])
                del output[:end]
                return seen

            if scenario.startswith("direct-"):
                expect("保存这个提示符样式？")
                expect("● 暂不保存")
                expect("○ 确认保存")
                assert snapshot() == before
                os.write(terminal, b"\x1b" if scenario == "direct-escape" else b"\r")
            else:
                expect("选择提示符样式")
                if scenario in ("broken", "check-broken"):
                    expect("● 彩虹分段")
                    steps = 4
                elif scenario == "apply":
                    expect("● 经典双行 · 已保存")
                    steps = 1
                else:
                    expect("● 专注单行 · 已保存")
                    steps = 0
                expect("返回上级")
                os.write(terminal, b"\x1b[B" * steps + b"\r")
                expect("~/project >")
                expect("仅样式示意，并非实时提示符")
                if scenario in ("theme-back", "theme-unknown"):
                    expect("尚未选定可用主题")
                    expect("不会自动选择默认主题")
                elif scenario == "theme-unreadable":
                    expect("已保存主题无法安全读取")
                    expect("slate status")
                expect("样式预览")
                if scenario == "theme-unreadable":
                    menu_text = expect("返回上级").decode()
                    assert "先选择主题" not in menu_text
                    assert "查看改动并确认" not in menu_text
                    assert snapshot() == before
                    os.write(terminal, b"\x1b[B" * 3 + b"\r")
                elif scenario in ("theme-back", "theme-unknown"):
                    expect("返回上级")
                    for key in (b"\r", b"\x1b"):
                        os.write(terminal, b"\r")
                        expect("要进入全局主题预览吗？")
                        expect("● 暂不进入")
                        assert snapshot() == before
                        os.write(terminal, key)
                        expect("样式预览")
                        expect("返回上级")
                        assert snapshot() == before
                    os.write(terminal, b"\x1b[B" * 4 + b"\r")
                elif scenario in ("check", "check-broken"):
                    expect("返回上级")
                    os.write(terminal, b"\x1b[B" * 3 + b"\r")
                    for key in (b"\x1b", b"\r"):
                        expect("starship doctor")
                        report = expect("完整结果与补充说明：slate doctor starship").replace(b"\r\n", b"")
                        expected = ("The selected file is absent" if english else "选中的配置文件不存在").encode()
                        assert expected in report, report
                        if scenario == "check-broken":
                            expected = ("Invalid TOML; file contents" if english else "TOML 语法错误").encode()
                            assert expected in report, report
                        expect("返回样式预览")
                        assert snapshot() == before
                        fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 10, columns, 0, 0))
                        os.kill(pid, signal.SIGWINCH)
                        expect("返回样式预览")
                        os.write(terminal, b"\x1b[F")
                        expect("完整结果与补充说明：slate doctor starship")
                        os.write(terminal, b"\x1b[H")
                        expect("starship doctor")
                        fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 40, columns, 0, 0))
                        os.kill(pid, signal.SIGWINCH)
                        expect("完整结果与补充说明：slate doctor starship")
                        expect("返回样式预览")
                        os.write(terminal, key)
                        expect("\x1b[?1049l")
                        expect("样式预览")
                        expect("● 检查提示符配置")
                        expect("返回上级")
                        if key == b"\x1b":
                            os.write(terminal, b"\r")
                    os.write(terminal, b"\x1b[B\r")
                elif scenario in ("decline", "stale", "apply", "escape", "interrupt", "noop"):
                    expect("返回上级")
                    os.write(terminal, b"\r")
                    review = expect("保存这个提示符样式？").decode()
                    if scenario == "noop":
                        assert ('Style Already Matches' if english else '样式已一致') in review
                        assert ('creates no recovery point' if english else '不新建恢复点') in review
                        assert ('A recovery point precedes writes' if english else '改写前创建恢复点') not in review
                    expect("● 暂不保存")
                    expect("○ 确认保存")
                    assert snapshot() == before
                    if scenario == "apply":
                        os.write(terminal, b"\x1b[D\r")
                        expect("查看恢复方案：slate restore")
                        expect("--dry-run")
                        expect("已保存：专注单行")
                        expect("实际效果未检查")
                    elif scenario == "noop":
                        os.write(terminal, b"\x1b[D\r")
                        expect("无需修改：专注单行")
                        expect("未改写文件或新建恢复点")
                    elif scenario == "stale":
                        (config / "config.toml").write_text("# changed elsewhere\n[prompt]\nstyle = 'classic'\n")
                        before = snapshot()
                        os.write(terminal, b"\x1b[D\r")
                        expect("configuration or destination changed after review")
                        expect("不自动重试")
                        expect("已保存：经典双行")
                    else:
                        os.write(terminal, {"escape": b"\x1b", "interrupt": b"\x03"}.get(scenario, b"\r"))
                    if scenario not in ("apply", "interrupt", "noop"):
                        expect("样式预览")
                        expect("返回上级")
                        os.write(terminal, b"\x1b[B" * 4 + b"\r")
                elif scenario == "broken":
                    os.write(terminal, b"\r")
                    expect("不自动重试")
                    expect("返回上级")
                    os.write(terminal, b"\x1b[B" * 4 + b"\r")
                else:
                    os.write(terminal, b"\x1b[B\r")
                    expect("选择提示符样式")
                    expect("● 专注单行")
                    expect("返回上级")
                    os.write(terminal, b"\x1b[B\x1b[B\r")
            if not cli:
                expect("test result: ok")
            deadline = time.monotonic() + 8
            while True:
                finished, status = os.waitpid(pid, os.WNOHANG)
                if finished:
                    reaped = True
                    break
                if time.monotonic() > deadline:
                    raise AssertionError("menu did not exit after confirmed action")
                # Drain pending output so a full PTY cannot prevent process exit.
                if select.select([terminal], [], [], 0.05)[0]:
                    try:
                        transcript.extend(os.read(terminal, 65536))
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
            assert os.waitstatus_to_exitcode(status) == (130 if scenario == "interrupt" else 0)
            # Capture the final screen restoration even if the child exited
            # before the normal wait loop consumed the remaining PTY output.
            while select.select([terminal], [], [], 0.05)[0]:
                try:
                    chunk = os.read(terminal, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    break
                if not chunk:
                    break
                transcript.extend(chunk)
            if cli:
                depth = 0
                for event in re.findall(rb'\x1b\[\?1049([hl])', transcript):
                    depth += 1 if event == b'h' else -1
                    assert depth in (0, 1), 'nested or unmatched scratch screen'
                assert depth == 0, 'scratch screen leaked after exit'
                if not scenario.startswith('direct-'):
                    assert b'\x1b[?1049h' in transcript, 'browser did not use scratch screen'
                if scenario in ('broken', 'stale'):
                    title = 'Action stopped: ' if english else '操作已停止：'
                    offset = transcript.index(title.encode())
                    preceding = transcript[:offset]
                    assert preceding.rfind(b'\x1b[?1049l') > preceding.rfind(b'\x1b[?1049h'), 'error hidden in scratch screen'
                    assert b'slate restore --list' in transcript
                    assert b'slate recover --dry-run' in transcript
            flags = termios.tcgetattr(terminal)[3]
            assert flags & termios.ECHO and flags & termios.ICANON, "terminal mode leaked"
            if scenario == "apply":
                assert (config / "current").read_bytes() == b"nord\n"
                assert '"focus"' in (config / "config.toml").read_text()
                for path in [root / ".config/starship.toml", config / "managed/starship/plain.toml"]:
                    assert '$directory$character' in path.read_text()
                    assert path.stat().st_mode & 0o777 == 0o600
                assert not (root / ".zshrc").exists()
                assert not (root / ".bashrc").exists()
            else:
                assert snapshot() == before, "browsing changed private configuration"
            if scenario == "noop":
                assert {path: path.stat().st_mtime_ns for path in file_times} == file_times, "no-op rewrote files"
            print(f"PASS: {scenario}; {columns} columns; {'English' if english else 'Chinese'}")
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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", action="store_true", help="exercise the actual Slate prompt command")
    parser.add_argument("--english", action="store_true", help="English CLI browsing and recoverable-error checks")
    parser.add_argument("--columns", type=int, choices=(20, 40, 80, 140), default=140)
    parser.add_argument("executable")
    parser.add_argument("scenarios", nargs="*")
    args = parser.parse_args()
    executable = str(Path(args.executable).resolve(strict=True))
    scenarios = args.scenarios or (["browse", "decline", "escape"] if args.english else ["browse", "broken", "decline", "stale", "apply", "check", "check-broken", "escape"])
    if args.english and (not args.cli or any(s not in ("browse", "decline", "escape", "broken", "stale", "noop", "check", "check-broken") for s in scenarios)):
        parser.error("--english requires --cli and browse/decline/escape/broken/stale/noop scenarios")
    if args.cli and not args.scenarios and not args.english:
        scenarios.extend(["interrupt", "noop", "direct-decline", "direct-escape"])
    for scenario in scenarios:
        if scenario not in ("browse", "broken", "decline", "stale", "apply", "check", "check-broken", "escape", "interrupt", "noop", "theme-back", "theme-unknown", "theme-unreadable", "direct-decline", "direct-escape"):
            raise SystemExit(f"unknown scenario: {scenario}")
        if scenario in ("interrupt", "noop", "direct-decline", "direct-escape") and not args.cli:
            raise SystemExit(f"{scenario} requires --cli")
        check(executable, scenario, args.cli, args.columns, args.english)
