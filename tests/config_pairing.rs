//! Pairing commands never invoke host tools; all fixtures live in private profiles.
use serde_json::Value;
use slate_cli::{
    config::{list_restore_points_with_env, ConfigManager, ConfigWriteGuard},
    env::SlateEnv,
};
use std::{
    ffi::CString,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    time::Duration,
};

#[path = "config_pairing/clear.rs"]
mod clear;
#[path = "config_pairing/interactive.rs"]
mod interactive;
#[path = "support/tree.rs"]
mod snapshot;

fn command(home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(8));
    command
}

fn report(home: &Path, args: &[&str]) -> Value {
    let output = command(home)
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn fixture(home: &Path) -> SlateEnv {
    let env = SlateEnv::with_home(home.into());
    for name in [
        "defaults",
        "gsettings",
        "pgrep",
        "starship",
        "fastfetch",
        "nvim",
        "osascript",
        "fc-list",
    ] {
        let executable = home.join("bin").join(name);
        write(
            &executable,
            "#!/bin/sh\nprintf native > \"$HOME/UNEXPECTED\"\nexit 99\n",
        );
        fs::set_permissions(executable, fs::Permissions::from_mode(0o700)).unwrap();
    }
    env
}

#[test]
fn config_pairing_absent_profile_inspection_and_preview_create_nothing() {
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("absent home");
    let read = report(&home, &["config", "pairing", "--json"]);
    assert_eq!(read["schema_version"], 1);
    assert_eq!(read["action"], "inspect");
    assert_eq!(read["pairing"]["dark"]["status"], "unset");
    assert_eq!(read["pairing"]["light"]["status"], "unset");
    let preview = report(
        &home,
        &["config", "pairing", "--dark", "nord", "--dry-run", "--json"],
    );
    assert_eq!(preview["action"], "preview");
    assert_eq!(preview["changed"], true);
    assert_eq!(preview["before"]["dark"]["status"], "unset");
    assert_eq!(preview["pairing"]["dark"]["theme_id"], "nord");
    assert_eq!(preview["pairing"]["light"]["status"], "unset");
    assert!(preview.get("restore_point_id").is_none());
    command(&home)
        .args(["config", "pairing", "--dark", "nord", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("dark: unset -> nord"))
        .stdout(predicates::str::contains("light: unset (unchanged)"))
        .stdout(predicates::str::contains("no files changed"));
    assert!(!home.exists());
}

#[test]
fn config_pairing_cli_saves_partial_changes_without_touching_watchers_shells_or_current_theme() {
    let td = tempfile::tempdir().unwrap();
    let env = fixture(td.path());
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_auto_theme_enabled(true).unwrap();
    write(
        &env.managed_file("auto.toml"),
        "# note\ndark_theme = 'nord'\nlight_theme = 'catppuccin-latte' # light\nextra=7\n",
    );
    let pair = env.managed_file("auto.toml");
    fs::set_permissions(&pair, fs::Permissions::from_mode(0o640)).unwrap();
    for name in [
        "current",
        "managed/shell/env.bash",
        "managed/bin/slate-dark-mode-notify",
        "managed/bin/slate-appearance-helper",
    ] {
        write(&env.managed_file(name), format!("unchanged {name}"));
    }
    let before = snapshot::tree(env.config_dir());
    let saved = report(
        td.path(),
        &["config", "pairing", "--dark", "catppuccin-mocha", "--json"],
    );
    assert_eq!(saved["action"], "saved");
    assert_eq!(saved["pairing"]["dark"]["theme_id"], "catppuccin-mocha");
    assert_eq!(saved["pairing"]["light"]["theme_id"], "catppuccin-latte");
    assert!(saved["restore_point_id"].is_string());
    assert!(config.is_auto_theme_enabled().unwrap());
    let after = snapshot::tree(env.config_dir());
    for (path, state) in before {
        if path != pair {
            assert_eq!(after.get(&path), Some(&state));
        }
    }
    let bytes = fs::read_to_string(&pair).unwrap();
    assert!(bytes.contains("# note"));
    assert!(bytes.contains("light_theme = 'catppuccin-latte' # light"));
    assert!(bytes.contains("extra=7"));
    assert_eq!(
        fs::metadata(&pair).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let before_repeat = snapshot::tree(td.path());
    let repeated = report(
        td.path(),
        &["config", "pairing", "--dark", "catppuccin-mocha", "--json"],
    );
    assert_eq!(repeated["action"], "unchanged");
    assert_eq!(repeated["changed"], false);
    assert!(repeated.get("restore_point_id").is_none());
    assert_eq!(snapshot::tree(td.path()), before_repeat);
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].entries.len(), 1);
    assert_eq!(points[0].entries[0].original_path, pair);
    assert!(!td.path().join("UNEXPECTED").exists());
}

#[test]
fn config_pairing_reads_and_previews_work_under_lock_and_pending_recovery_but_saves_do_not() {
    let td = tempfile::tempdir().unwrap();
    let env = fixture(td.path());
    let guard = ConfigWriteGuard::acquire(&env).unwrap();
    write(
        &env.slate_cache_dir().join("preview-session.json"),
        "PRIVATE_RECOVERY",
    );
    let before = snapshot::tree(td.path());
    report(td.path(), &["config", "pairing", "--json"]);
    report(
        td.path(),
        &[
            "config",
            "pairing",
            "--light",
            "catppuccin-latte",
            "--dry-run",
            "--json",
        ],
    );
    command(td.path())
        .args(["config", "pairing", "--dark", "nord"])
        .assert()
        .failure();
    assert_eq!(snapshot::tree(td.path()), before);
    drop(guard);
    command(td.path())
        .args(["config", "pairing", "--dark", "nord"])
        .assert()
        .failure();
    assert_eq!(snapshot::tree(td.path()), before);
}

#[test]
fn config_pairing_argument_preflight_and_noninteractive_configure_fail_without_creating_a_profile()
{
    let td = tempfile::tempdir().unwrap();
    let home = td.path().join("absent");
    for args in [
        vec!["config", "pairing", "--dark", "catppuccin-latte"],
        vec!["config", "pairing", "--light", "nord"],
        vec!["config", "pairing", "--dark", "PRIVATE_UNKNOWN\x1b[2J"],
        vec!["config", "pairing", "--dry-run"],
        vec!["config", "set", "auto-theme", "configure"],
    ] {
        let output = command(&home)
            .env_remove("HOME")
            .env_remove("SLATE_HOME")
            .args(args)
            .assert()
            .failure()
            .get_output()
            .clone();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!error.contains("PRIVATE_UNKNOWN"));
        assert!(!error.contains("HOME environment"));
        assert!(!home.exists());
    }
}

