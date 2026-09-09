//! Best-effort conflict checking for an already prepared user-owned include file.
//! Not a multi-file transaction or a lock against external editors.
use crate::{
    config::{
        file_read::{self, Links, Source, MAX_TOOL_CONFIG_BYTES},
        recovery_paths, state_files,
    },
    env::SlateEnv,
    error::{Result, SlateError},
};
use std::path::{Path, PathBuf};

pub(super) fn destination(path: &Path, label: &str) -> Result<PathBuf> {
    file_read::directory_alias_target(path).ok_or_else(|| {
        SlateError::InvalidConfig(format!(
            "Cannot resolve {label} destination; no files were changed."
        ))
    })
}

pub(super) fn publish(
    env: &SlateEnv,
    path: &Path,
    expected_destination: &Path,
    original: Option<&Source>,
    updated: &[u8],
    label: &str,
) -> Result<()> {
    let changed = || {
        SlateError::InvalidConfig(format!(
        "{label} changed or became unsafe during sync; it was not overwritten. The managed palette may already be updated; review the recovery point before retrying."
    ))
    };
    if updated.len() as u64 > MAX_TOOL_CONFIG_BYTES {
        return Err(changed());
    }
    recovery_paths::validate_file_path(env, path, label).map_err(|_| changed())?;
    if file_read::directory_alias_target(path).as_deref() != Some(expected_destination) {
        return Err(changed());
    }
    let current =
        file_read::read(path, MAX_TOOL_CONFIG_BYTES, Links::Reject).map_err(|_| changed())?;
    if current.as_ref() != original {
        return Err(changed());
    }
    if original.is_some_and(|source| source.bytes == updated) {
        return Ok(());
    }
    state_files::atomic_write_synced_mode(path, updated, original.and_then(|source| source.mode))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_publication_preserves_identity_but_still_rejects_external_changes() {
        use std::{
            fs,
            os::unix::fs::{MetadataExt, PermissionsExt},
        };
        for change in ["none", "permissions", "replacement"] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            let path = home.path().join("config");
            let bytes = b"# private settings\n";
            fs::write(&path, bytes).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            let expected = destination(&path, "fixture").unwrap();
            let original = file_read::read(&path, MAX_TOOL_CONFIG_BYTES, Links::Reject).unwrap();
            match change {
                "permissions" => {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap()
                }
                "replacement" => {
                    let replacement = home.path().join("replacement");
                    fs::write(&replacement, bytes).unwrap();
                    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o640)).unwrap();
                    fs::rename(&replacement, &path).unwrap();
                }
                _ => {}
            }
            let before = fs::metadata(&path).unwrap();
            let result = publish(&env, &path, &expected, original.as_ref(), bytes, "fixture");
            if change == "none" {
                result.unwrap();
            } else {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("was not overwritten"));
            }
            let after = fs::metadata(&path).unwrap();
            assert_eq!(
                (after.ino(), after.mode(), after.modified().unwrap()),
                (before.ino(), before.mode(), before.modified().unwrap())
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read_dir(home.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn integration_publication_pins_alias_destination_for_missing_and_hardlinked_sources() {
        use std::{fs, os::unix::fs::symlink};
        for existing in [false, true] {
            for retarget in [false, true] {
                let home = tempfile::tempdir().unwrap();
                let env = SlateEnv::with_home(home.path().to_owned());
                let first = home.path().join("first");
                let second = home.path().join("second");
                fs::create_dir(&first).unwrap();
                fs::create_dir(&second).unwrap();
                let alias = home.path().join("alias");
                symlink(&first, &alias).unwrap();
                let path = alias.join("config");
                if existing {
                    fs::write(first.join("config"), b"# personal\n").unwrap();
                    fs::hard_link(first.join("config"), second.join("config")).unwrap();
                }
                let expected = destination(&path, "integration").unwrap();
                let original =
                    file_read::read(&path, MAX_TOOL_CONFIG_BYTES, Links::Reject).unwrap();
                if retarget {
                    fs::remove_file(&alias).unwrap();
                    symlink(&second, &alias).unwrap();
                }
                let result = publish(
                    &env,
                    &path,
                    &expected,
                    original.as_ref(),
                    b"# updated\n",
                    "integration",
                );
                if retarget {
                    assert!(result
                        .unwrap_err()
                        .to_string()
                        .contains("was not overwritten"));
                    assert_eq!(
                        fs::read(first.join("config")).ok(),
                        existing.then(|| b"# personal\n".to_vec())
                    );
                } else {
                    result.unwrap();
                    assert_eq!(fs::read(first.join("config")).unwrap(), b"# updated\n");
                }
                assert_eq!(
                    fs::read(second.join("config")).ok(),
                    existing.then(|| b"# personal\n".to_vec())
                );
            }
        }
    }

    #[test]
    fn missing_integration_is_created_but_new_external_file_is_not_overwritten() {
        for appeared in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            let path = home.path().join(".tmux.conf");
            if appeared {
                std::fs::write(&path, b"PRIVATE_EXTERNAL").unwrap();
            }
            let result = publish(
                &env,
                &path,
                &destination(&path, "tmux configuration").unwrap(),
                None,
                b"# prepared include\n",
                "tmux configuration",
            );
            if appeared {
                let error = result.unwrap_err().to_string();
                assert!(error.contains("was not overwritten"));
                assert!(!error.contains("PRIVATE_EXTERNAL"));
                assert_eq!(std::fs::read(&path).unwrap(), b"PRIVATE_EXTERNAL");
            } else {
                result.unwrap();
                assert_eq!(std::fs::read(&path).unwrap(), b"# prepared include\n");
            }
            assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
        }
    }
}
