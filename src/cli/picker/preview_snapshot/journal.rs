//! Private, atomically published preview recovery records and kernel-held locks.

use super::*;
use crate::config::write_guard::{
    busy_error as active_error, check_private_file, open_lock, record_path, try_lock,
};
use crate::config::RestoreAction;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

pub(super) const MAX_RECORD_BYTES: u64 = 32 * 1024 * 1024;

mod bounded_json;
mod cleanup;
mod prepared;
mod record_source;
pub(crate) use prepared::prepare_recovery;
use record_source::RecordSource;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    version: u32,
    session_id: String,
    pid: u32,
    home: PathBuf,
    config_dir: PathBuf,
    cache_dir: PathBuf,
    writing: bool,
    files: Vec<CapturedFile>,
    expected: Vec<FileState>,
    missing_dirs: Vec<PathBuf>,
}

impl Record {
    pub(super) fn new(snapshot: &PreviewSnapshot, expected: Vec<FileState>, writing: bool) -> Self {
        Self {
            version: 1,
            session_id: snapshot.journal.session_id.clone(),
            pid: std::process::id(),
            home: snapshot.env.home().to_owned(),
            config_dir: snapshot.env.config_dir().to_owned(),
            cache_dir: snapshot.env.slate_cache_dir().to_owned(),
            writing,
            files: snapshot.files.clone(),
            expected,
            missing_dirs: snapshot.missing_dirs.clone(),
        }
    }

    fn validate(&self, env: &SlateEnv) -> Result<()> {
        if self.version != 1
            || self.home != env.home()
            || self.config_dir != env.config_dir()
            || self.cache_dir != env.slate_cache_dir()
            || self.files.len() != self.expected.len()
        {
            return Err(invalid(
                "Recovery record version or environment does not match",
            ));
        }
        let allowed: BTreeSet<_> = preview_paths(env)?.into_iter().collect();
        let mut seen = BTreeSet::new();
        for file in &self.files {
            if !allowed.contains(&file.path)
                || !seen.insert(&file.path)
                || !file.destination.is_absolute()
            {
                return Err(invalid(
                    "Recovery record contains an unexpected or duplicate file",
                ));
            }
        }
        let mut allowed_dirs = BTreeSet::new();
        for path in allowed.iter().filter_map(|path| path.parent()).chain([
            env.config_dir(),
            env.slate_cache_dir().join("backups").as_path(),
        ]) {
            for ancestor in path.ancestors() {
                // Only parent directories of the known preview targets are eligible,
                // and neither HOME nor the filesystem root may be removed.
                if ancestor == env.home() || ancestor.parent().is_none() {
                    break;
                }
                if ancestor.starts_with(env.home())
                    || ancestor.starts_with(env.xdg_config_home())
                    || ancestor.starts_with(env.cache_dir())
                {
                    allowed_dirs.insert(ancestor.to_owned());
                }
            }
        }
        if self
            .missing_dirs
            .iter()
            .any(|path| !allowed_dirs.contains(path))
        {
            return Err(invalid(
                "Recovery record contains an unexpected cleanup directory",
            ));
        }
        Ok(())
    }
}

pub(super) struct JournalGuard {
    env: SlateEnv,
    lock: Mutex<Option<File>>,
    session_id: String,
    finished: AtomicBool,
    expected_record: Option<RecordSource>,
}

