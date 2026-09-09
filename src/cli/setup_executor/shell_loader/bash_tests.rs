use super::*;

#[test]
fn bash_startup_loader_rejects_managed_environment_rebinding_even_with_same_rc() {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    let plan = PreparedShellLoader::capture(&env, ShellBackend::Zsh).unwrap();
    let rebound = SlateEnv::from_vars(|name| match name {
        "HOME" => Some(td.path().as_os_str().to_owned()),
        "XDG_CONFIG_HOME" => Some(td.path().join("other-config").into_os_string()),
        _ => None,
    })
    .unwrap();
    let error = plan.publish(&rebound).unwrap_err().to_string();
    assert!(error.contains("environment path changed after preparation"));
    assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
}

#[cfg(target_os = "macos")]
#[test]
fn bash_startup_loader_rejects_selection_drift_before_publication() {
    use std::{fs, os::unix::fs::symlink};
    for change in ["higher-created", "higher-dangling", "selected-removed"] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        fs::write(env.shell_profile_path(), "# user shared profile\n").unwrap();
        let plan = PreparedShellLoader::capture(&env, ShellBackend::Bash).unwrap();
        match change {
            "higher-created" => {
                fs::write(env.bash_profile_path(), "# later login profile\n").unwrap()
            }
            "higher-dangling" => symlink(td.path().join("absent"), env.bash_login_path()).unwrap(),
            "selected-removed" => fs::remove_file(env.shell_profile_path()).unwrap(),
            _ => unreachable!(),
        }
        let before = super::tests::snapshot::tree(td.path());
        let error = plan.publish(&env).unwrap_err().to_string();
        assert!(error.contains("selected startup or environment path changed"));
        assert_eq!(super::tests::snapshot::tree(td.path()), before);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn bash_startup_loader_preserves_existing_profile_and_guards_other_shells() {
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};
    for shared in [false, true] {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        let path = if shared {
            env.shell_profile_path()
        } else {
            env.bash_login_path()
        };
        let user = "USER_PROFILE_LOADED=yes\n";
        fs::write(&path, user).unwrap();
        fs::write(env.bashrc_path(), "# user rc unchanged\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let plan = PreparedShellLoader::capture(&env, ShellBackend::Bash).unwrap();
        plan.publish(&env).unwrap();
        assert_eq!(plan.path, path);
        assert!(!env.bash_profile_path().exists());
        assert_eq!(
            fs::read(env.bashrc_path()).unwrap(),
            b"# user rc unchanged\n"
        );
        assert!(fs::read_to_string(&path).unwrap().starts_with(user));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        fs::create_dir_all(plan.managed.parent().unwrap()).unwrap();
        // Controlled payload, not the user's env or any generated tool commands.
        fs::write(&plan.managed, "SLATE_BASH_LOADED=yes\n").unwrap();
        let before = super::tests::snapshot::tree(td.path());
        for shell in if shared {
            &["bash", "zsh"][..]
        } else {
            &["bash"][..]
        } {
            let binary = which::which(shell).expect("macOS Bash/Zsh required");
            let mut process = assert_cmd::Command::new(binary);
            process
                .env_clear()
                .env("HOME", td.path())
                .env("PATH", "")
                .timeout(Duration::from_secs(5));
            if *shell == "bash" {
                process.args(["--noprofile", "--norc"]);
            } else {
                process.arg("-f");
            }
            process.args(["-c", ". \"$1\" || exit 71; printf '%s/%s' \"$USER_PROFILE_LOADED\" \"${SLATE_BASH_LOADED:-no}\"", "private-profile"])
                .arg(&path).assert().success().stdout(if *shell == "bash" { "yes/yes" } else { "yes/no" });
            assert_eq!(super::tests::snapshot::tree(td.path()), before);
        }
    }
}
