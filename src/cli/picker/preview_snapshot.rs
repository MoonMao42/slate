//! File-level undo for the live terminal preview, independent of theme generation.

use crate::adapter::{AlacrittyAdapter, GhosttyAdapter, KittyAdapter, ToolAdapter};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::opacity::OpacityPreset;
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};

// A serialized byte needs at least two JSON bytes. More raw state than this
// cannot fit the 32 MiB journal, even before other state and metadata are added.
const MAX_CAPTURE_BYTES: u64 = crate::config::preview_write::MAX_STATE_BYTES;

mod journal;
#[cfg(test)]
mod read_safety_tests;
#[cfg(test)]
mod write_state_tests;
#[cfg(test)]
use journal::discard_recovery;
pub(crate) use journal::{inspect_recovery, prepare_recovery, RecoveryPlan};

use crate::config::preview_write::{Entry as WriteEntry, FileState, Scope as WriteScope};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CapturedFile {
    path: PathBuf,
    destination: PathBuf,
    original: FileState,
}

pub(crate) struct PreviewSnapshot {
    env: SlateEnv,
    files: Vec<CapturedFile>,
    missing_dirs: Vec<PathBuf>,
    expected: Mutex<Vec<FileState>>,
    operation: Mutex<()>,
    writing: AtomicBool,
    restored: AtomicBool,
    journal: journal::JournalGuard,
}

fn preview_paths(env: &SlateEnv) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<_> = [
        "managed/ghostty/theme.conf",
        "managed/ghostty/font.conf",
        "managed/ghostty/opacity.conf",
        "managed/ghostty/blur.conf",
        "managed/alacritty/colors.toml",
        "managed/alacritty/opacity.toml",
        "managed/kitty/theme.conf",
        "managed/kitty/opacity.conf",
        "config.toml",
    ]
    .iter()
    .map(|path| env.config_dir().join(path))
    .collect();
    paths.extend(GhosttyAdapter.integration_candidate_paths_with_env(env)?);
    paths.push(KittyAdapter::resolve_config_path_with_env(env));
    paths.extend(AlacrittyAdapter::integration_candidate_paths_with_env(env));
    Ok(paths)
}

impl PreviewSnapshot {
    /// Capture before ConfigManager or any preview adapter can create files.
    pub(super) fn capture(env: &SlateEnv) -> Result<Self> {
        let paths = if env.session().is_remote() {
            Vec::new()
        } else {
            preview_paths(env)?
        };
        let mut missing_dirs = BTreeSet::new();
        for path in paths.iter().filter_map(|path| path.parent()).chain([
            env.config_dir(),
            env.slate_cache_dir().join("backups").as_path(),
        ]) {
            collect_missing_dirs(path, &mut missing_dirs)?;
        }
        // Lock before reading originals, so another picker cannot preview them
        // between capture and publication of the durable recovery record.
        let journal = journal::JournalGuard::begin(env)?;
        let mut remaining = MAX_CAPTURE_BYTES;
        let files: Vec<_> = paths
            .into_iter()
            .map(|path| {
                let destination = resolve_destination(&path)?;
                let original = read_state_with_budget(&destination, &mut remaining)?;
                Ok(CapturedFile {
                    path,
                    destination,
                    original,
                })
            })
            .collect::<Result<_>>()?;
        let expected = files.iter().map(|file| file.original.clone()).collect();
        let mut missing_dirs: Vec<_> = missing_dirs.into_iter().collect();
        missing_dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        let snapshot = Self {
            env: env.clone(),
            files,
            missing_dirs,
            expected: Mutex::new(expected),
            operation: Mutex::new(()),
            writing: AtomicBool::new(false),
            restored: AtomicBool::new(false),
            journal,
        };
        snapshot.save_journal(false)?;
        Ok(snapshot)
    }

    fn save_journal(&self, writing: bool) -> Result<()> {
        let expected = self.expected_state()?.clone();
        self.journal
            .save(&journal::Record::new(self, expected, writing))
    }

