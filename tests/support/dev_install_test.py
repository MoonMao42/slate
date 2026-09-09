"""Exercise the developer installer with disposable executable fixtures only."""
from pathlib import Path
import subprocess
import tempfile

installer = Path(__file__).resolve().parents[2] / 'scripts/install-dev.sh'
with tempfile.TemporaryDirectory(prefix='slate dev install ') as directory:
    root = Path(directory)
    binary = root / 'candidate'
    destination = root / 'bin'
    backups = root / 'backups'
    destination.mkdir()
    target = destination / 'slate'

    def candidate(text, status=0):
        binary.write_text(f'#!/bin/sh\nprintf "%s\\n" "{text}"\nexit {status}\n')
        binary.chmod(0o755)

    def run(success=True):
        result = subprocess.run(['bash', str(installer), str(binary), str(destination),
                                 str(backups)], capture_output=True, text=True, timeout=10)
        assert (result.returncode == 0) == success, result
        assert not list(destination.glob('.slate-update-*'))
        return result

    candidate('new build')
    old = b'#!/bin/sh\necho old\n'
    target.write_bytes(old)
    target.chmod(0o751)
    run()
    assert target.read_bytes() == binary.read_bytes()
    saved = list(backups.glob('update-*/slate'))
    assert len(saved) == 1 and saved[0].read_bytes() == old
    assert saved[0].stat().st_mode & 0o777 == 0o751
    assert 'Already installed' in run().stdout
    assert list(backups.glob('update-*/slate')) == saved
    installed = target.read_bytes()
    candidate('broken build', 1)
    run(False)
    assert target.read_bytes() == installed
    candidate('newer build')
    target.unlink()
    target.symlink_to(binary)
    run(False)
    assert target.is_symlink()
    target.unlink()
    lock = destination / '.slate-dev-install.lock'
    lock.mkdir()
    run(False)
    assert lock.is_dir() and not target.exists()
    lock.rmdir()
    run()
    assert target.read_bytes() == binary.read_bytes()
    assert not lock.exists()
print('PASS: replacement, backup mode/bytes, no-op, failed candidate, symlink, lock and fresh install')
