use super::*;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};

fn fixture() -> (tempfile::TempDir, SlateEnv, ConfigManager) {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    let config = ConfigManager::with_env(&env).unwrap();
    fs::write(env.managed_file("current-font"), "Private Mono").unwrap();
    fs::write(env.managed_file("config.toml"), "# keep comment\n[auto_theme]\nenabled = false # keep tail\n[private]\nvalue = 'PRIVATE_CONTENT'\n").unwrap();
    (td, env, config)
}

fn source(path: &Path) -> Option<Source> {
    file_read::read(path, MAX_TOOL_CONFIG_BYTES, Links::Reject).unwrap()
}

#[test]
fn prompt_preferences_keep_intent_until_files_succeed_and_preserve_personal_settings() {
    for preference in [
        ShellPreference::Starship(false),
        ShellPreference::Highlighting(false),
    ] {
        let (_td, env, config) = fixture();
        let path = env.managed_file("config.toml");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let original = source(&path);
        let plan = PreparedShellPreference::capture(&env, preference).unwrap();
        assert!(plan
            .publish_with(|index| {
                if index == 2 {
                    return Err(SlateError::InvalidConfig("fixture failure".into()));
                }
                Ok(())
            })
            .is_err());
        assert!(source(&path) == original);
        assert!(config.is_starship_enabled().unwrap());
        assert!(config.is_zsh_highlighting_enabled().unwrap());
        let plan = PreparedShellPreference::capture(&env, preference).unwrap();
        plan.publish().unwrap();
        assert_eq!(source(&path).unwrap().mode, Some(0o640));
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# keep comment") && saved.contains("PRIVATE_CONTENT"));
        assert_eq!(
            config.is_starship_enabled().unwrap(),
            !matches!(preference, ShellPreference::Starship(_))
        );
        assert_eq!(
            config.is_zsh_highlighting_enabled().unwrap(),
            !matches!(preference, ShellPreference::Highlighting(_))
        );
        assert!(!PreparedShellPreference::capture(&env, preference)
            .unwrap()
            .changed());
    }
}

#[test]
fn shell_preference_rejects_parent_retarget_during_initial_capture() {
    for existing in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(root.path().to_owned());
        let a = root.path().join("a");
        let b = root.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        if existing {
            fs::write(a.join("config.toml"), b"# unchanged\n").unwrap();
            fs::hard_link(a.join("config.toml"), b.join("config.toml")).unwrap();
        }
        let alias = root.path().join("alias");
        symlink(&a, &alias).unwrap();
        let path = alias.join("config.toml");
        let stable = File::capture(&env, path.clone(), MAX_DOCUMENT_BYTES).unwrap();
        stable.verify(&env).unwrap();
        let result = File::capture_with(&env, path, MAX_DOCUMENT_BYTES, |path| {
            let original = source(path);
            fs::remove_file(&alias).unwrap();
            symlink(&b, &alias).unwrap();
            Ok(original)
        });
        let error = result
            .err()
            .expect("retargeted source accepted")
            .to_string();
        assert!(error.contains("parent path changed while reading"));
        assert!(stable.verify(&env).is_err());
        if existing {
            assert_eq!(fs::read(a.join("config.toml")).unwrap(), b"# unchanged\n");
            assert_eq!(fs::read(b.join("config.toml")).unwrap(), b"# unchanged\n");
        } else {
            assert!(!a.join("config.toml").exists());
            assert!(!b.join("config.toml").exists());
        }
        assert!(!env.config_dir().exists());
        assert!(!env.slate_cache_dir().exists());
    }
}

#[test]
fn shell_preference_prepares_readonly_and_publishes_intent_last_preserving_comments_modes() {
    let (_td, env, config) = fixture();
    let document = env.managed_file("config.toml");
    fs::set_permissions(&document, fs::Permissions::from_mode(0o640)).unwrap();
    let original = source(&document);
    let plan = PreparedShellPreference::capture(&env, ShellPreference::AutoTheme(true)).unwrap();
    assert!(source(&document) == original);
    for file in &plan.files[..4] {
        assert!(!file.path.exists());
    }
    for file in &plan.files[1..4] {
        assert!(String::from_utf8_lossy(file.desired.as_ref().unwrap())
            .contains("slate-dark-mode-notify"));
    }
    plan.publish_with(|_| {
        assert!(!config.is_auto_theme_enabled()?);
        Ok(())
    })
    .unwrap();
    assert!(config.is_auto_theme_enabled().unwrap());
    let result = source(&document).unwrap();
    assert_eq!(result.mode, Some(0o640));
    assert_eq!(
        String::from_utf8(result.bytes).unwrap(),
        String::from_utf8(original.unwrap().bytes)
            .unwrap()
            .replace("enabled = false", "enabled = true")
    );
    assert_eq!(
        source(&env.managed_file("managed/shell/env.fish"))
            .unwrap()
            .mode,
        Some(0o600)
    );
    let again = PreparedShellPreference::capture(&env, ShellPreference::AutoTheme(true)).unwrap();
    assert!(!again.changed());
    let sources: Vec<_> = again.paths().iter().map(|path| source(path)).collect();
    again.publish().unwrap();
    assert!(
        again
            .paths()
            .iter()
            .map(|path| source(path))
            .collect::<Vec<_>>()
            == sources
    );
}