impl JournalGuard {
    pub(super) fn begin(env: &SlateEnv) -> Result<Self> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(env.slate_cache_dir())?;
        let lock = open_lock(env, true)?.expect("create opens a file");
        if !try_lock(&lock)? {
            return Err(active_error());
        }
        match fs::symlink_metadata(record_path(env)) {
            Ok(_) => return Err(invalid("An unfinished preview remains. Run `slate recover --dry-run` before opening another picker.")),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {},
            Err(err) => return Err(err.into()),
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| invalid(&err.to_string()))?
            .as_nanos();
        Ok(Self {
            env: env.clone(),
            lock: Mutex::new(Some(lock)),
            session_id: format!("{}-{nanos}", std::process::id()),
            finished: AtomicBool::new(false),
            expected_record: None,
        })
    }

    pub(super) fn save(&self, record: &Record) -> Result<()> {
        self.save_bounded(record, MAX_RECORD_BYTES)
    }

    fn save_bounded(&self, record: &Record, limit: u64) -> Result<()> {
        let bytes = bounded_json::encode(record, limit)?;
        let mut temp = tempfile::NamedTempFile::new_in(self.env.slate_cache_dir())?;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(record_path(&self.env))
            .map_err(|err| err.error)?;
        File::open(self.env.slate_cache_dir())?.sync_all()?;
        Ok(())
    }

    pub(super) fn finish(&self) -> Result<()> {
        cleanup::after_restore(self.finish_inner(false).map(|_| ()))
    }

    pub(super) fn finish_for_commit(&self) -> Result<crate::config::ConfigWriteGuard> {
        cleanup::after_restore(
            self.finish_inner(true).and_then(|guard| {
                guard.ok_or_else(|| invalid("Preview lock was already released"))
            }),
        )
    }

    fn finish_inner(&self, transfer: bool) -> Result<Option<crate::config::ConfigWriteGuard>> {
        let mut lock = self
            .lock
            .lock()
            .map_err(|_| invalid("Preview lock ownership was poisoned"))?;
        if self.finished.load(Ordering::SeqCst) {
            return Ok(None);
        }
        if let Some(source) = &self.expected_record {
            let verified = lock
                .as_ref()
                .ok_or_else(|| invalid("Preview lock is missing"))
                .and_then(|file| record_source::verify_lock(&self.env, file))
                .and_then(|()| source.verify(&self.env));
            verified.map_err(|_| invalid("Recovery record or lock changed or could not be verified; the current record was not cleared"))?;
            cleanup::remove_record(&self.env)?;
        } else if let Some(record) = read_record(&self.env)? {
            if record.session_id != self.session_id {
                return Err(invalid(
                    "Recovery record was replaced; leaving it untouched",
                ));
            }
            cleanup::remove_record(&self.env)?;
        }
        // Keep the inode. Closing the descriptor releases the kernel lock;
        // committing instead transfers ownership without an unlock window.
        let guard = if transfer {
            Some(crate::config::ConfigWriteGuard::adopt_locked(
                lock.take()
                    .ok_or_else(|| invalid("Preview lock is missing"))?,
            )?)
        } else {
            drop(lock.take());
            None
        };
        self.finished.store(true, Ordering::SeqCst);
        Ok(guard)
    }
}

fn invalid(message: &str) -> SlateError {
    SlateError::InvalidConfig(message.to_owned())
}
fn read_record(env: &SlateEnv) -> Result<Option<Record>> {
    RecordSource::capture(env)?
        .map(|source| source.parse(env))
        .transpose()
}

#[derive(serde::Serialize)]
pub(crate) struct RecoveryChange {
    pub path: PathBuf,
    pub action: RestoreAction,
    pub reason: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct RecoveryPlan {
    pub available: bool,
    pub active: bool,
    pub interrupted_write: bool,
    pub record_path: PathBuf,
    pub pid: Option<u32>,
    pub changes: Vec<RecoveryChange>,
}

impl RecoveryPlan {
    pub fn blocked_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| change.action == RestoreAction::Blocked)
            .count()
    }
}

fn plan(env: &SlateEnv, record: Option<&Record>, active: bool) -> RecoveryPlan {
    let changes = record.map(|record| record.files.iter().zip(&record.expected).map(|(file, expected)| {
        let classified = (|| -> Result<RestoreAction> {
            validate_destination(file)?;
            let current = read_state(&file.destination)?;
            if current == file.original { return Ok(RestoreAction::Unchanged); }
            if current != *expected {
                return Err(invalid(if record.writing {
                    "Interrupted write was not fully recorded, or the file was edited later; original bytes are retained."
                } else { "File changed after preview; preserving the external edit." }));
            }
            Ok(match (&file.original, current) {
                (FileState::Absent, _) => RestoreAction::Remove,
                (_, FileState::Absent) => RestoreAction::Create,
                _ => RestoreAction::Replace,
            })
        })();
        let (action, reason) = match classified {
            Ok(action) => (action, None),
            Err(err) => (RestoreAction::Blocked, Some(err.to_string())),
        };
        RecoveryChange { path: file.path.clone(), action, reason }
    }).collect()).unwrap_or_default();
    RecoveryPlan {
        available: record.is_some(),
        active,
        interrupted_write: record.is_some_and(|r| r.writing),
        record_path: record_path(env),
        pid: record.map(|r| r.pid),
        changes,
    }
}

