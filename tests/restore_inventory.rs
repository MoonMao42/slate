//! Read-only history listing, including unusable records and hidden undo points.
use slate_cli::config::{
    begin_restore_point_baseline_with_env, create_pre_restore_snapshot_with_env, ConfigWriteGuard,
};
use slate_cli::env::SlateEnv;
use std::fs;
use std::os::fd::OwnedFd;
use std::os::unix::{ffi::OsStringExt, fs::symlink, net::UnixStream};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

#[path = "support/redirected_output.rs"]
mod redirected_output;
#[path = "support/tree.rs"]
mod tree_snapshot;

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(3));
    command
}

#[test]
fn history_json_and_all_are_read_only_and_expose_hidden_undo_points() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let before = tree_snapshot::tree(home.path());
    let output = command(home.path())
        .args(["restore", "--list", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let empty: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(empty["schema_version"], 1);
    assert_eq!(empty["points"], serde_json::json!([]));
    assert_eq!(empty["issues"], serde_json::json!([]));
    assert!(output.stderr.is_empty());
    assert_eq!(tree_snapshot::tree(home.path()), before);

    fs::write(env.zshrc_path(), "PRIVATE_BACKUP_CONTENT\n").unwrap();
    let baseline = begin_restore_point_baseline_with_env(&env).unwrap();
    let undo = create_pre_restore_snapshot_with_env(&env, &baseline.id).unwrap();
    // Tie the timestamps to verify deterministic ID ordering, independent of
    // readdir order or the duration of the test run.
    for id in [&baseline.id, &undo.id] {
        let path = env
            .slate_cache_dir()
            .join("backups")
            .join(id)
            .join("manifest.toml");
        let mut doc: toml::Value = fs::read_to_string(&path).unwrap().parse().unwrap();
        doc["metadata"]["created_at"] = "2026-09-05T00-00-00Z".into();
        fs::write(path, toml::to_string(&doc).unwrap()).unwrap();
    }
    let before = tree_snapshot::tree(home.path());
    for all in [false, true] {
        let mut args = vec!["restore", "--list", "--json"];
        if all {
            args.push("--all");
        }
        let output = command(home.path())
            .args(args)
            .assert()
            .success()
            .get_output()
            .clone();
        let text = String::from_utf8(output.stdout).unwrap();
        let data: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(data["valid_count"], 2);
        assert_eq!(data["target_contents_checked"], false);
        assert_eq!(data["hidden_undo_count"], if all { 0 } else { 1 });
        assert_eq!(
            data["points"].as_array().unwrap().len(),
            if all { 2 } else { 1 }
        );
        assert!(data["points"]
            .as_array()
            .unwrap()
            .iter()
            .any(|point| point["id"] == baseline.id && point["is_baseline"] == true));
        assert!(data["points"]
            .as_array()
            .unwrap()
            .iter()
            .all(|point| point["may_regenerate_theme_files"] == false));
        if all {
            let ids: Vec<_> = data["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|point| point["id"].as_str().unwrap())
                .collect();
            let mut expected = vec![baseline.id.as_str(), undo.id.as_str()];
            expected.sort();
            assert_eq!(ids, expected);
            assert!(data["points"]
                .as_array()
                .unwrap()
                .iter()
                .any(|point| point["id"] == undo.id && point["is_undo"] == true));
        }
        assert!(!text.contains("PRIVATE_BACKUP_CONTENT"));
        assert!(output.stderr.is_empty());
    }
    command(home.path())
        .args(["restore", "--list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("slate restore --list --all"));
    command(home.path())
        .args(["restore", "--list", "--all"])
        .assert()
        .success()
        .stdout(predicates::str::contains(&undo.id));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    fs::rename(
        env.slate_cache_dir().join("backups").join(&baseline.id),
        home.path().join("baseline-aside"),
    )
    .unwrap();
    let before = tree_snapshot::tree(home.path());
    let output = command(home.path())
        .args(["restore", "--list"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("slate restore --list --all"));
    assert!(!text.contains("Run 'slate setup'"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    // With only undo checkpoints, the picker returns guidance without opening
    // an interactive selector. Seed its usual writer/config infrastructure.
    slate_cli::config::ConfigManager::with_env(&env).unwrap();
    drop(ConfigWriteGuard::acquire(&env).unwrap());
    let before = tree_snapshot::tree(home.path());
    let output = command(home.path())
        .args(["--quiet", "restore"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("slate restore --list --all"));
    assert!(!text.contains("Run 'slate setup'"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn inventory_reports_bad_records_and_incomplete_captures_without_reading_payloads() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let healthy = begin_restore_point_baseline_with_env(&env).unwrap();
    let bad = begin_restore_point_baseline_with_env(&env).unwrap();
    let incomplete = begin_restore_point_baseline_with_env(&env).unwrap();
    let special = begin_restore_point_baseline_with_env(&env).unwrap();
    let root = env.slate_cache_dir().join("backups");
    fs::write(
        root.join(&bad.id).join("manifest.toml"),
        "PRIVATE_MANIFEST_BODY = ???\n",
    )
    .unwrap();
    fs::remove_file(root.join(&incomplete.id).join("manifest.toml")).unwrap();
    let fifo_path = root.join(&special.id).join("manifest.toml");
    fs::remove_file(&fifo_path).unwrap();
    let fifo = std::ffi::CString::new(fifo_path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    fs::write(outside.path().join("manifest.toml"), "PRIVATE_OUTSIDE_BODY").unwrap();
    symlink(outside.path(), root.join("linked-record")).unwrap();
    // Old per-tool backups and arbitrary non-record files are not damaged snapshots.
    fs::create_dir(root.join("starship")).unwrap();
    fs::write(root.join("starship/old.bak"), "PRIVATE_LEGACY_BACKUP").unwrap();
    fs::write(root.join("loose.bak"), "PRIVATE_LOOSE_BACKUP").unwrap();
    // APFS rejects non-UTF-8 path components before Slate can see them. Linux
    // exercises the on-disk case; the inventory unit test covers formatting on both.
    let names = if cfg!(target_os = "linux") {
        vec![
            std::ffi::OsString::from("bad\u{1b}[31m"),
            std::ffi::OsString::from_vec(b"bad-\xff".to_vec()),
        ]
    } else {
        vec![std::ffi::OsString::from("bad\u{1b}[31m")]
    };
    let invalid_name_count = names.len();
    for name in names {
        let path = root.join(name);
        fs::create_dir(&path).unwrap();
        fs::write(path.join("manifest.toml"), "PRIVATE_INVALID_ID_BODY").unwrap();
    }
    let before = tree_snapshot::tree(home.path());
    let outside_before = tree_snapshot::tree(outside.path());
    let output = command(home.path())
        .args(["restore", "--list", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("PRIVATE_"));
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["valid_count"], 1);
    assert_eq!(report["points"][0]["id"], healthy.id);
    assert_eq!(report["ignored_entries"], 2);
    let issues = report["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 4 + invalid_name_count);
    for (id, kind) in [
        (&bad.id, "invalid_record"),
        (&special.id, "invalid_record"),
        (&incomplete.id, "missing_manifest"),
    ] {
        let issue = issues
            .iter()
            .find(|issue| issue["id"].as_str() == Some(id))
            .unwrap();
        assert_eq!(issue["kind"], kind);
        assert!(issue["next_step"].as_str().unwrap().contains("--dry-run"));
    }
    assert!(issues.iter().any(|issue| issue["kind"] == "linked_entry"));
    assert_eq!(
        issues
            .iter()
            .filter(|issue| issue["kind"] == "invalid_id" && issue["id"].is_null())
            .count(),
        invalid_name_count
    );
    assert_eq!(
        issues.iter().any(|issue| issue["path_is_lossy"] == true),
        cfg!(target_os = "linux")
    );
    let rendered = command(home.path())
        .args(["restore", "--list"])
        .assert()
        .success()
        .get_output()
        .clone();
    let rendered = String::from_utf8(rendered.stdout).unwrap();
    assert!(rendered.contains("not deleted"));
    assert!(rendered.contains(&bad.id) && rendered.contains(&healthy.id));
    assert!(!rendered.contains('\u{1b}') && !rendered.contains("PRIVATE_"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
    assert_eq!(tree_snapshot::tree(outside.path()), outside_before);
}

#[test]
fn listing_bypasses_preferences_active_writer_and_pending_preview_and_handles_closed_stdout() {
    use assert_cmd::assert::OutputAssertExt;
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    let _guard = ConfigWriteGuard::acquire(&env).unwrap();
    fs::create_dir_all(env.config_dir()).unwrap();
    let path = std::ffi::CString::new(
        env.managed_file("config.toml")
            .as_os_str()
            .as_encoded_bytes(),
    )
    .unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    fs::write(
        env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_PENDING_RECORD",
    )
    .unwrap();
    let before = tree_snapshot::tree(home.path());
    for args in [
        vec!["restore", "--list", "--all"],
        vec!["--quiet", "restore", "--list", "--json"],
    ] {
        let output = command(home.path())
            .args(args)
            .assert()
            .success()
            .get_output()
            .clone();
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(&point.id));
        assert!(!text.contains("PRIVATE_PENDING_RECORD"));
        assert!(output.stderr.is_empty());
    }
    let (consumer, producer) = UnixStream::pair().unwrap();
    drop(consumer);
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    child
        .env_clear()
        .env("HOME", home.path())
        .env("SLATE_HOME", home.path())
        .env("PATH", "")
        .args(["restore", "--list", "--json"])
        .stdout(Stdio::from(OwnedFd::from(producer)))
        .stderr(Stdio::piped());
    redirected_output::run(&mut child)
        .assert()
        .success()
        .stderr("");
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn listing_modes_reject_ambiguous_commands_and_fail_on_unreadable_storage() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        vec!["restore", "--all"],
        vec!["restore", "--all", "--json"],
        vec!["restore", "--json"],
        vec!["restore", "id", "--json"],
        vec!["restore", "id", "--all"],
        vec!["restore", "--list", "--dry-run"],
        vec!["restore", "--list", "--delete", "id"],
        vec!["restore", "--delete", "id", "--json"],
        vec!["restore", "--list", "id", "--json"],
        vec!["restore", "id", "--dry-run", "--all"],
    ] {
        command(home.path()).args(args).assert().code(2);
        assert_eq!(
            tree_snapshot::tree(home.path()).len(),
            1,
            "invalid input created profile files"
        );
    }
    let env = SlateEnv::with_home(home.path().to_owned());
    fs::create_dir_all(env.slate_cache_dir()).unwrap();
    let root = env.slate_cache_dir().join("backups");
    fs::write(&root, "PRIVATE_NOT_A_DIRECTORY").unwrap();
    for kind in ["file", "broken-link"] {
        if kind == "broken-link" {
            fs::remove_file(&root).unwrap();
            symlink(home.path().join("absent"), &root).unwrap();
        }
        let before = tree_snapshot::tree(home.path());
        let output = command(home.path())
            .args(["restore", "--list", "--json"])
            .assert()
            .code(1)
            .get_output()
            .clone();
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}
