use slate_cli::cli::theme_apply::{SnapshotPolicy, ThemeApplyCoordinator};
use slate_cli::config::{
    execute_restore_with_env, get_restore_point_with_env, list_restore_points_with_env,
    ConfigManager,
};
use slate_cli::env::SlateEnv;
use slate_cli::theme::ThemeRegistry;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

const BAD_FLAGS: &str = "[tools]\nstarship = 'PRIVATE_INVALID_BOOLEAN'\n";

fn private_cli(home: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("slate").unwrap();
    cmd.env_clear()
        .env("HOME", home)
        .env("SLATE_HOME", home)
        .env("PATH", home.join("bin"))
        .env("NO_COLOR", "1")
        .timeout(Duration::from_secs(10));
    cmd
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn saved_state(env: &SlateEnv) -> Vec<(PathBuf, String)> {
    let originals = [
        (env.managed_file("current"), "nord"),
        (env.managed_file("current-opacity"), "solid"),
        (env.managed_file("auto.toml"), "dark_theme = 'nord'\n"),
        (
            env.slate_cache_dir().join("current_theme.lua"),
            "return 'nord'\n",
        ),
    ];
    for (path, content) in &originals {
        write(path, content);
    }
    originals.into_iter().map(|(p, s)| (p, s.into())).collect()
}

fn assert_preserved(originals: &[(PathBuf, String)]) {
    for (path, content) in originals {
        assert_eq!(
            &fs::read_to_string(path).unwrap(),
            content,
            "{}",
            path.display()
        );
    }
}

#[test]
fn shared_shell_failure_keeps_the_last_committed_theme() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let config = ConfigManager::with_env(&env).unwrap();
    let originals = saved_state(&env);
    fs::write(env.managed_file("config.toml"), BAD_FLAGS).unwrap();
    let themes = ThemeRegistry::new().unwrap();
    let outcome = ThemeApplyCoordinator::new(&env).apply_to_tools(
        themes.get("catppuccin-mocha").unwrap(),
        &["ls_colors".into()],
    );
    assert!(outcome
        .as_ref()
        .map_or(true, |report| report.ensure_no_failures().is_err()));
    assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
    let error = outcome.unwrap_err().to_string();
    assert!(error.contains("shared shell configuration"), "{error}");
    assert!(!error.contains("PRIVATE_INVALID_BOOLEAN"));
    assert!(list_restore_points_with_env(&env).unwrap().is_empty());
    assert_preserved(&originals);
    assert!(!env.config_dir().join("managed/shell/env.zsh").exists());
}

#[test]
fn unsafe_shared_targets_stop_before_apply_and_can_be_retried_or_restored() {
    for blocked in ["managed/shell/env.bash", "current"] {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        let originals = saved_state(&env);
        let zsh = env.managed_file("managed/shell/env.zsh");
        write(&zsh, "# old zsh\n");
        let target = env.managed_file(blocked);
        let external = td.path().join("private-link-target");
        let old = if blocked == "current" {
            "nord"
        } else {
            "# old bash\n"
        };
        write(&external, old);
        fs::set_permissions(&external, fs::Permissions::from_mode(0o640)).unwrap();
        if target.exists() {
            fs::remove_file(&target).unwrap(); // Only the private test fixture.
        }
        symlink(&external, &target).unwrap();
        let themes = ThemeRegistry::new().unwrap();
        let error = ThemeApplyCoordinator::new(&env)
            .apply_to_tools(
                themes.get("catppuccin-mocha").unwrap(),
                &["ls_colors".into()],
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("final symlink"), "{error}");
        assert!(list_restore_points_with_env(&env).unwrap().is_empty());
        assert_preserved(&originals);
        assert_eq!(fs::read_to_string(&zsh).unwrap(), "# old zsh\n");
        assert!(!env.managed_file("managed/starship/plain.toml").exists());
        assert_eq!(fs::read_link(&target).unwrap(), external);
        assert_eq!(fs::read_to_string(&external).unwrap(), old);
        assert_eq!(
            fs::metadata(&external).unwrap().permissions().mode() & 0o777,
            0o640
        );
        // The operator repairs the unsafe entry, then retries normally. Slate
        // must never silently remove a symlink or write through it itself.
        fs::remove_file(&target).unwrap();
        write(&target, old);
        let retry = ThemeApplyCoordinator::new(&env)
            .apply_to_tools(
                themes.get("catppuccin-mocha").unwrap(),
                &["ls_colors".into()],
            )
            .unwrap();
        retry.ensure_no_failures().unwrap();
        let id = retry.restore_point_id.as_ref().unwrap();
        let point = get_restore_point_with_env(&env, id).unwrap();
        assert!(!point.reapplies_theme());
        let entry = point
            .entries
            .iter()
            .find(|e| e.original_path == zsh)
            .unwrap();
        assert_eq!(
            fs::read_to_string(entry.backup_path.as_ref().unwrap()).unwrap(),
            "# old zsh\n"
        );
        assert_eq!(
            config.get_current_theme().unwrap().as_deref(),
            Some("catppuccin-mocha")
        );
        assert!(
            fs::read_to_string(env.slate_cache_dir().join("current_theme.lua"))
                .unwrap()
                .contains("catppuccin-mocha")
        );
        assert!(execute_restore_with_env(&env, id)
            .unwrap()
            .is_fully_successful());
        assert_eq!(fs::read_to_string(&zsh).unwrap(), "# old zsh\n");
        assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
        assert_eq!(fs::read_to_string(&external).unwrap(), old);
    }
}

#[test]
fn no_target_commit_failure_is_not_success_and_does_not_invent_a_snapshot() {
    let td = TempDir::new().unwrap();
    let env = SlateEnv::with_home(td.path().to_owned());
    let _config = ConfigManager::with_env(&env).unwrap();
    let originals = saved_state(&env);
    fs::write(env.managed_file("config.toml"), BAD_FLAGS).unwrap();
    let themes = ThemeRegistry::new().unwrap();
    let error = ThemeApplyCoordinator::with_snapshot_policy(&env, SnapshotPolicy::Skip)
        .apply_to_tools(themes.get("catppuccin-mocha").unwrap(), &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("shared shell configuration"));
    assert!(!error.contains("slate restore"));
    assert!(list_restore_points_with_env(&env).unwrap().is_empty());
    assert_preserved(&originals);
}

#[test]
fn theme_cli_reports_shared_failure_even_when_quiet() {
    for quiet in [false, true] {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let _config = ConfigManager::with_env(&env).unwrap();
        let originals = saved_state(&env);
        fs::write(env.managed_file("config.toml"), BAD_FLAGS).unwrap();
        // A supported private Neovim fixture must still receive no new state
        // when the later shared-shell commit fails.
        let nvim = td.path().join("bin/nvim");
        write(&nvim, "#!/bin/sh\nprintf 'NVIM v0.8.0\\n'\n");
        fs::set_permissions(&nvim, fs::Permissions::from_mode(0o755)).unwrap();
        let mut cmd = private_cli(td.path());
        if quiet {
            cmd.arg("--quiet");
        }
        let output = cmd.args(["theme", "catppuccin-mocha"]).output().unwrap();
        assert!(!output.status.success());
        let err = String::from_utf8_lossy(&output.stderr);
        assert!(
            err.contains("No theme files were written")
                && err.contains("shared shell configuration"),
            "{err}"
        );
        assert!(!err.contains("PRIVATE_INVALID_BOOLEAN"));
        let points = list_restore_points_with_env(&env).unwrap();
        assert!(points.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Theme switched"));
        assert_preserved(&originals);
    }
}