    fn enter_operation(&self) -> Result<MutexGuard<'_, ()>> {
        match self.operation.try_lock() {
            Ok(guard) => Ok(guard),
            // This mutex contains no state; it only excludes overlapping work.
            // After unwind releases it, actual file/expected-state checks still
            // decide what is safe. Never recover a poisoned expected-state mutex.
            Err(TryLockError::Poisoned(error)) => Ok(error.into_inner()),
            Err(TryLockError::WouldBlock) => Err(SlateError::InvalidConfig(
                "Another preview operation is still active. The requested operation did not start and did not clear the recovery record. Close the picker, then inspect `slate recover --dry-run`.".into(),
            )),
        }
    }

    fn expected_state(&self) -> Result<MutexGuard<'_, Vec<FileState>>> {
        let states = self.expected.try_lock().map_err(|_| SlateError::InvalidConfig(
            "Recorded preview write state is unavailable; refusing further writes or cleanup. The recovery record was not cleared. Close the picker, then inspect `slate recover --dry-run`.".into(),
        ))?;
        if states.len() != self.files.len() {
            return Err(SlateError::InvalidConfig(
                "Recorded preview write state is incomplete; refusing further writes or cleanup. The recovery record was not cleared. Close the picker, then inspect `slate recover --dry-run`.".into(),
            ));
        }
        Ok(states)
    }

    pub(super) fn apply(&self, theme_id: &str, opacity: OpacityPreset) -> Result<()> {
        self.apply_with(|| crate::cli::set::silent_preview_apply(&self.env, theme_id, opacity))
    }

    pub(super) fn apply_with(&self, apply: impl FnOnce() -> Result<()>) -> Result<()> {
        let _operation = self.enter_operation()?;
        if self.restored.load(Ordering::SeqCst) {
            return Err(SlateError::Internal("Preview session already ended".into()));
        }
        if self.writing.load(Ordering::SeqCst) {
            return Err(SlateError::InvalidConfig(
                "The previous preview write was not fully recorded; refusing another preview. Close the picker, then inspect `slate recover --dry-run`.".into(),
            ));
        }
        let expected = self.expected_state()?.clone();
        let before = self.read_current()?;
        if before != expected {
            return Err(SlateError::InvalidConfig(
                "A preview file changed outside Slate; stopping live preview to preserve that edit.".into(),
            ));
        }
        drop(before);
        self.save_journal(true)?;
        let writes = WriteScope::begin(
            self.files
                .iter()
                .zip(expected)
                .map(|(file, expected)| WriteEntry {
                    path: file.path.clone(),
                    destination: file.destination.clone(),
                    expected,
                })
                .collect(),
        )?;
        // The operation gate stays held across adapters. A panic hook must not
        // race an unfinished write or deadlock trying to clean it up; it leaves
        // the durable record for review when this gate cannot be acquired.
        self.writing.store(true, Ordering::SeqCst);
        let result = apply();
        let (written, write_failed) = writes.finish()?;
        *self.expected_state()? = written;
        let after = self.read_current();
        let verified = after.and_then(|after| {
            if after == *self.expected_state()? {
                Ok(())
            } else {
                Err(SlateError::InvalidConfig(
                    "Preview files differ from Slate's recorded writes; preserving unrecorded or external changes for recovery review.".into(),
                ))
            }
        });
        self.writing.store(verified.is_err(), Ordering::SeqCst);
        self.save_journal(verified.is_err())?;
        result.and(verified).and_then(|()| {
            if write_failed {
                Err(SlateError::InvalidConfig(
                    "One or more preview writes did not complete; inspect the saved recovery before continuing.".into(),
                ))
            } else {
                Ok(())
            }
        })
    }

    fn read_current(&self) -> Result<Vec<FileState>> {
        let mut remaining = MAX_CAPTURE_BYTES;
        self.files
            .iter()
            .map(|file| {
                validate_destination(file)?;
                read_state_with_budget(&file.destination, &mut remaining)
            })
            .collect()
    }

    /// Restore each unconflicted file; do not remove changed links or new user files.
    pub(super) fn restore(&self) -> Result<()> {
        let _operation = self.enter_operation()?;
        self.restore_files(true)?;
        self.journal.finish()
    }

    pub(super) fn restore_for_commit(&self) -> Result<crate::config::ConfigWriteGuard> {
        let _operation = self.enter_operation()?;
        // Restore original bytes for the recovery checkpoint, not the window.
        // The confirmed selection will perform the next live reload.
        self.restore_files(false)?;
        self.journal.finish_for_commit()
    }

    fn restore_files(&self, reload_live: bool) -> Result<()> {
        self.restore_files_with_reload(reload_live, || {
            if std::env::var("TERM_PROGRAM").is_ok_and(|name| name.eq_ignore_ascii_case("ghostty"))
            {
                if let Err(err) = GhosttyAdapter.reload_with_env(&self.env) {
                    eprintln!("warning: preview files restored; Ghostty reload: {err}");
                }
            }
            if let Err(err) = crate::adapter::kitty::reload_config_after_preview(&self.env) {
                eprintln!("warning: preview files restored; Kitty reload: {err}");
            }
        })
    }

    fn restore_files_with_reload(&self, reload_live: bool, reload: impl FnOnce()) -> Result<()> {
        if self.restored.load(Ordering::SeqCst) {
            return Ok(());
        }
        let expected = self.expected_state()?.clone();
        let interrupted = self.writing.load(Ordering::SeqCst);
        let mut errors = Vec::new();
        let mut changed = false;
        for (index, file) in self.files.iter().enumerate() {
            let result = (|| -> Result<()> {
                validate_destination(file)?;
                let current = read_state(&file.destination)?;
                if current == file.original {
                    return Ok(());
                }
                if expected[index] != current {
                    let reason = if interrupted {
                        "Preserved an unrecorded or external change after an interrupted preview write to"
                    } else {
                        "Preserved an external edit to"
                    };
                    return Err(SlateError::InvalidConfig(format!(
                        "{reason} {}",
                        file.path.display(),
                    )));
                }
                match &file.original {
                    FileState::Absent => fs::remove_file(&file.destination)?,
                    FileState::Present { bytes, mode } => {
                        // Set permissions on the temporary file before publication:
                        // saved private bytes must not inherit a public preview mode.
                        crate::config::state_files::atomic_write_synced_mode(
                            &file.destination,
                            bytes,
                            Some(*mode),
                        )?;
                    }
                }
                changed = true;
                Ok(())
            })();
            if let Err(err) = result {
                errors.push(crate::cli::file_output::terminal_text(&format!(
                    "{}: {err}",
                    file.path.display()
                )));
            }
        }
        // Non-recursive cleanup only: independently created files keep their directory.
        for dir in &self.missing_dirs {
            if fs::symlink_metadata(dir).is_ok_and(|metadata| metadata.is_dir()) {
                if let Err(err) = fs::remove_dir(dir) {
                    if !matches!(
                        err.kind(),
                        std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
                    ) {
                        errors.push(crate::cli::file_output::terminal_text(&format!(
                            "{}: {err}",
                            dir.display()
                        )));
                    }
                }
            }
        }
        if reload_live && changed && errors.is_empty() && self.env.session().can_reload_terminal() {
            reload();
        }
        if errors.is_empty() {
            self.restored.store(true, Ordering::SeqCst);
            Ok(())
        } else {
            Err(SlateError::InvalidConfig(format!(
                "Preview cleanup needs attention. Some files may already have been restored; completed changes were not rolled back. Recovery finalization was not attempted. Run `slate recover --dry-run` before proceeding. Details: {}",
                errors.join("; "),
            )))
        }
    }
}