#[test]
fn shell_preference_failure_mid_publish_keeps_intent_and_untouched_later_files() {
    let (_td, env, config) = fixture();
    let document = source(&env.managed_file("config.toml"));
    let plan = PreparedShellPreference::capture(&env, ShellPreference::AutoTheme(true)).unwrap();
    let error = plan
        .publish_with(|index| {
            if index == 2 {
                return Err(SlateError::InvalidConfig("injected file failure".into()));
            }
            Ok(())
        })
        .unwrap_err();
    assert!(error.to_string().contains("injected"));
    assert!(plan.files[0].path.exists());
    assert!(plan.files[1].path.exists());
    assert!(!plan.files[2].path.exists());
    assert!(!plan.files[3].path.exists());
    assert!(source(&env.managed_file("config.toml")) == document);
    assert!(!config.is_auto_theme_enabled().unwrap());
}

#[test]
fn shell_preference_rejects_late_inputs_targets_and_parent_alias_changes() {
    for target in ["config.toml", "current-font", "managed/shell/env.fish"] {
        let (_td, env, _) = fixture();
        let plan =
            PreparedShellPreference::capture(&env, ShellPreference::Fastfetch(true)).unwrap();
        let path = env.managed_file(target);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "later edit").unwrap();
        assert!(plan.publish().unwrap_err().to_string().contains("changed"));
        assert_eq!(fs::read(&path).unwrap(), b"later edit");
        assert!(!env.managed_file("autorun-fastfetch").exists());
        assert!(!env.managed_file("managed/starship/plain.toml").exists());
    }
    let (_td, env, _) = fixture();
    let original = env.home().join("first-shell");
    let other = env.home().join("second-shell");
    fs::create_dir_all(&original).unwrap();
    fs::create_dir_all(&other).unwrap();
    fs::create_dir_all(env.managed_file("managed")).unwrap();
    let alias = env.managed_file("managed/shell");
    symlink(&original, &alias).unwrap();
    let plan = PreparedShellPreference::capture(&env, ShellPreference::Fastfetch(true)).unwrap();
    fs::remove_file(&alias).unwrap();
    symlink(&other, &alias).unwrap();
    assert!(plan.publish().is_err());
    assert_eq!(fs::read_dir(&other).unwrap().count(), 0);
}

#[test]
fn shell_preference_preserves_late_edits_between_generated_files() {
    let (_td, env, _) = fixture();
    let plan = PreparedShellPreference::capture(&env, ShellPreference::Fastfetch(true)).unwrap();
    let path = env.managed_file("managed/shell/env.bash");
    let error = plan
        .publish_with(|index| {
            if index == 2 {
                fs::write(&path, b"later edit")?;
            }
            Ok(())
        })
        .unwrap_err();
    assert!(error.to_string().contains("changed"));
    assert_eq!(fs::read(path).unwrap(), b"later edit");
    assert!(!env.managed_file("autorun-fastfetch").exists());
}

#[test]
fn shell_preference_fastfetch_roundtrip_and_noop_keep_unrelated_preferences() {
    let (_td, env, config) = fixture();
    let document = source(&env.managed_file("config.toml"));
    for enabled in [true, false, false] {
        let plan =
            PreparedShellPreference::capture(&env, ShellPreference::Fastfetch(enabled)).unwrap();
        plan.publish().unwrap();
        assert_eq!(config.has_fastfetch_autorun().unwrap(), enabled);
        assert!(source(&env.managed_file("config.toml")) == document);
        let again =
            PreparedShellPreference::capture(&env, ShellPreference::Fastfetch(enabled)).unwrap();
        assert!(!again.changed());
        again.publish().unwrap();
    }
}

#[test]
fn shell_preference_bad_documents_or_unsafe_outputs_fail_before_publication() {
    for variant in ["invalid-toml", "symlink", "directory", "oversize"] {
        let (_td, env, _) = fixture();
        let output = env.managed_file("managed/shell/env.fish");
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        match variant {
            "invalid-toml" => {
                fs::write(env.managed_file("config.toml"), "PRIVATE_CONTENT = [").unwrap()
            }
            "symlink" => symlink(env.managed_file("config.toml"), &output).unwrap(),
            "directory" => fs::create_dir(&output).unwrap(),
            "oversize" => fs::File::create(&output)
                .unwrap()
                .set_len(MAX_TOOL_CONFIG_BYTES + 1)
                .unwrap(),
            _ => unreachable!(),
        }
        let document = source(&env.managed_file("config.toml"));
        let error = PreparedShellPreference::capture(&env, ShellPreference::Fastfetch(true))
            .err()
            .unwrap()
            .to_string();
        assert!(!error.contains("PRIVATE_CONTENT"));
        assert!(source(&env.managed_file("config.toml")) == document);
        assert!(!env.managed_file("autorun-fastfetch").exists());
        assert!(!env.managed_file("managed/starship/plain.toml").exists());
    }
}