pub(crate) fn inspect_recovery(env: &SlateEnv) -> Result<RecoveryPlan> {
    let lock = open_lock(env, false)?;
    let active = match lock.as_ref() {
        Some(file) => !try_lock(file)?,
        None => false,
    };
    let record = read_record(env)?;
    if record.is_some() && lock.is_none() {
        return Err(invalid("Recovery record has no lock file"));
    }
    Ok(plan(env, record.as_ref(), active))
}

#[cfg(test)]
fn recover(env: &SlateEnv) -> Result<()> {
    prepare_recovery(env)?
        .ok_or_else(|| invalid("No unfinished preview exists"))?
        .recover()
}

#[cfg(test)]
pub(crate) fn discard_recovery(env: &SlateEnv) -> Result<()> {
    prepare_recovery(env)?
        .ok_or_else(|| invalid("No unfinished preview exists"))?
        .discard()
}

#[cfg(test)]
fn export_recovery(env: &SlateEnv, directory: &Path) -> Result<()> {
    prepare_recovery(env)?
        .ok_or_else(|| invalid("No unfinished preview exists"))?
        .export(directory)
}

fn write_private_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, SlateEnv, PathBuf, PreviewSnapshot) {
        let td = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let path = env.xdg_config_home().join("ghostty/config.ghostty");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"# original before interruption\n").unwrap();
        let snapshot = PreviewSnapshot::capture(&env).unwrap();
        snapshot.apply("nord", OpacityPreset::Solid).unwrap();
        (td, env, path, snapshot)
    }

    #[test]
    fn interrupted_write_requires_review_and_allows_export_of_originals() {
        let (td, env, path, snapshot) = fixture();
        // Model a crash before the after-write state could be published. Unlike
        // stable SIGKILL recovery, these new bytes cannot be attributed safely.
        let expected = snapshot
            .files
            .iter()
            .map(|file| file.original.clone())
            .collect();
        snapshot
            .journal
            .save(&Record::new(&snapshot, expected, true))
            .unwrap();
        drop(snapshot); // Release the kernel lock without normal picker cleanup.
        let plan = inspect_recovery(&env).unwrap();
        assert!(plan.interrupted_write && plan.blocked_count() > 0);
        let current = fs::read(&path).unwrap();
        assert!(recover(&env).is_err());
        assert_eq!(fs::read(&path).unwrap(), current);
        let directory = td.path().join("export");
        export_recovery(&env, &directory).unwrap();
        let exported: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("manifest.json")).unwrap()).unwrap();
        let entry = exported
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["path"] == path.to_str().unwrap())
            .unwrap();
        assert_eq!(
            fs::read(directory.join(entry["original_file"].as_str().unwrap())).unwrap(),
            b"# original before interruption\n"
        );
        assert!(record_path(&env).exists());
        discard_recovery(&env).unwrap();
        assert_eq!(fs::read(path).unwrap(), current);
    }

    #[test]
    fn malformed_recovery_targets_cannot_modify_unrelated_files() {
        let (_td, env, path, snapshot) = fixture();
        drop(snapshot);
        let outside = env.home().join("unrelated-private-file");
        fs::write(&outside, b"do not replace\n").unwrap();
        let original = fs::read(record_path(&env)).unwrap();
        let previewed = fs::read(&path).unwrap();
        for invalid_directory in [false, true] {
            let mut record: Record = serde_json::from_slice(&original).unwrap();
            if invalid_directory {
                record.missing_dirs.push(env.home().to_owned());
            } else {
                record.files[0].path = outside.clone();
                record.files[0].destination = fs::canonicalize(&outside).unwrap();
            }
            fs::write(record_path(&env), serde_json::to_vec(&record).unwrap()).unwrap();
            assert!(inspect_recovery(&env).is_err());
            assert!(recover(&env).is_err());
            assert_eq!(fs::read(&outside).unwrap(), b"do not replace\n");
            assert_eq!(fs::read(&path).unwrap(), previewed);
        }
        fs::write(record_path(&env), original).unwrap();
        recover(&env).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"# original before interruption\n");
    }
}
