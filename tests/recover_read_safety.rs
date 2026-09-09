//! Bounded CLI children inspect only private profiles; never read fixture FIFOs.
use slate_cli::env::SlateEnv;
use std::fs;
use std::os::unix::{
    ffi::OsStrExt,
    fs::{MetadataExt, PermissionsExt},
};
use std::path::Path;
use std::time::Duration;

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .write_stdin("")
        .timeout(Duration::from_secs(5))
        .args(["--quiet", "recover"]);
    command
}

fn identity(path: &Path) -> (u64, u64, u64, u32) {
    let metadata = fs::symlink_metadata(path).unwrap();
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mode(),
    )
}

fn private_output(output: &std::process::Output) {
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        assert!(
            !text.contains("PRIVATE_") && !text.contains("panicked"),
            "{text}"
        );
    }
}

#[test]
fn recover_read_safety_blocks_large_and_special_targets_but_keeps_export_and_discard_available() {
    for kind in ["oversized", "fifo"] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let target = env.managed_file("managed/ghostty/theme.conf");
        let record = env.slate_cache_dir().join("preview-session.json");
        write(&target, b"PRIVATE_PREVIEW");
        let mode = fs::metadata(&target).unwrap().permissions().mode();
        let value = serde_json::json!({
            "version": 1, "session_id": "bounded-read-fixture", "pid": std::process::id(),
            "home": env.home(), "config_dir": env.config_dir(), "cache_dir": env.slate_cache_dir(),
            "writing": false, "missing_dirs": [],
            "files": [{"path": target, "destination": fs::canonicalize(&target).unwrap(),
                "original": {"Present": {"bytes": b"PRIVATE_ORIGINAL".to_vec(), "mode": mode}}}],
            "expected": [{"Present": {"bytes": b"PRIVATE_PREVIEW".to_vec(), "mode": mode}}],
        });
        write(&env.slate_cache_dir().join("preview-session.lock"), b"");
        write(&record, serde_json::to_vec(&value).unwrap());
        if kind == "oversized" {
            fs::OpenOptions::new()
                .write(true)
                .open(&target)
                .unwrap()
                .set_len(8 * 1024 * 1024 + 1)
                .unwrap();
        } else {
            fs::remove_file(&target).unwrap();
            let path = std::ffi::CString::new(target.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        }
        let before = identity(&target);
        let saved = fs::read(&record).unwrap();
        let output = command(home.path())
            .args(["--dry-run", "--json"])
            .assert()
            .code(1)
            .get_output()
            .clone();
        private_output(&output);
        let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(plan["changes"][0]["action"], "blocked");
        let reason = plan["changes"][0]["reason"].as_str().unwrap();
        assert!(
            reason.contains(if kind == "oversized" {
                "8 MiB per file"
            } else {
                "regular file"
            }),
            "{reason}"
        );
        let output = command(home.path())
            .arg("--yes")
            .assert()
            .code(1)
            .get_output()
            .clone();
        private_output(&output);
        assert_eq!(identity(&target), before);
        assert_eq!(fs::read(&record).unwrap(), saved);
        let export = home.path().join("export");
        let output = command(home.path())
            .arg("--export")
            .arg(&export)
            .assert()
            .success()
            .get_output()
            .clone();
        private_output(&output);
        assert_eq!(
            fs::read(export.join("00.original")).unwrap(),
            b"PRIVATE_ORIGINAL"
        );
        assert_eq!(fs::read(&record).unwrap(), saved);
        assert_eq!(identity(&target), before);
        let output = command(home.path())
            .args(["--discard", "--yes"])
            .assert()
            .success()
            .get_output()
            .clone();
        private_output(&output);
        assert!(!record.exists());
        assert_eq!(identity(&target), before);
    }
}
