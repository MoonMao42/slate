//! Bounded, file-only shell diagnostics through the real CLI, never native shells.
use slate_cli::{config::ConfigWriteGuard, env::SlateEnv};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::TempDir;

#[path = "support/tree.rs"]
mod tree_snapshot;

fn command(home: &Path, target: &str) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .args(["doctor", target])
        .timeout(Duration::from_secs(5));
    command
}

fn startup(env: &SlateEnv, target: &str) -> PathBuf {
    match target {
        "bash" => env.bash_integration_path(),
        "zsh" => env.zshrc_path(),
        "fish" => env.fish_loader_path(),
        _ => unreachable!(),
    }
}

fn check<'a>(report: &'a serde_json::Value, code: &str) -> &'a serde_json::Value {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["code"] == code)
        .unwrap()
}

fn inspect(home: &Path, target: &str) -> serde_json::Value {
    let before = tree_snapshot::tree(home);
    let output = command(home, target)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_SOURCE"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_SOURCE"));
    assert_eq!(tree_snapshot::tree(home), before);
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn doctor_shell_cli_is_read_only_with_lock_and_never_launches_sources_or_shells() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    fs::create_dir(td.path().join("bin")).unwrap();
    for name in ["bash", "zsh", "fish", "brew", "apt-get"] {
        let stub = td.path().join("bin").join(name);
        fs::write(
            &stub,
            "#!/bin/sh\nprintf launched > \"$HOME/unexpected-launch\"\nexit 93\n",
        )
        .unwrap();
        fs::set_permissions(stub, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let _guard = ConfigWriteGuard::acquire(&env).unwrap();
    for target in ["bash", "zsh", "fish"] {
        let startup = startup(&env, target);
        let managed = env.config_dir().join(format!("managed/shell/env.{target}"));
        fs::create_dir_all(startup.parent().unwrap()).unwrap();
        fs::create_dir_all(managed.parent().unwrap()).unwrap();
        fs::write(
            &managed,
            "printf PRIVATE_SOURCE > \"$HOME/unexpected-source\"\n",
        )
        .unwrap();
        fs::write(
            &startup,
            format!("# PRIVATE_SOURCE\nsource '{}'\n", managed.display()),
        )
        .unwrap();
        fs::set_permissions(&startup, fs::Permissions::from_mode(0o640)).unwrap();
        let report = inspect(td.path(), target);
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["target"], target);
        assert_eq!(
            check(&report, "startup_file")["path"],
            startup.to_str().unwrap()
        );
        assert_eq!(check(&report, "loader_reference")["status"], "ok");
        assert_eq!(check(&report, "managed_environment")["status"], "ok");
        if target == "fish" {
            assert_eq!(check(&report, "loader_ownership")["status"], "info");
        }
        let before = tree_snapshot::tree(td.path());
        let output = command(td.path(), target)
            .assert()
            .success()
            .get_output()
            .clone();
        let text = String::from_utf8(output.stdout).unwrap();
        for check in report["checks"].as_array().unwrap() {
            assert!(text.contains(check["message"].as_str().unwrap()));
        }
        assert!(text.contains("execution is not verified"));
        assert!(!text.contains("PRIVATE_SOURCE"));
        assert_eq!(tree_snapshot::tree(td.path()), before);
        command(td.path(), target)
            .arg("--check-version")
            .assert()
            .failure();
        assert_eq!(tree_snapshot::tree(td.path()), before);
    }
    assert!(!td.path().join("unexpected-launch").exists());
    assert!(!td.path().join("unexpected-source").exists());
}

#[test]
fn doctor_shell_cli_reports_unsafe_sources_without_following_or_blocking() {
    for target in ["bash", "zsh", "fish"] {
        for managed in [false, true] {
            for kind in ["fifo", "directory", "oversized", "symlink"] {
                let td = TempDir::new().unwrap();
                let env = SlateEnv::with_home(td.path().into());
                let path = if managed {
                    env.config_dir().join(format!("managed/shell/env.{target}"))
                } else {
                    startup(&env, target)
                };
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                match kind {
                    "fifo" => {
                        let name =
                            std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
                        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
                    }
                    "directory" => fs::create_dir(&path).unwrap(),
                    "oversized" => fs::File::create(&path)
                        .unwrap()
                        .set_len(8 * 1024 * 1024 + 1)
                        .unwrap(),
                    "symlink" => {
                        let source = td.path().join("PRIVATE_SOURCE");
                        fs::write(&source, "do not follow\n").unwrap();
                        symlink(source, &path).unwrap();
                    }
                    _ => unreachable!(),
                }
                let report = inspect(td.path(), target);
                assert_eq!(
                    check(
                        &report,
                        if managed {
                            "managed_environment"
                        } else {
                            "startup_file"
                        }
                    )["status"],
                    "error",
                    "{target}/{managed}/{kind}"
                );
            }
        }
    }
}

#[test]
fn doctor_shell_cli_rejects_isolated_parent_escape_without_reading_external_file() {
    let td = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    fs::create_dir_all(env.config_dir().join("managed")).unwrap();
    symlink(external.path(), env.config_dir().join("managed/shell")).unwrap();
    for target in ["bash", "zsh", "fish"] {
        fs::write(
            external.path().join(format!("env.{target}")),
            "PRIVATE_SOURCE",
        )
        .unwrap();
        let before = tree_snapshot::tree(external.path());
        let report = inspect(td.path(), target);
        let problem = check(&report, "managed_environment");
        assert_eq!(problem["status"], "error");
        assert!(problem["message"]
            .as_str()
            .unwrap()
            .contains("escapes the isolated SLATE_HOME"));
        assert_eq!(tree_snapshot::tree(external.path()), before);
    }
}

#[test]
fn bash_startup_doctor_uses_login_precedence_and_reports_unsafe_candidates() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    fs::write(env.shell_profile_path(), "# shared user profile\n").unwrap();
    fs::write(env.bashrc_path(), "# user rc\n").unwrap();
    let selected = if cfg!(target_os = "macos") {
        env.shell_profile_path()
    } else {
        env.bashrc_path()
    };
    let report = inspect(td.path(), "bash");
    assert_eq!(
        check(&report, "startup_file")["path"],
        selected.to_str().unwrap()
    );
    symlink(td.path().join("missing"), env.bash_login_path()).unwrap();
    let report = inspect(td.path(), "bash");
    assert_eq!(
        check(&report, "startup_file")["status"],
        if cfg!(target_os = "macos") {
            "error"
        } else {
            "ok"
        }
    );
    assert!(!env.bash_profile_path().exists());
}
