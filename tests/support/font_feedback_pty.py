"""Check font success feedback in private profiles; never install real fonts."""
import errno
import os
from pathlib import Path
import pty
import select
import subprocess
import sys
import tempfile
import time

binary = str(Path(sys.argv[1]).resolve(strict=True))
for terminal in [False, True]:
    with tempfile.TemporaryDirectory(prefix="slate-font-feedback-") as directory:
        root = Path(directory)
        config = root / ".config/slate"
        config.mkdir(parents=True)
        (config / "current").write_text("nord\n")
        (config / "config.toml").write_text("[sound]\nenabled = false\n")
        fonts = root / ("Library/Fonts" if sys.platform == "darwin" else ".local/share/fonts")
        fonts.mkdir(parents=True)
        fixture = fonts / "SlateFeedbackFixtureNerdFont-Regular.ttf"
        fixture.write_bytes(b"\x00\x01\x00\x00private-font-feedback-fixture")
        env = {"HOME": directory, "SLATE_HOME": directory, "PATH": "",
               "NO_COLOR": "1", "TERM": "xterm-256color"}
        command = [binary, "font", "SlateFeedbackFixture Nerd Font"]
        if not terminal:
            result = subprocess.run(command, env=env, cwd=directory,
                                    capture_output=True, timeout=10)
            output = result.stdout + result.stderr
            assert result.returncode == 0, output
            assert b"Terminal font refs:" in output, output
        else:
            master, slave = pty.openpty()
            process = subprocess.Popen(command, env=env, cwd=directory,
                                       stdin=subprocess.DEVNULL, stdout=slave, stderr=slave)
            os.close(slave)
            output = bytearray()
            try:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    if select.select([master], [], [], 0.1)[0]:
                        try:
                            chunk = os.read(master, 65536)
                        except OSError as error:
                            if error.errno != errno.EIO:
                                raise
                            break
                        if not chunk:
                            break
                        output.extend(chunk)
                assert process.wait(timeout=1) == 0, output
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
                os.close(master)
            assert "尚未发现终端引用此字体配置".encode() in output, output
            assert b"Terminal font refs:" not in output, output
            assert b"slate doctor font" in output, output
        assert "SlateFeedbackFixture Nerd Font" in (config / "current-font").read_text()
        assert (config / "current").read_text() == "nord\n"
        assert fixture.read_bytes() == b"\x00\x01\x00\x00private-font-feedback-fixture"
        print("PASS:", "compact terminal feedback" if terminal else "detailed redirected feedback")
