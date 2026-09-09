use super::*;
use std::{
    cell::Cell,
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
};

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
}

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let temp = tempfile::tempdir().unwrap();
    // Not an isolated session: exercise notification ordering with a fake
    // callback. Never call apply() or a real terminal/cache helper here.
    let env =
        SlateEnv::from_vars(|key| (key == "HOME").then(|| temp.path().as_os_str().to_owned()))
            .unwrap();
    write(&env.managed_file("current-font"), "Old Mono");
    (temp, env)
}

#[test]
fn prepared_font_commits_choice_last_then_notifies_and_uses_new_prompt_mode() {
    let (_temp, env) = fixture();
    for family in ["New Mono Nerd Font", "Other Mono"] {
        let old = fs::read(env.managed_file("current-font")).unwrap();
        let prepared = PreparedFont::capture(&env, family).unwrap();
        assert!(prepared.changed());
        let notified = Cell::new(false);
        prepared
            .apply_with(
                |_| {
                    assert_eq!(fs::read(env.managed_file("current-font")).unwrap(), old);
                    assert!(!notified.get());
                    Ok(())
                },
                |_| {
                    assert_eq!(
                        fs::read(env.managed_file("current-font")).unwrap(),
                        family.as_bytes()
                    );
                    notified.set(true);
                },
            )
            .unwrap();
        assert!(notified.get());
        let actual = fs::read(env.managed_file("managed/shell/env.bash")).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&actual).contains("Apple_Terminal"),
            family.contains("Nerd Font")
        );
        let expected = ConfigManager::from_env_paths(&env)
            .font_shell_files(family)
            .unwrap();
        assert_eq!(
            actual,
            expected
                .iter()
                .find(|(_, name, _)| *name == "env.bash")
                .unwrap()
                .2
                .as_bytes()
        );
    }
}

#[test]
fn prepared_font_late_conflict_preserves_external_edit_and_has_file_recovery() {
    let (_temp, env) = fixture();
    let prepared = PreparedFont::capture(&env, "New Mono").unwrap();
    let targets = recovery_paths::targets(&env, prepared.paths(), "Font").unwrap();
    let point = crate::config::snapshot_font_targets_with_env(&env, &targets).unwrap();
    let conflict = env.managed_file("managed/alacritty/font.toml");
    let error = prepared
        .apply_with(
            |index| {
                if index == 1 {
                    write(&conflict, "# external editor");
                }
                Ok(())
            },
            |_| panic!("must not notify on failure"),
        )
        .unwrap_err();
    assert!(error.to_string().contains("changed after preparation"));
    assert_eq!(fs::read(&conflict).unwrap(), b"# external editor");
    assert_eq!(
        fs::read(env.managed_file("current-font")).unwrap(),
        b"Old Mono"
    );
    assert_eq!(
        fs::read(env.managed_file("managed/ghostty/font.conf")).unwrap(),
        font_config::ghostty("New Mono").unwrap().as_bytes()
    );
    // Deliberately request file recovery, including reverting the test's
    // external edit. This is not automatic rollback of someone else's work.
    assert!(!point.reapplies_theme());
    let restored = crate::config::execute_restore_with_env(&env, &point.id).unwrap();
    assert!(restored.is_fully_successful(), "{restored:?}");
    assert!(!env.managed_file("managed/ghostty/font.conf").exists());
    assert!(!conflict.exists());
    assert_eq!(
        fs::read(env.managed_file("current-font")).unwrap(),
        b"Old Mono"
    );
}

#[test]
fn prepared_font_changed_input_or_new_optional_entry_stops_before_any_write() {
    for input in [".config/slate/config.toml", ".config/kitty/kitty.conf"] {
        let (_temp, env) = fixture();
        let prepared = PreparedFont::capture(&env, "New Mono").unwrap();
        write(&env.home().join(input), "# external editor");
        assert!(prepared
            .apply_with(|_| panic!("preflight must stop"), |_| panic!("no reload"))
            .is_err());
        assert!(!env.managed_file("managed/ghostty/font.conf").exists());
        assert_eq!(
            fs::read(env.managed_file("current-font")).unwrap(),
            b"Old Mono"
        );
    }
}

#[test]
fn prepared_font_identical_repeat_keeps_file_identity_modes_and_times() {
    let (_temp, env) = fixture();
    PreparedFont::capture(&env, "New Mono")
        .unwrap()
        .apply_with(|_| Ok(()), |_| {})
        .unwrap();
    let repeat = PreparedFont::capture(&env, "New Mono").unwrap();
    assert!(!repeat.changed());
    let before: Vec<_> = repeat
        .paths()
        .filter_map(|path| {
            fs::metadata(&path)
                .ok()
                .map(|meta| (path, meta.ino(), meta.mode(), meta.modified().unwrap()))
        })
        .collect();
    let notified = Cell::new(false);
    repeat
        .apply_with(|_| Ok(()), |_| notified.set(true))
        .unwrap();
    assert!(notified.get());
    for (path, inode, mode, modified) in before {
        let meta = fs::metadata(path).unwrap();
        assert_eq!(
            (meta.ino(), meta.mode(), meta.modified().unwrap()),
            (inode, mode, modified)
        );
    }
}

#[test]
fn prepared_font_rejects_input_output_aliases_and_retargeted_parent() {
    for linked_input in [true, false] {
        let (temp, env) = fixture();
        let parent = env.managed_file("managed/ghostty");
        fs::create_dir_all(&parent).unwrap();
        if linked_input {
            write(&parent.join("font.conf"), "nord");
            symlink(parent.join("font.conf"), env.managed_file("current")).unwrap();
            let error = PreparedFont::capture(&env, "New Mono").err().unwrap();
            assert!(
                error.to_string().contains("aliases a read-only input"),
                "{error}"
            );
        } else {
            let prepared = PreparedFont::capture(&env, "New Mono").unwrap();
            fs::rename(&parent, temp.path().join("old-parent")).unwrap();
            fs::create_dir(temp.path().join("elsewhere")).unwrap();
            symlink(temp.path().join("elsewhere"), &parent).unwrap();
            assert!(prepared
                .apply_with(|_| panic!("preflight must stop"), |_| panic!("no reload"))
                .is_err());
            assert!(!temp.path().join("elsewhere/font.conf").exists());
        }
        assert_eq!(
            fs::read(env.managed_file("current-font")).unwrap(),
            b"Old Mono"
        );
    }
}

#[test]
fn prepared_font_isolated_session_never_requests_terminal_reload() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    PreparedFont::capture(&env, "New Mono")
        .unwrap()
        .apply_with(|_| Ok(()), |_| panic!("isolated reload"))
        .unwrap();
}
