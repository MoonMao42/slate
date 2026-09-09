use slate_cli::env::SlateEnv;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;
use tempfile::TempDir;

fn run(home: &Path, args: &[&str]) -> Output {
    let binary = std::env::var_os("SLATE_STATUS_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| assert_cmd::cargo::cargo_bin!("slate").to_owned());
    assert_cmd::Command::new(binary)
        .env("SLATE_HOME", home)
        .env("NO_COLOR", "1")
        .args(args)
        .timeout(Duration::from_secs(10))
        .assert()
        .success()
        .get_output()
        .clone()
}

#[test]
fn status_json_handles_non_utf8_paths_without_writes() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let td = TempDir::new().unwrap();
    // APFS rejects creating this name; diagnostics must also handle nonexistent
    // or inaccessible paths without first creating them.
    for name in [b"home-\xff".to_vec(), "home-中文".as_bytes().to_vec()] {
        let home = td.path().join(OsString::from_vec(name));
        let env = SlateEnv::with_home(home.clone());
        let before = tree(td.path());
        let output = run(&home, &["status", "--json"]);
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        for (key, path) in [
            ("config_dir", env.config_dir().to_owned()),
            ("cache_dir", env.slate_cache_dir().to_owned()),
        ] {
            assert_eq!(report[key], path.to_string_lossy().as_ref());
            assert_eq!(report[format!("{key}_is_lossy")], path.to_str().is_none());
        }
        let record = env.slate_cache_dir().join("preview-session.json");
        assert_eq!(
            report["recovery"]["record_path"],
            record.to_string_lossy().as_ref()
        );
        assert_eq!(
            report["recovery"]["record_path_is_lossy"],
            record.to_str().is_none()
        );
        assert_eq!(tree(td.path()), before);
    }
}

