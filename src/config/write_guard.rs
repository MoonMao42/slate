//! Cooperative writer exclusion using the same inode as preview recovery.

use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

type FileKey = (u64, u64);
struct Lease {
    _file: File,
}

thread_local! {
    // Nested CLI/coordinator calls on this thread share ownership. Other
    // threads must obtain their own kernel lock, so unrelated writes cannot
    // accidentally become reentrant. Rc also keeps guards !Send and !Sync.
    static HELD: RefCell<HashMap<FileKey, Weak<Lease>>> = RefCell::new(HashMap::new());
}

#[must_use = "Keep the guard alive for the whole configuration operation"]
pub struct ConfigWriteGuard {
    _lease: Rc<Lease>,
}

impl ConfigWriteGuard {
    /// Fail promptly on contention or unresolved recovery, before any config
    /// mutation. The empty private lock file intentionally remains afterward.
    pub fn acquire(env: &SlateEnv) -> Result<Self> {
        let file = match open_lock(env, false)? {
            Some(file) => file,
            None => {
                ensure_no_pending_preview(env)?;
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(env.slate_cache_dir())?;
                open_lock(env, true)?.expect("create opens a lock file")
            }
        };
        let key = file_key(&file)?;
        if let Some(lease) = HELD.with(|held| held.borrow().get(&key).and_then(Weak::upgrade)) {
            return Ok(Self { _lease: lease });
        }
        if !try_lock(&file)? {
            return Err(busy_error());
        }
        // Recheck under the lock: the initial existence check was not atomic
        // with another process publishing its preview record.
        ensure_no_pending_preview(env)?;
        Self::adopt_locked(file)
    }

    /// Transfer a completed preview's locked descriptor into the commit path.
    /// The journal must be cleared first, with no intervening unlock/relock.
    pub(crate) fn adopt_locked(file: File) -> Result<Self> {
        check_private_file(&file)?;
        let key = file_key(&file)?;
        let lease = Rc::new(Lease { _file: file });
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            held.retain(|_, lease| lease.strong_count() > 0);
            held.insert(key, Rc::downgrade(&lease));
        });
        Ok(Self { _lease: lease })
    }
}

fn file_key(file: &File) -> Result<FileKey> {
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

pub(crate) fn record_path(env: &SlateEnv) -> PathBuf {
    env.slate_cache_dir().join("preview-session.json")
}

pub(crate) fn lock_path(env: &SlateEnv) -> PathBuf {
    env.slate_cache_dir().join("preview-session.lock")
}

fn ensure_no_pending_preview(env: &SlateEnv) -> Result<()> {
    match fs::symlink_metadata(record_path(env)) {
        Ok(_) => Err(SlateError::PreviewRecoveryPending),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

pub(crate) fn busy_error() -> SlateError {
    SlateError::ConfigurationBusy
}

pub(crate) fn open_lock(env: &SlateEnv, create: bool) -> Result<Option<File>> {
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(lock_path(env))
    {
        Ok(file) => file,
        Err(err) if !create && err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    check_private_file(&file)?;
    Ok(Some(file))
}

pub(crate) fn check_private_file(file: &File) -> Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(SlateError::InvalidConfig(
            "Preview recovery files must be regular, private files owned by the current user"
                .into(),
        ));
    }
    Ok(())
}

pub(crate) fn try_lock(file: &File) -> Result<bool> {
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let err = std::io::Error::last_os_error();
    if err.kind() == std::io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(err.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_guard_nested_ownership_excludes_other_threads_until_last_drop() {
        let td = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let outer = ConfigWriteGuard::acquire(&env).unwrap();
        let inner = ConfigWriteGuard::acquire(&env).unwrap();
        drop(outer);
        let other_env = env.clone();
        assert!(
            std::thread::spawn(move || ConfigWriteGuard::acquire(&other_env).is_err())
                .join()
                .unwrap()
        );
        let probe = open_lock(&env, false).unwrap().unwrap();
        assert!(!try_lock(&probe).unwrap());
        drop(inner);
        assert!(try_lock(&probe).unwrap());
        assert_eq!(
            fs::metadata(lock_path(&env)).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!env.config_dir().exists());
    }

    #[test]
    fn write_guard_pending_or_unsafe_files_fail_without_creating_config() {
        let td = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        fs::create_dir_all(env.slate_cache_dir()).unwrap();
        fs::write(record_path(&env), b"unreadable recovery fixture").unwrap();
        assert!(ConfigWriteGuard::acquire(&env).is_err());
        assert!(!lock_path(&env).exists());
        fs::remove_file(record_path(&env)).unwrap();
        let target = td.path().join("unrelated");
        fs::write(&target, b"do not touch").unwrap();
        std::os::unix::fs::symlink(&target, lock_path(&env)).unwrap();
        assert!(ConfigWriteGuard::acquire(&env).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"do not touch");
        assert!(!env.config_dir().exists());
    }
}