#[test]
fn config_pairing_unsafe_or_invalid_documents_report_errors_and_refuse_saves_without_leaks() {
    for kind in [
        "symlink",
        "fifo",
        "directory",
        "toml",
        "unknown",
        "wrong-type",
    ] {
        let td = tempfile::tempdir().unwrap();
        let env = fixture(td.path());
        let path = env.managed_file("auto.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        match kind {
            "symlink" => {
                write(&td.path().join("target"), "PRIVATE_CONTENT");
                symlink(td.path().join("target"), &path).unwrap();
            }
            "fifo" => {
                let name = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "directory" => fs::create_dir(&path).unwrap(),
            "toml" => write(&path, "PRIVATE_CONTENT = ["),
            "unknown" => write(
                &path,
                "dark_theme='PRIVATE_CONTENT'\nlight_theme='catppuccin-latte'\n",
            ),
            "wrong-type" => write(&path, "dark_theme=123\n"),
            _ => unreachable!(),
        }
        let before = snapshot::tree(td.path());
        let inspection = report(td.path(), &["config", "pairing", "--json"]);
        assert_eq!(inspection["pairing"]["dark"]["status"], "error");
        assert!(!inspection.to_string().contains("PRIVATE_CONTENT"));
        if kind != "unknown" {
            let output = command(td.path())
                .args(["config", "pairing", "--dark", "nord", "--json"])
                .assert()
                .failure()
                .get_output()
                .clone();
            assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_CONTENT"));
        } else {
            assert_eq!(
                inspection["pairing"]["light"]["theme_id"],
                "catppuccin-latte"
            );
        }
        assert_eq!(snapshot::tree(td.path()), before);
    }
}
