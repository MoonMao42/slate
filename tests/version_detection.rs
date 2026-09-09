//! Public version-probe deadline checks with disposable executable scripts only.
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

#[test]
#[ignore = "invoked by the parent in a deadline-guarded private subprocess"]
fn version_probe_deadline_child() {
    let path = std::env::var("SLATE_VERSION_FIXTURE").unwrap();
    let start = Instant::now();
    let error = slate_cli::platform::version_check::detect_version(&path)
        .unwrap_err()
        .to_string();
    assert!(error.contains("timed out after 2000 ms"), "{error}");
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn version_probe_public_entry_bounds_hangs_and_inherited_pipes() {
    for inherited in [false, true] {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("probe");
        let body = if inherited {
            "/bin/sleep 20 & exit 0"
        } else {
            "exec /bin/sleep 20"
        };
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\n[ \"$1\" = --version ] || exit 91\nprintf 'NVIM v0.12.0\\n'\n{body}\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", td.path())
            .env("SLATE_HOME", td.path())
            .env("PATH", td.path().join("empty-bin"))
            .env("SLATE_VERSION_FIXTURE", &path)
            .args([
                "--exact",
                "version_probe_deadline_child",
                "--ignored",
                "--nocapture",
            ])
            .timeout(Duration::from_secs(7))
            .assert()
            .success();
    }
}