fn seed(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn tree(root: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    fn visit(path: &Path, entries: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let bytes = if metadata.is_file() {
            fs::read(path).unwrap()
        } else {
            Vec::new()
        };
        entries.insert(path.to_owned(), (metadata.permissions().mode(), bytes));
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(&entry.unwrap().path(), entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, &mut entries);
    entries
}

#[test]
fn hub_entry_noninteractive_is_read_only_for_empty_known_and_invalid_settings() {
    for theme in [None, Some("nord"), Some("retired-theme")] {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        if let Some(theme) = theme {
            seed(&env.managed_file("current"), theme.as_bytes());
            seed(&env.managed_file("config.toml"), b"[PRIVATE_BROKEN\n");
        }
        let before = tree(td.path());
        let output = run(td.path(), &[]);
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("slate theme") && text.contains("slate setup"));
        assert!(!text.contains("What would you like to do?"));
        assert!(!text.contains("Catppuccin Mocha"));
        assert!(!text.contains("PRIVATE_BROKEN"));
        assert_eq!(tree(td.path()), before);
    }
}

#[test]
fn status_is_read_only_and_does_not_invent_a_configured_theme() {
    let td = TempDir::new().unwrap();
    let before = tree(td.path());
    let output = run(td.path(), &["status", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert!(report["theme"].is_null() && report["font"].is_null() && report["opacity"].is_null());
    assert_eq!(report["auto_theme_enabled"], false);
    assert_eq!(report["recovery"]["status"], "clear");
    assert_eq!(report["warnings"], serde_json::json!([]));
    assert_eq!(tree(td.path()), before);
    let output = run(td.path(), &["status"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("No recognized saved theme"));
    assert!(text.contains("availability only") && text.contains("nvim"));
    assert!(!text.contains("Catppuccin Mocha"));
    assert_eq!(tree(td.path()), before, "status created config/cache files");
}

#[test]
fn status_reports_invalid_language_without_overwriting_or_disclosing_it() {
    for (value, invalid) in [
        ("\"en\"", false),
        ("\"zh-CN\"", false),
        ("\"PRIVATE_UNKNOWN_LANGUAGE\"", true),
        ("42", true),
        ("[]", true),
    ] {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        seed(
            &env.managed_file("config.toml"),
            format!("[preferences]\nlanguage = {value}\n").as_bytes(),
        );
        let before = tree(td.path());
        let output = run(td.path(), &["status", "--json"]);
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let warnings = report["warnings"].as_array().unwrap();
        assert_eq!(
            warnings
                .iter()
                .any(|warning| warning["field"] == "language"),
            invalid,
            "{report}"
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_UNKNOWN_LANGUAGE"));
        assert_eq!(tree(td.path()), before);
    }
}

#[test]
fn status_reports_invalid_settings_without_changing_or_disclosing_them() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    seed(&env.managed_file("current"), b"retired-theme\n");
    seed(&env.managed_file("current-font"), b"Fixture Mono\n");
    seed(&env.managed_file("current-opacity"), b"invalid-opacity\n");
    seed(
        &env.managed_file("config.toml"),
        b"[PRIVATE_CONFIG_CONTENT\n",
    );
    let before = tree(td.path());
    let output = run(td.path(), &["status", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["theme"]["id"], "retired-theme");
    assert!(report["theme"]["name"].is_null());
    assert_eq!(report["font"], "Fixture Mono");
    assert!(report["opacity"].is_null() && report["auto_theme_enabled"].is_null());
    assert!(report["prompt_style"].is_null());
    assert_eq!(report["warnings"].as_array().unwrap().len(), 5);
    assert!(report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["field"] == "prompt_style"));
    for bytes in [&output.stdout, &output.stderr] {
        assert!(!String::from_utf8_lossy(bytes).contains("PRIVATE_CONFIG_CONTENT"));
    }
    let output = run(td.path(), &["status"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("retired-theme") && text.contains("Warning [opacity]"));
    assert!(!text.contains("Catppuccin Mocha") && !text.contains("PRIVATE_CONFIG_CONTENT"));
    assert_eq!(tree(td.path()), before);
}

#[test]
fn status_and_hub_surface_pending_active_conflicting_and_corrupt_recovery() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let target = env.managed_file("managed/ghostty/theme.conf");
    let record_path = env.slate_cache_dir().join("preview-session.json");
    let lock_path = env.slate_cache_dir().join("preview-session.lock");
    let preview = b"preview colors\n";
    seed(&target, preview);
    seed(&lock_path, b"");
    let mode = fs::metadata(&target).unwrap().permissions().mode();
    let record = serde_json::json!({
        "version": 1, "session_id": "status-fixture", "pid": std::process::id(),
        "home": env.home(), "config_dir": env.config_dir(), "cache_dir": env.slate_cache_dir(),
        "writing": false, "missing_dirs": [],
        "files": [{"path": target, "destination": fs::canonicalize(&target).unwrap(),
                   "original": {"Present": {"bytes": b"PRIVATE_BACKUP_CONTENT".to_vec(), "mode": mode}}}],
        "expected": [{"Present": {"bytes": preview.to_vec(), "mode": mode}}],
    });
    seed(&record_path, &serde_json::to_vec(&record).unwrap());
    // The recovery-first hub must not need to parse interrupted preferences.
    seed(
        &env.managed_file("config.toml"),
        b"[PRIVATE_CONFIG_CONTENT\n",
    );
    for state in ["pending", "active", "conflicted", "unreadable"] {
        let lock = if state == "active" {
            let lock = File::open(&lock_path).unwrap();
            assert_eq!(
                unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                0
            );
            Some(lock)
        } else {
            None
        };
        if state == "conflicted" {
            seed(&target, b"user edit after interrupted preview\n");
        }
        if state == "unreadable" {
            let mut invalid = record.clone();
            invalid["files"][0]["original"]["Present"]["bytes"] =
                serde_json::json!("PRIVATE_BACKUP_CONTENT");
            seed(&record_path, &serde_json::to_vec(&invalid).unwrap());
        }
        let before = tree(td.path());
        let output = run(td.path(), &["status", "--json"]);
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["recovery"]["status"], state);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_BACKUP_CONTENT"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_BACKUP_CONTENT"));
        match state {
            "pending" => assert_eq!(report["recovery"]["files_to_restore"], 1),
            "conflicted" => assert_eq!(report["recovery"]["conflicts"], 1),
            _ => assert!(report["recovery"]["conflicts"].is_null()),
        }
        for args in [&["status"][..], &[][..]] {
            let output = run(td.path(), args);
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(combined.to_ascii_lowercase().contains("preview recovery"));
            assert!(!combined.contains("PRIVATE_BACKUP_CONTENT"));
            assert!(!combined.contains("PRIVATE_CONFIG_CONTENT"));
            assert!(!combined.contains("What would you like to do?"));
        }
        assert_eq!(
            tree(td.path()),
            before,
            "inspection changed files for {state}"
        );
        if state == "unreadable" {
            let output = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
                .env("SLATE_HOME", td.path())
                .args(["recover", "--dry-run", "--json"])
                .timeout(Duration::from_secs(10))
                .assert()
                .failure()
                .get_output()
                .clone();
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("line") && stderr.contains("column"));
            assert!(!stderr.contains("PRIVATE_BACKUP_CONTENT"));
            assert_eq!(tree(td.path()), before);
        }
        drop(lock);
    }
}
