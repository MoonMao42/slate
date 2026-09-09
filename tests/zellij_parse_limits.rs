//! Only disposable child processes see adversarial KDL. Never native Zellij.
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    process::Command,
    time::Duration,
};

#[path = "support/tree.rs"]
mod tree_snapshot;

fn slate(home: &std::path::Path) -> assert_cmd::Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .current_dir(home);
    // A regressed parser must not create a core dump, nor abort this test runner.
    unsafe {
        command.pre_exec(|| {
            let limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::setrlimit(libc::RLIMIT_CORE, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut command = assert_cmd::Command::from_std(command);
    command.timeout(Duration::from_secs(6));
    command
}

fn doctor(home: &std::path::Path) -> assert_cmd::Command {
    let mut command = slate(home);
    command.args(["doctor", "zellij", "--json"]);
    command
}

fn sync_fixture(home: &std::path::Path, document: &str) {
    for directory in ["bin", ".config/zellij", ".config/slate"] {
        fs::create_dir_all(home.join(directory)).unwrap();
    }
    let binary = home.join("bin/zellij");
    fs::write(
        &binary,
        "#!/bin/sh\nprintf unexpected > \"$HOME/UNEXPECTED_PROCESS\"\nexit 91\n",
    )
    .unwrap();
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(home.join(".config/slate/current"), "nord\n").unwrap();
    fs::write(home.join(".config/zellij/config.kdl"), document).unwrap();
}

#[test]
fn deeply_nested_zellij_configs_and_other_themes_report_errors_without_abort() {
    for target in ["config.kdl", "themes/personal.kdl"] {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join(".config/zellij").join(target);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let document = format!(
            "{}// PRIVATE_NESTED_CONTENT\n{}",
            "node {\n".repeat(4096),
            "}\n".repeat(4096)
        );
        fs::write(path, document).unwrap();
        let before = tree_snapshot::tree(home.path());
        let output = doctor(home.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert!(!String::from_utf8_lossy(&output).contains("PRIVATE_NESTED_CONTENT"));
        assert!(String::from_utf8_lossy(&output).contains("KDL complexity exceeds 128"));
        let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert!(report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["status"] == "error"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn excessive_kdl_stops_sync_and_clean_before_configuration_changes() {
    let home = tempfile::tempdir().unwrap();
    let document = format!("{}{}", "node {\n".repeat(4096), "}\n".repeat(4096));
    sync_fixture(home.path(), &document);
    let before = tree_snapshot::tree(home.path());
    for args in [
        vec!["tools", "sync", "zellij", "--dry-run"],
        vec!["tools", "sync", "zellij", "--yes"],
    ] {
        let output = slate(home.path())
            .args(args)
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        assert!(String::from_utf8_lossy(&output).contains("KDL complexity exceeds 128"));
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
    let output = slate(home.path())
        .args(["--quiet", "clean"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8_lossy(&output).contains("KDL complexity exceeds 128"));
    // Clean may allocate its writer lock/cache, but cannot create a restore point
    // or alter settings/assets after this target-preflight failure.
    let unchanged = |mut tree: std::collections::BTreeMap<_, _>| {
        tree.retain(|path: &std::path::PathBuf, _| !path.starts_with(home.path().join(".cache")));
        tree
    };
    assert_eq!(
        unchanged(tree_snapshot::tree(home.path())),
        unchanged(before)
    );
    assert!(!home.path().join(".cache/slate/backups").exists());
}

#[test]
fn maximum_budget_also_applies_from_parallel_adapter_workers() {
    let home = tempfile::tempdir().unwrap();
    let document = format!("{}{}", "node {\n".repeat(128), "}\n".repeat(128));
    sync_fixture(home.path(), &document);
    slate(home.path())
        .args(["tools", "sync", "zellij", "--yes"])
        .assert()
        .success();
    assert!(
        fs::read_to_string(home.path().join(".config/zellij/config.kdl"))
            .unwrap()
            .starts_with(&document)
    );
    assert!(home
        .path()
        .join(".config/zellij/themes/slate-sync.kdl")
        .is_file());
    assert!(!home.path().join("UNEXPECTED_PROCESS").exists());
}

#[test]
fn block_comments_are_handled_iteratively_and_slashdash_recursion_is_bounded() {
    for (document, rejected) in [
        (
            format!("/*{}*/\ntheme \"nord\"\n", " * / ".repeat(8192)),
            false,
        ),
        (
            format!(
                "{}PRIVATE_NESTED_CONTENT{}\ntheme \"nord\"\n",
                "/*".repeat(4096),
                "*/".repeat(4096)
            ),
            false,
        ),
        (
            format!("{}{}", "/- ".repeat(4096), "node\n".repeat(4097)),
            true,
        ),
        (
            format!("node {}{}\n", "/- ".repeat(4096), "1 ".repeat(4097)),
            true,
        ),
    ] {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join(".config/zellij/config.kdl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, document).unwrap();
        let before = tree_snapshot::tree(home.path());
        let output = doctor(home.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert!(!String::from_utf8_lossy(&output).contains("PRIVATE_NESTED_CONTENT"));
        let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let errors = report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["status"] == "error");
        assert_eq!(errors, rejected);
        assert_eq!(tree_snapshot::tree(home.path()), before);
    }
}

#[test]
fn bounded_parser_accepts_its_maximum_budget_for_nodes_entries_and_mixed_nesting() {
    for document in [
        format!("{}{}", "node {\n".repeat(128), "}\n".repeat(128)),
        format!("{}{}", "/- ".repeat(128), "node\n".repeat(129)),
        format!("node {}{}\n", "/- ".repeat(128), "1 ".repeat(129)),
        format!(
            "{}{}{}{}",
            "node {\n".repeat(64),
            "/- ".repeat(64),
            "node\n".repeat(65),
            "}\n".repeat(64)
        ),
    ] {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join(".config/zellij/config.kdl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, document).unwrap();
        let output = doctor(home.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert!(
            !report["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|check| check["status"] == "error"),
            "{report}"
        );
    }
}
