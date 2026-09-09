"""Check an explicit Slate binary with a private non-Unicode HOME; no writes."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = str(Path(sys.argv[1]).resolve(strict=True))
with tempfile.TemporaryDirectory(prefix="slate-path-cli-") as directory:
    home = os.fsencode(directory) + b"/profile-\xff"
    env = {b"HOME": home, b"SLATE_HOME": home, b"PATH": b"", b"NO_COLOR": b"1"}
    for json_mode in (True, False):
        args = [binary, "doctor", "ghostty"] + (["--json"] if json_mode else [])
        result = subprocess.run(args, env=env, cwd=directory, capture_output=True, timeout=8, check=True)
        assert not result.stderr, result.stderr
        if json_mode:
            report = json.loads(result.stdout)
            assert report["paths_are_lossy"] is True
            assert report["validation"]["status"] == "skipped"
            assert report["entries"]
            assert all(entry["path_is_lossy"] and not entry["exists"] for entry in report["entries"])
        else:
            assert b"lossy display; not an exact path" in result.stdout
        assert not list(Path(directory).iterdir()), "read-only inspection created files"
print("PASS: installed Ghostty diagnosis accepts byte paths, labels lossy display, writes nothing")
