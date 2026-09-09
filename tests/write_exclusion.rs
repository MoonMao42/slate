//! Real command entrypoints under contention; all data lives in a fixture HOME.
use slate_cli::{config::ConfigWriteGuard, env::SlateEnv};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn command(home: &Path, args: &[&str]) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env("SLATE_HOME", home)
        .env("NO_COLOR", "1")
        // Defense in depth: even a regressed cleanup path cannot find pkill;
        // invalid setup/font arguments also prevent package installation.
        .env("PATH", "")
        .args(args)
        .timeout(Duration::from_secs(10));
    command
}

fn tree(root: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    fn visit(path: &Path, entries: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        entries.insert(
            path.to_owned(),
            (
                metadata.permissions().mode(),
                if metadata.is_file() {
                    fs::read(path).unwrap()
                } else {
                    Vec::new()
                },
            ),
        );
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
fn direct_writers_are_excluded_but_read_only_commands_remain_available() {
    let td = tempfile::TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let guard = ConfigWriteGuard::acquire(&env).unwrap();
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::write(
        env.managed_file("config.toml"),
        "[preferences]\nsound = true\n",
    )
    .unwrap();
    let before = tree(td.path());
    let writers: &[&[&str]] = &[
        &["theme", "nord"],
        &["set", "nord"],
        &["theme", "--auto", "--quiet"],
        &["config", "set", "sound", "off"],
        &["font", "__invalid_fixture_font"],
        &["setup", "--only", "__invalid_fixture_tool"],
        &["restore", "missing"],
        &["restore", "--delete", "missing"],
        &["reset", "missing"],
        &["import", "slate://none/none/solid/none"],
        &["clean"],
    ];
    for args in writers {
        command(td.path(), args)
            .assert()
            .failure()
            .stderr(predicates::str::contains(
                if args.first() == Some(&"setup") {
                    // Exact-tool validation precedes mutation/lock acquisition.
                    "Unknown tool: '__invalid_fixture_tool'"
                } else {
                    "still running"
                },
            ));
        assert_eq!(
            tree(td.path()),
            before,
            "blocked writer changed files: {args:?}"
        );
    }
    for args in [
        &["status", "--json"][..],
        &["doctor", "nvim", "--json"][..],
        &["list"][..],
        &["theme", "--list"][..],
        &["restore", "--list"][..],
        &["export"][..],
    ] {
        command(td.path(), args).assert().success();
        assert_eq!(tree(td.path()), before, "reader changed files: {args:?}");
    }
    let output = command(td.path(), &["status", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["recovery"]["status"], "busy");
    drop(guard);

    let record = env.slate_cache_dir().join("preview-session.json");
    fs::write(&record, b"unfinished recovery fixture").unwrap();
    fs::set_permissions(&record, fs::Permissions::from_mode(0o600)).unwrap();
    let before = tree(td.path());
    for args in writers {
        command(td.path(), args)
            .assert()
            .failure()
            .stderr(predicates::str::contains(
                if args.first() == Some(&"setup") {
                    "Unknown tool: '__invalid_fixture_tool'"
                } else {
                    "slate recover --dry-run"
                },
            ));
        assert_eq!(
            tree(td.path()),
            before,
            "pending preview was modified: {args:?}"
        );
    }
    command(td.path(), &["recover", "--discard", "--yes"])
        .assert()
        .success();
    // Main -> config handler nests two guards; neither deadlocks nor leaks its lock.
    command(td.path(), &["config", "set", "sound", "off"])
        .assert()
        .success();
    assert!(fs::read_to_string(env.managed_file("config.toml"))
        .unwrap()
        .contains("sound = false"));
    command(td.path(), &["status", "--json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\": \"clear\""));
}
