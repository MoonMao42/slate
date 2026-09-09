use super::*;
use crate::config::{execute_restore_with_env, list_restore_points_with_env};
use std::os::unix::fs::{symlink, PermissionsExt};

fn fixture() -> (tempfile::TempDir, SlateEnv) {
    let td = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(td.path().into());
    (td, env)
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn pairing_save_preserves_partial_values_comments_modes_and_noop_identity() {
    let (_td, env) = fixture();
    let path = env.managed_file("auto.toml");
    let original = b"# private note\ndark_theme = 'nord' # tail\nlight_theme = 'catppuccin-latte'\nextra = 7\n";
    write(&path, original);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let plan = PreparedPairing::capture(&env, Some("catppuccin-mocha"), None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(plan.before.dark_theme.as_deref(), Some("nord"));
    let id = plan.save().unwrap().unwrap();
    let bytes = fs::read_to_string(&path).unwrap();
    assert!(bytes.contains("# private note"));
    assert!(bytes.contains("# tail"));
    assert!(bytes.contains("light_theme = 'catppuccin-latte'"));
    assert!(bytes.contains("extra = 7"));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let before = read(&env, &path).unwrap();
    let again = PreparedPairing::capture(&env, Some("catppuccin-mocha"), None).unwrap();
    assert!(!again.changed());
    assert!(again.save().unwrap().is_none());
    assert!(read(&env, &path).unwrap() == before);
    let points = list_restore_points_with_env(&env).unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].entries.len(), 1);
    assert_eq!(points[0].entries[0].original_path, path);
    assert!(!points[0].reapplies_theme());
    assert!(execute_restore_with_env(&env, &id)
        .unwrap()
        .results
        .iter()
        .all(|entry| entry.success));
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn pairing_preparation_is_readonly_and_new_file_recovery_preserves_absence() {
    let (_td, env) = fixture();
    let plan = PreparedPairing::capture(&env, Some("nord"), Some("catppuccin-latte")).unwrap();
    assert_eq!(fs::read_dir(env.home()).unwrap().count(), 0);
    assert!(plan.changed());
    let id = plan.save().unwrap().unwrap();
    assert_eq!(
        fs::metadata(&plan.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!env.managed_file("config.toml").exists());
    assert!(!env.managed_file("current").exists());
    assert!(!env.managed_file("managed").exists());
    assert!(execute_restore_with_env(&env, &id)
        .unwrap()
        .results
        .iter()
        .all(|entry| entry.success));
    assert!(!plan.path.exists());
}

#[test]
fn pairing_detected_late_changes_never_overwrite_new_content_and_retain_checkpoint_id() {
    for after_snapshot in [false, true] {
        let (_td, env) = fixture();
        let path = env.managed_file("auto.toml");
        write(&path, b"dark_theme='nord'\n");
        let plan = PreparedPairing::capture(&env, Some("catppuccin-mocha"), None).unwrap();
        let late = b"dark_theme='nord'\n# later editor\n";
        if !after_snapshot {
            fs::write(&path, late).unwrap();
        }
        let error = plan
            .save_with(|_| {
                fs::write(&path, late).unwrap();
            })
            .unwrap_err()
            .to_string();
        assert!(error.contains("changed after preparation"));
        assert_eq!(fs::read(&path).unwrap(), late);
        let points = list_restore_points_with_env(&env).unwrap();
        assert_eq!(points.len(), usize::from(after_snapshot));
        if after_snapshot {
            assert!(error.contains(&format!("slate restore {} --dry-run", points[0].id)));
        }
    }
    let (_td, env) = fixture();
    let plan = PreparedPairing::capture(&env, Some("nord"), None).unwrap();
    let other = env.home().join("other");
    fs::create_dir_all(&other).unwrap();
    fs::create_dir_all(env.config_dir().parent().unwrap()).unwrap();
    symlink(&other, env.config_dir()).unwrap();
    assert!(plan.save().is_err());
    assert_eq!(fs::read_dir(&other).unwrap().count(), 0);
}

#[test]
fn pairing_backup_failure_or_invalid_source_prevents_publication() {
    for invalid in ["toml", "type", "size", "backup"] {
        let (_td, env) = fixture();
        let path = env.managed_file("auto.toml");
        match invalid {
            "toml" => write(&path, b"PRIVATE_PAIR = ["),
            "type" => write(&path, b"dark_theme=123\n"),
            "size" => {
                write(&path, b"");
                fs::File::create(&path)
                    .unwrap()
                    .set_len(MAX_DOCUMENT_BYTES + 1)
                    .unwrap();
            }
            "backup" => {
                write(&path, b"dark_theme='nord'\n");
                write(&env.slate_cache_dir().join("backups"), b"blocked");
            }
            _ => unreachable!(),
        }
        let before = fs::read(&path).unwrap();
        let error = PreparedPairing::capture(&env, Some("catppuccin-mocha"), None)
            .and_then(|plan| plan.save())
            .unwrap_err()
            .to_string();
        assert!(!error.contains("PRIVATE_PAIR"));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn pairing_clear_preserves_comments_and_other_values_without_retaining_removed_value_content() {
    for (both, newline) in [(false, "\n"), (true, "\n"), (true, "\r\n")] {
        let (_td, env) = fixture();
        let path = env.managed_file("auto.toml");
        // Multiline string contents containing '#' are not comment decoration.
        let original = "# header\n\"dark_theme\" = '''PRIVATE_PAIR # not a comment\ncontinued''' # dark note\n# light note\nlight_theme = 'catppuccin-latte' # light tail\nextra = { key = 7 }\n[private]\nkeep = 'yes'\n# footer".replace('\n', newline);
        write(&path, original.as_bytes());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let plan = PreparedPairing::capture_edits(
            &env,
            SlotEdit::Clear,
            if both {
                SlotEdit::Clear
            } else {
                SlotEdit::Keep
            },
        )
        .unwrap();
        assert!(plan.after.dark_theme.is_none());
        assert_eq!(plan.after.light_theme.is_none(), both);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let id = plan.save().unwrap().unwrap();
        let bytes = fs::read_to_string(&path).unwrap();
        for comment in [
            "# header",
            "# dark note",
            "# light note",
            "# light tail",
            "# footer",
        ] {
            assert_eq!(bytes.matches(comment).count(), 1, "{bytes}");
        }
        assert!(!bytes.contains("PRIVATE_PAIR"));
        assert!(!bytes.contains("not a comment"));
        let parsed: toml::Value = toml::from_str(&bytes).unwrap();
        assert!(parsed.get("dark_theme").is_none());
        assert_eq!(parsed.get("light_theme").is_none(), both);
        assert_eq!(parsed["private"]["keep"].as_str(), Some("yes"));
        assert_eq!(parsed["extra"]["key"].as_integer(), Some(7));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(execute_restore_with_env(&env, &id)
            .unwrap()
            .results
            .iter()
            .all(|entry| entry.success));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}

#[test]
fn pairing_clear_keeps_absence_and_identical_files_but_never_deletes_existing_empty_documents() {
    for original in [None, Some("# keep\nextra = 7\n"), Some("")] {
        let (_td, env) = fixture();
        let path = env.managed_file("auto.toml");
        if let Some(original) = original {
            write(&path, original.as_bytes());
        }
        let before = read(&env, &path).unwrap();
        let plan = PreparedPairing::capture_edits(&env, SlotEdit::Clear, SlotEdit::Clear).unwrap();
        assert!(!plan.changed());
        assert!(plan.save().unwrap().is_none());
        assert!(read(&env, &path).unwrap() == before);
        assert!(list_restore_points_with_env(&env).unwrap().is_empty());
    }
    let (_td, env) = fixture();
    let path = env.managed_file("auto.toml");
    write(&path, b"dark_theme='nord'\n");
    let plan = PreparedPairing::capture_edits(&env, SlotEdit::Clear, SlotEdit::Keep).unwrap();
    plan.save().unwrap().unwrap();
    assert!(path.is_file());
    assert!(fs::read(&path).unwrap().is_empty());
    assert!(
        !PreparedPairing::capture_edits(&env, SlotEdit::Clear, SlotEdit::Keep)
            .unwrap()
            .changed()
    );
}

#[test]
fn pairing_clear_can_mix_with_setting_other_slot_and_detects_late_edits() {
    for late in [false, true] {
        let (_td, env) = fixture();
        let path = env.managed_file("auto.toml");
        let original = b"dark_theme='nord'\nlight_theme='PRIVATE_UNKNOWN'\n";
        write(&path, original);
        let plan = PreparedPairing::capture_edits(
            &env,
            SlotEdit::Set("catppuccin-mocha"),
            SlotEdit::Clear,
        )
        .unwrap();
        assert_eq!(plan.after.dark_theme.as_deref(), Some("catppuccin-mocha"));
        assert!(plan.after.light_theme.is_none());
        if late {
            let edit = b"dark_theme='nord'\n# external change\n";
            let error = plan
                .save_with(|_| {
                    fs::write(&path, edit).unwrap();
                })
                .unwrap_err()
                .to_string();
            assert_eq!(fs::read(&path).unwrap(), edit);
            assert!(error.contains(&list_restore_points_with_env(&env).unwrap()[0].id));
            assert!(!error.contains("PRIVATE_UNKNOWN"));
        } else {
            let id = plan.save().unwrap().unwrap();
            assert!(execute_restore_with_env(&env, &id)
                .unwrap()
                .results
                .iter()
                .all(|entry| entry.success));
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }
}