fn validate_destination(file: &CapturedFile) -> Result<()> {
    if resolve_destination(&file.path)? != file.destination {
        return Err(SlateError::InvalidConfig(format!(
            "Preview path was redirected; leaving it untouched: {}",
            file.path.display(),
        )));
    }
    Ok(())
}

fn read_state(path: &Path) -> Result<FileState> {
    let mut remaining = MAX_CAPTURE_BYTES;
    read_state_with_budget(path, &mut remaining)
}

fn read_state_with_budget(path: &Path, remaining: &mut u64) -> Result<FileState> {
    use crate::config::file_read::{self, Links, ReadError, MAX_TOOL_CONFIG_BYTES};
    let limit = MAX_TOOL_CONFIG_BYTES.min(*remaining);
    let source = file_read::read_with_metadata(path, limit, Links::Reject).map_err(|error| {
        let reason = match error {
            ReadError::TooLarge(_) => {
                "preview input limit exceeded (8 MiB per file, 16 MiB per captured state)"
            }
            ReadError::Unsafe(reason) => reason,
        };
        SlateError::InvalidConfig(crate::cli::file_output::terminal_text(&format!(
            "Cannot read preview file {}: {reason}",
            path.display()
        )))
    })?;
    match source {
        Some((source, metadata)) => {
            *remaining -= source.bytes.len() as u64;
            Ok(FileState::Present {
                bytes: source.bytes,
                mode: metadata.permissions().mode(),
            })
        }
        None => Ok(FileState::Absent),
    }
}

fn resolve_destination(path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(fs::canonicalize(path)?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| {
                SlateError::InvalidConfig(format!(
                    "Cannot resolve preview path: {}",
                    path.display()
                ))
            })?;
            Ok(
                resolve_destination(parent)?.join(path.file_name().ok_or_else(|| {
                    SlateError::InvalidConfig(format!("Invalid preview path: {}", path.display()))
                })?),
            )
        }
        Err(err) => Err(err.into()),
    }
}

