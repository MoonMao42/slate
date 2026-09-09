//! Recovery records are input, not authority. Use only disposable profiles.
use slate_cli::config::{
    begin_restore_point_baseline_with_env, execute_restore_with_env, get_restore_point_with_env,
    list_restore_points_with_env, ConfigWriteGuard,
};
use slate_cli::env::SlateEnv;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::time::Duration;

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
fn fifo_manifest_cannot_hang_listing_or_preview() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    let path = env
        .slate_cache_dir()
        .join("backups")
        .join(&point.id)
        .join("manifest.toml");
    fs::remove_file(&path).unwrap();
    let fifo = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let before = tree_snapshot::tree(home.path());
    command(home.path())
        .args(["restore", "--list"])
        .assert()
        .success();
    let output = command(home.path())
        .args(["restore", &point.id, "--dry-run", "--json"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&output.stderr).contains("regular file"));
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

#[test]
fn malformed_timestamp_cannot_crash_restore_listing() {
    let home = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(home.path().to_owned());
    let point = begin_restore_point_baseline_with_env(&env).unwrap();
    let path = env
        .slate_cache_dir()
        .join("backups")
        .join(&point.id)
        .join("manifest.toml");
    let mut doc: toml::Value = fs::read_to_string(&path).unwrap().parse().unwrap();
    doc["metadata"]["created_at"] = "202é-09-05T00-00-0Z".into();
    fs::write(&path, toml::to_string(&doc).unwrap()).unwrap();
    let before = tree_snapshot::tree(home.path());
    let output = command(home.path())
        .args(["restore", "--list", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["points"], serde_json::json!([]));
    assert_eq!(report["issues"][0]["id"], point.id);
    command(home.path())
        .args(["restore", &point.id, "--dry-run"])
        .assert()
        .code(1);
    assert_eq!(tree_snapshot::tree(home.path()), before);
}

fn manifest_path(env: &SlateEnv, id: &str) -> std::path::PathBuf {
    env.slate_cache_dir()
        .join("backups")
        .join(id)
        .join("manifest.toml")
}

#[test]
fn damaged_records_are_skipped_without_hiding_healthy_history_or_mutating_files() {
    for kind in [
        "syntax",
        "utf8",
        "manifest-limit",
        "id",
        "theme-control",
        "tool-control",
        "key-control",
        "relative-target",
        "parent-target",
        "duplicate-target",
        "alias-target",
        "recovery-target",
        "entry-limit",
        "absent-with-backup",
        "source-outside",
        "source-link",
        "source-fifo",
        "source-limit",
        "total-source-limit",
        "manifest-link",
        "directory-link",
    ] {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        drop(ConfigWriteGuard::acquire(&env).unwrap());
        fs::write(env.zshrc_path(), "before\n").unwrap();
        let point = begin_restore_point_baseline_with_env(&env).unwrap();
        let healthy = begin_restore_point_baseline_with_env(&env).unwrap();
        let path = manifest_path(&env, &point.id);
        let mut doc: toml::Value = fs::read_to_string(&path).unwrap().parse().unwrap();
        // Keep a simple present entry; the healthy point retains the full schema.
        let entry = doc["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["tool_key"].as_str() == Some("zshrc"))
            .unwrap()
            .clone();
        doc["entries"] = vec![entry.clone()].into();
        let sentinel = outside.path().join("sentinel");
        fs::write(&sentinel, "PRIVATE_SAVED_CONTENT").unwrap();
        match kind {
            "id" => doc["metadata"]["id"] = "different-id".into(),
            "theme-control" => {
                doc["metadata"]["theme_name"] = "PRIVATE_SAVED_CONTENT\u{1b}[2J".into()
            }
            "tool-control" => doc["entries"][0]["display_tool"] = "PRIVATE_SAVED_CONTENT\n".into(),
            "key-control" => doc["entries"][0]["tool_key"] = "PRIVATE_SAVED_CONTENT\u{1b}".into(),
            "relative-target" => doc["entries"][0]["original_path"] = "relative-file".into(),
            "parent-target" => {
                doc["entries"][0]["original_path"] = home
                    .path()
                    .join("folder/../.zshrc")
                    .display()
                    .to_string()
                    .into()
            }
            "recovery-target" => {
                doc["entries"][0]["original_path"] = path.display().to_string().into()
            }
            "duplicate-target" | "alias-target" => {
                let mut second = entry.clone();
                second["tool_key"] = "second".into();
                if kind == "alias-target" {
                    symlink(home.path(), home.path().join("alias")).unwrap();
                    second["original_path"] = home
                        .path()
                        .join("alias/.zshrc")
                        .display()
                        .to_string()
                        .into();
                }
                doc["entries"].as_array_mut().unwrap().push(second);
            }
            "entry-limit" => doc["entries"] = vec![entry; 513].into(),
            "absent-with-backup" => doc["entries"][0]["original_state"] = "absent".into(),
            "source-outside" => {
                doc["entries"][0]["backup_path"] = sentinel.display().to_string().into()
            }
            "source-link" => {
                let source = Path::new(doc["entries"][0]["backup_path"].as_str().unwrap());
                fs::remove_file(source).unwrap();
                symlink(&sentinel, source).unwrap();
            }
            "source-fifo" => {
                let source = Path::new(doc["entries"][0]["backup_path"].as_str().unwrap());
                fs::remove_file(source).unwrap();
                let name = std::ffi::CString::new(source.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "source-limit" => {
                fs::OpenOptions::new()
                    .write(true)
                    .open(doc["entries"][0]["backup_path"].as_str().unwrap())
                    .unwrap()
                    .set_len(8 * 1024 * 1024 + 1)
                    .unwrap();
            }
            "total-source-limit" => {
                let mut entries = Vec::new();
                for index in 0..9 {
                    let mut next = entry.clone();
                    let source = path.parent().unwrap().join(format!("part{index}.backup"));
                    fs::File::create(&source)
                        .unwrap()
                        .set_len(8 * 1024 * 1024)
                        .unwrap();
                    next["tool_key"] = format!("part{index}").into();
                    next["original_path"] = home
                        .path()
                        .join(format!("target{index}"))
                        .display()
                        .to_string()
                        .into();
                    next["backup_path"] = source.display().to_string().into();
                    entries.push(next);
                }
                doc["entries"] = entries.into();
            }
            _ => {}
        }
        fs::write(&path, toml::to_string(&doc).unwrap()).unwrap();
        match kind {
            "syntax" => fs::write(&path, "PRIVATE_SAVED_CONTENT = ???\n").unwrap(),
            "utf8" => fs::write(&path, [0xff, 0xfe, 0]).unwrap(),
            "manifest-limit" => fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(1024 * 1024 + 1)
                .unwrap(),
            "manifest-link" => {
                fs::copy(&path, &sentinel).unwrap();
                fs::remove_file(&path).unwrap();
                symlink(&sentinel, &path).unwrap();
            }
            "directory-link" => {
                let directory = path.parent().unwrap();
                let moved = outside.path().join(&point.id);
                fs::rename(directory, &moved).unwrap();
                symlink(moved, directory).unwrap();
            }
            _ => {}
        }
        // Sparse size fixtures are not read into the comparison tree: verify
        // their sizes below, while all other bytes/links/modes remain compared.
        let snapshot = |root: &Path| {
            fn visit(path: &Path, output: &mut Vec<(std::path::PathBuf, u64, u32, Vec<u8>)>) {
                let meta = fs::symlink_metadata(path).unwrap();
                if meta.is_dir() {
                    for entry in fs::read_dir(path).unwrap() {
                        visit(&entry.unwrap().path(), output);
                    }
                } else {
                    let bytes = if meta.is_symlink() {
                        fs::read_link(path)
                            .unwrap()
                            .as_os_str()
                            .as_encoded_bytes()
                            .to_vec()
                    } else if meta.is_file() && meta.len() <= 64 * 1024 {
                        fs::read(path).unwrap()
                    } else {
                        Vec::new()
                    };
                    output.push((
                        path.to_owned(),
                        meta.len(),
                        meta.permissions().mode(),
                        bytes,
                    ));
                }
            }
            let mut entries = Vec::new();
            visit(root, &mut entries);
            entries.sort();
            entries
        };
        let before = snapshot(home.path());
        let external_before = snapshot(outside.path());
        let listed = list_restore_points_with_env(&env).unwrap();
        assert_eq!(
            listed.iter().map(|point| &point.id).collect::<Vec<_>>(),
            vec![&healthy.id],
            "{kind}"
        );
        assert!(
            get_restore_point_with_env(&env, &point.id).is_err(),
            "{kind}"
        );
        assert!(execute_restore_with_env(&env, &point.id).is_err(), "{kind}");
        let result = command(home.path())
            .args(["restore", &point.id, "--dry-run", "--json"])
            .assert()
            .code(1)
            .get_output()
            .clone();
        let message = String::from_utf8_lossy(&result.stderr);
        assert!(
            !message.contains("PRIVATE_SAVED_CONTENT"),
            "{kind}: {message}"
        );
        assert!(result.stdout.is_empty(), "{kind}");
        assert_eq!(snapshot(home.path()), before, "{kind}");
        assert_eq!(snapshot(outside.path()), external_before, "{kind}");
    }
}

#[test]
fn oversized_special_and_broken_parent_targets_block_before_any_restore() {
    for kind in ["oversized", "fifo", "broken-parent", "final-link"] {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        drop(ConfigWriteGuard::acquire(&env).unwrap());
        fs::write(env.zshrc_path(), "old zsh\n").unwrap();
        fs::write(env.bashrc_path(), "old bash\n").unwrap();
        let point = begin_restore_point_baseline_with_env(&env).unwrap();
        fs::write(env.zshrc_path(), "new zsh\n").unwrap();
        fs::remove_file(env.bashrc_path()).unwrap();
        match kind {
            "oversized" => fs::File::create(env.bashrc_path())
                .unwrap()
                .set_len(8 * 1024 * 1024 + 1)
                .unwrap(),
            "fifo" => {
                let name = std::ffi::CString::new(env.bashrc_path().as_os_str().as_encoded_bytes())
                    .unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "final-link" => symlink(home.path().join("absent"), env.bashrc_path()).unwrap(),
            "broken-parent" => {
                symlink(home.path().join("absent"), home.path().join("broken")).unwrap();
                let path = manifest_path(&env, &point.id);
                let mut doc: toml::Value = fs::read_to_string(&path).unwrap().parse().unwrap();
                for entry in doc["entries"].as_array_mut().unwrap() {
                    if entry["tool_key"].as_str() == Some("bashrc") {
                        entry["original_path"] = home
                            .path()
                            .join("broken/child")
                            .display()
                            .to_string()
                            .into();
                    }
                }
                fs::write(path, toml::to_string(&doc).unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        let before = tree_snapshot::tree(home.path());
        let output = command(home.path())
            .args(["restore", &point.id, "--dry-run", "--json"])
            .assert()
            .code(1)
            .get_output()
            .clone();
        let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            plan["changes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|change| change["tool_key"] == "bashrc" && change["action"] == "blocked"),
            "{kind}"
        );
        assert!(execute_restore_with_env(&env, &point.id).is_err(), "{kind}");
        assert_eq!(tree_snapshot::tree(home.path()), before, "{kind}");
    }
}
