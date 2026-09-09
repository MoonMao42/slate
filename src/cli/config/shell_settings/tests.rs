use super::*;
use crate::{
    config::{execute_restore_with_env, list_restore_points_with_env, RestorePoint},
    env::SlateEnv,
};
use std::{cell::Cell, fs, os::unix::fs::PermissionsExt, path::Path};

fn fixture() -> (tempfile::TempDir, ConfigManager) {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    let config = ConfigManager::with_env(&env).unwrap();
    fs::write(env.managed_file("current-font"), "Private Mono").unwrap();
    config.set_auto_theme_enabled(true).unwrap();
    (td, config)
}

fn points(config: &ConfigManager) -> Vec<RestorePoint> {
    list_restore_points_with_env(config.environment())
        .unwrap()
        .into_iter()
        .filter(|point| point.theme_name == "pre-config")
        .collect()
}

fn fail() -> Result<()> {
    Err(SlateError::InvalidConfig("injected failure".into()))
}

fn write(path: &Path, bytes: &[u8], mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn checkpoint_notice_keeps_exact_review_command_without_duplicate_menu_id() {
    let id = "2026-09-09T10-00-00Z-123-0000";
    let compact = checkpoint_notice(id, true);
    assert_eq!(compact.lines().count(), 1);
    assert_eq!(compact.matches(id).count(), 1);
    assert!(compact.contains(&format!("slate restore {id} --dry-run")));
    let full = checkpoint_notice(id, false);
    assert_eq!(
        full,
        format!(
            "Pre-config recovery point: {id}\nInspect file recovery: slate restore {id} --dry-run"
        )
    );
}

#[test]
fn shell_settings_checkpoints_restore_all_three_preferences_and_generated_files() {
    for preference in [
        ShellPreference::Fastfetch(true),
        ShellPreference::Starship(false),
        ShellPreference::Highlighting(false),
    ] {
        let (_td, config) = fixture();
        let env = config.environment();
        let zsh = env.managed_file("managed/shell/env.zsh");
        write(&zsh, b"# previous shell bytes\n", 0o640);
        let document = env.managed_file("config.toml");
        let mut bytes = fs::read(&document).unwrap();
        bytes.extend_from_slice(b"\n[personal]\nnote = 'keep this'\n");
        write(&document, &bytes, 0o640);
        let paths = PreparedShellPreference::capture(env, preference)
            .unwrap()
            .paths();
        let state = |path: &Path| {
            fs::read(path)
                .ok()
                .map(|bytes| (bytes, fs::metadata(path).unwrap().permissions().mode()))
        };
        let before: Vec<_> = paths.iter().map(|path| state(path)).collect();
        apply_with(&config, preference, |_| Ok(()), |_| Ok(())).unwrap();
        let points = points(&config);
        assert_eq!(points.len(), 1);
        assert!(paths.iter().map(|path| state(path)).collect::<Vec<_>>() != before);
        let receipt = execute_restore_with_env(env, &points[0].id).unwrap();
        assert!(receipt.results.iter().all(|result| result.success));
        assert_eq!(
            paths.iter().map(|path| state(path)).collect::<Vec<_>>(),
            before
        );
        assert!(config.is_starship_enabled().unwrap());
        assert!(config.is_zsh_highlighting_enabled().unwrap());
        assert!(!config.has_fastfetch_autorun().unwrap());
    }
}

#[test]
fn shell_settings_preflight_and_snapshot_failures_never_change_existing_enabled_flag() {
    for bad_backup in [false, true] {
        let (_td, config) = fixture();
        let env = config.environment();
        let blocked = if bad_backup {
            env.slate_cache_dir().join("backups")
        } else {
            env.managed_file("managed/shell/env.fish")
        };
        if bad_backup {
            fs::remove_dir(&blocked).unwrap();
            write(&blocked, b"private", 0o600);
        } else {
            fs::create_dir_all(&blocked).unwrap();
        }
        let before = fs::read(env.managed_file("config.toml")).unwrap();
        let called = Cell::new(false);
        assert!(apply_with(
            &config,
            ShellPreference::AutoTheme(true),
            |_| {
                called.set(true);
                Ok(())
            },
            |_| {
                called.set(true);
                Ok(())
            }
        )
        .is_err());
        assert!(!called.get());
        assert_eq!(fs::read(env.managed_file("config.toml")).unwrap(), before);
        assert!(config.is_auto_theme_enabled().unwrap());
        assert!(!env.managed_file("managed/starship/plain.toml").exists());
        if !bad_backup {
            assert!(points(&config).is_empty());
        }
    }
}

#[test]
fn shell_settings_helper_failure_retains_preference_and_recoverable_helper_bytes() {
    let (_td, config) = fixture();
    let env = config.environment();
    let helper = env.managed_file("managed/bin/slate-dark-mode-notify");
    write(&helper, b"original helper", 0o750);
    let called = Cell::new(false);
    let error = apply_with(
        &config,
        ShellPreference::AutoTheme(true),
        |_| {
            write(&helper, b"partial helper", 0o700);
            fail()
        },
        |_| {
            called.set(true);
            Ok(())
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("helper preparation failed"));
    assert!(!called.get());
    assert!(config.is_auto_theme_enabled().unwrap());
    assert!(!env.managed_file("managed/shell/env.bash").exists());
    let points = points(&config);
    assert_eq!(points.len(), 1);
    assert!(error.contains(&points[0].id));
    let receipt = execute_restore_with_env(env, &points[0].id).unwrap();
    assert!(receipt.results.iter().all(|result| result.success));
    assert_eq!(fs::read(&helper).unwrap(), b"original helper");
    assert_eq!(
        fs::metadata(&helper).unwrap().permissions().mode() & 0o777,
        0o750
    );
}

#[test]
fn shell_settings_lifecycle_failure_keeps_saved_intent_and_file_only_checkpoint() {
    for enabled in [true, false] {
        let (_td, config) = fixture();
        let env = config.environment();
        config.set_auto_theme_enabled(!enabled).unwrap();
        let document = env.managed_file("config.toml");
        let original = fs::read(&document).unwrap();
        fs::set_permissions(&document, fs::Permissions::from_mode(0o640)).unwrap();
        let shell = env.managed_file("managed/shell/env.bash");
        write(&shell, b"original shell", 0o644);
        let error = apply_with(
            &config,
            ShellPreference::AutoTheme(enabled),
            |_| Ok(()),
            |_| {
                assert_eq!(config.is_auto_theme_enabled().unwrap(), enabled);
                assert_ne!(fs::read(&shell).unwrap(), b"original shell");
                fail()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("watcher lifecycle did not finish"));
        assert!(error.contains("file recovery does not restore processes"));
        assert_eq!(config.is_auto_theme_enabled().unwrap(), enabled);
        let points = points(&config);
        assert_eq!(points.len(), 1);
        let point = &points[0];
        assert_eq!(point.entries.len(), 7);
        assert!(!point.reapplies_theme());
        assert!(error.contains(&point.id));
        assert!(execute_restore_with_env(env, &point.id)
            .unwrap()
            .results
            .iter()
            .all(|result| result.success));
        assert_eq!(fs::read(&document).unwrap(), original);
        assert_eq!(
            fs::metadata(&document).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(fs::read(&shell).unwrap(), b"original shell");
        assert_eq!(
            fs::metadata(&shell).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert!(!env.managed_file("managed/shell/env.fish").exists());
        assert!(!env
            .managed_file("managed/bin/slate-appearance-helper")
            .exists());
    }
}

#[test]
fn shell_settings_change_after_checkpoint_is_preserved_with_recovery_guidance() {
    let (_td, config) = fixture();
    let env = config.environment();
    let shell = env.managed_file("managed/shell/env.fish");
    let after = Cell::new(false);
    let error = apply_with(
        &config,
        ShellPreference::Fastfetch(true),
        |_| {
            write(&shell, b"later external edit", 0o600);
            Ok(())
        },
        |_| {
            after.set(true);
            Ok(())
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("file update was incomplete"));
    assert!(error.contains(&points(&config)[0].id));
    assert!(!after.get());
    assert_eq!(fs::read(&shell).unwrap(), b"later external edit");
    assert!(!config.has_fastfetch_autorun().unwrap());
    assert!(!env.managed_file("managed/starship/plain.toml").exists());
}

#[test]
fn shell_settings_fastfetch_identical_repeat_does_not_create_checkpoint_or_replace_files() {
    let (_td, config) = fixture();
    apply(&config, ShellPreference::Fastfetch(true)).unwrap();
    let point = points(&config).pop().unwrap();
    assert_eq!(point.entries.len(), 5);
    let plan =
        PreparedShellPreference::capture(config.environment(), ShellPreference::Fastfetch(true))
            .unwrap();
    apply(&config, ShellPreference::Fastfetch(true)).unwrap();
    assert_eq!(points(&config).len(), 1);
    assert_eq!(points(&config)[0].id, point.id);
    // Includes identity/mode/bytes: even identical replacements fail verification.
    plan.verify().unwrap();
}