fn collect_missing_dirs(path: &Path, missing: &mut BTreeSet<PathBuf>) -> Result<()> {
    let mut current = path;
    loop {
        match fs::symlink_metadata(current) {
            Ok(_) => break,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                missing.insert(current.to_owned());
                let Some(parent) = current.parent() else {
                    break;
                };
                current = parent;
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn seed(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn preview_snapshot_commit_cleanup_is_file_only_but_cancel_reloads() {
        for reload_live in [false, true] {
            let td = TempDir::new().unwrap();
            let env = SlateEnv::from_vars(|key| match key {
                "HOME" => Some(td.path().as_os_str().to_owned()),
                _ => None,
            })
            .unwrap();
            assert!(env.session().can_reload_terminal());
            let path = env.managed_file("managed/ghostty/theme.conf");
            seed(&path, b"# original colors\n");
            let snapshot = PreviewSnapshot::capture(&env).unwrap();
            // Model a recorded preview write without launching native reloads.
            seed(&path, b"# selected colors\n");
            let index = snapshot
                .files
                .iter()
                .position(|file| file.path == path)
                .unwrap();
            snapshot.expected.lock().unwrap()[index] = read_state(&path).unwrap();
            let reloads = std::cell::Cell::new(0);
            snapshot
                .restore_files_with_reload(reload_live, || reloads.set(reloads.get() + 1))
                .unwrap();
            assert_eq!(reloads.get(), usize::from(reload_live));
            assert_eq!(fs::read(&path).unwrap(), b"# original colors\n");
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            snapshot
                .restore_files_with_reload(true, || panic!("cleanup must be idempotent"))
                .unwrap();
            snapshot.restore().unwrap();
        }
    }

    #[test]
    fn preview_snapshot_covers_alacritty_alternates_and_external_precedence_changes() {
        for external_edit in [false, true] {
            let td = TempDir::new().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let alternate = env.home().join(".alacritty.toml");
            let higher = env.xdg_config_home().join("alacritty/alacritty.toml");
            let original = b"# original alternate\n[general]\nimport = ['user.toml']\n";
            seed(&alternate, original);
            let snapshot = PreviewSnapshot::capture(&env).unwrap();
            assert_eq!(
                snapshot
                    .files
                    .iter()
                    .filter(
                        |file| AlacrittyAdapter::integration_candidate_paths_with_env(&env)
                            .contains(&file.path)
                    )
                    .count(),
                3
            );
            snapshot.apply("nord", OpacityPreset::Solid).unwrap();
            assert_ne!(fs::read(&alternate).unwrap(), original);
            assert!(!higher.exists());
            if external_edit {
                seed(&higher, b"# independently created config\n");
                assert!(snapshot
                    .apply("catppuccin-mocha", OpacityPreset::Solid)
                    .unwrap_err()
                    .to_string()
                    .contains("changed outside Slate"));
                assert!(snapshot.restore().is_err());
                assert_eq!(
                    fs::read(&higher).unwrap(),
                    b"# independently created config\n"
                );
            } else {
                snapshot.restore().unwrap();
                assert!(!higher.exists());
            }
            assert_eq!(fs::read(&alternate).unwrap(), original);
            assert_eq!(
                fs::metadata(&alternate).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn preview_snapshot_commit_transfers_lock_without_an_unlock_window() {
        use crate::config::write_guard::{open_lock, try_lock};
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let path = env.xdg_config_home().join("ghostty/config.ghostty");
        seed(&path, b"# original settings\n");
        let snapshot = PreviewSnapshot::capture(&env).unwrap();
        snapshot.apply("nord", OpacityPreset::Solid).unwrap();
        let guard = snapshot.restore_for_commit().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"# original settings\n");
        assert!(!env.slate_cache_dir().join("preview-session.json").exists());
        let nested = crate::config::ConfigWriteGuard::acquire(&env).unwrap();
        let probe = open_lock(&env, false).unwrap().unwrap();
        assert!(!try_lock(&probe).unwrap());
        snapshot.restore().unwrap(); // a late Drop/panic cleanup cannot unlock the commit
        drop(nested);
        assert!(!try_lock(&probe).unwrap());
        drop(guard);
        assert!(try_lock(&probe).unwrap());
    }

    #[test]
    fn preview_snapshot_restores_original_bytes_modes_and_links() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let ghostty = env.xdg_config_home().join("ghostty/config.ghostty");
        let linked = td.path().join("dotfiles/ghostty.conf");
        seed(&linked, b"# custom settings\xff\nbackground = #112233\n");
        fs::create_dir_all(ghostty.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&linked, &ghostty).unwrap();
        let alacritty = env.xdg_config_home().join("alacritty/alacritty.toml");
        let kitty = env.xdg_config_home().join("kitty/kitty.conf");
        seed(
            &alacritty,
            b"# user's spacing\n[window]\npadding = { x = 7, y = 9 }\n",
        );
        seed(&kitty, b"# user settings\nbackground #113355\n");
        seed(&env.managed_file("current-font"), b"Fixture Mono");
        let snapshot = PreviewSnapshot::capture(&env).unwrap();
        let original = snapshot.read_current().unwrap();
        snapshot.apply("nord", OpacityPreset::Frosted).unwrap();
        assert!(fs::read_to_string(&kitty)
            .unwrap()
            .contains("allow_remote_control"));
        assert!(fs::read(&linked)
            .unwrap()
            .windows(12)
            .any(|s| s == b"config-file "));
        snapshot
            .apply("catppuccin-latte", OpacityPreset::Clear)
            .unwrap();
        snapshot.restore().unwrap();
        assert!(snapshot.read_current().unwrap() == original);
        assert_eq!(fs::read_link(ghostty).unwrap(), linked);
        assert_eq!(
            fs::metadata(alacritty).unwrap().permissions().mode() & 0o777,
            0o600
        );
        snapshot.restore().unwrap(); // Idempotent across normal exit, Drop, and panic paths.
    }

    #[test]
    fn preview_snapshot_fresh_home_does_not_keep_generated_files_or_directories() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let snapshot = PreviewSnapshot::capture(&env).unwrap();
        snapshot.apply("nord", OpacityPreset::Frosted).unwrap();
        assert!(env
            .config_dir()
            .join("managed/ghostty/opacity.conf")
            .exists());
        snapshot.restore().unwrap();
        assert!(!env.config_dir().exists());
        assert!(!env.slate_cache_dir().join("preview-session.json").exists());
        assert!(env.slate_cache_dir().join("preview-session.lock").exists());
    }

    #[test]
    fn preview_snapshot_preserves_external_edits_and_redirected_links() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let ghostty = env.xdg_config_home().join("ghostty/config.ghostty");
        seed(&ghostty, b"# original\n");
        let snapshot = PreviewSnapshot::capture(&env).unwrap();
        snapshot.apply("nord", OpacityPreset::Solid).unwrap();
        seed(&ghostty, b"# changed in another editor\n");
        let user_file = env.config_dir().join("managed/ghostty/user-created.conf");
        seed(&user_file, b"# keep this independent file\n");
        seed(&env.managed_file("auto.toml"), b"dark_theme = 'nord'\n");
        assert!(snapshot
            .apply("catppuccin-latte", OpacityPreset::Clear)
            .is_err());
        assert!(snapshot.restore().is_err());
        assert_eq!(
            fs::read(&ghostty).unwrap(),
            b"# changed in another editor\n"
        );
        assert!(!env.config_dir().join("managed/ghostty/theme.conf").exists());
        assert_eq!(
            fs::read(&user_file).unwrap(),
            b"# keep this independent file\n"
        );
        assert_eq!(
            fs::read(env.managed_file("auto.toml")).unwrap(),
            b"dark_theme = 'nord'\n"
        );

        assert!(PreviewSnapshot::capture(&env).is_err());
        drop(snapshot);
        discard_recovery(&env).unwrap();
        let snapshot = PreviewSnapshot::capture(&env).unwrap();
        snapshot.apply("nord", OpacityPreset::Solid).unwrap();
        let external = td.path().join("new-target");
        seed(&external, b"# do not overwrite\n");
        fs::remove_file(&ghostty).unwrap();
        std::os::unix::fs::symlink(&external, &ghostty).unwrap();
        assert!(snapshot.restore().is_err());
        assert_eq!(fs::read_link(ghostty).unwrap(), external);
        assert_eq!(fs::read(external).unwrap(), b"# do not overwrite\n");
    }

    #[test]
    fn preview_snapshot_failed_adapter_can_still_restore_partial_preview() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        seed(
            &env.xdg_config_home().join("alacritty/alacritty.toml"),
            b"[invalid TOML\n",
        );
        let snapshot = std::sync::Arc::new(PreviewSnapshot::capture(&env).unwrap());
        let guard = super::super::rollback_guard::RollbackGuard::arm(
            snapshot.clone(),
            std::sync::Arc::new(AtomicBool::new(false)),
        );
        let original = snapshot.read_current().unwrap();
        assert!(snapshot.apply("nord", OpacityPreset::Solid).is_err());
        drop(guard);
        assert!(snapshot.read_current().unwrap() == original);
    }
}
