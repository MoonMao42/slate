//! Optimistic path/file identity checks; not exclusion of external writers.
use super::{directory_identity, failure, MAX_BINARY_BYTES};
use crate::{
    config::{
        file_read::{self, FileIdentity, Links, Source},
        state_files::atomic_write_synced_mode,
    },
    env::SlateEnv,
    error::{Result, SlateError},
};
use std::{fs, io::ErrorKind, path::PathBuf};

pub(super) struct Target {
    home: PathBuf,
    path: PathBuf,
    directories: Vec<(PathBuf, Option<FileIdentity>)>,
    original: Option<Source>,
}

fn optional_directory(path: &std::path::Path) -> Result<Option<FileIdentity>> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(Some(FileIdentity::from_metadata(&meta))),
        Ok(_) => Err(failure("target directory is linked or not a directory")),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

impl Target {
    pub(super) fn capture(env: &SlateEnv) -> Result<Self> {
        let home = fs::canonicalize(env.home())?;
        let directories = [home.clone(), home.join(".local"), home.join(".local/bin")]
            .into_iter()
            .map(|path| optional_directory(&path).map(|identity| (path, identity)))
            .collect::<Result<Vec<_>>>()?;
        let path = home.join(".local/bin/starship");
        let original = file_read::read(&path, MAX_BINARY_BYTES, Links::Reject).map_err(failure)?;
        Ok(Self {
            home,
            path,
            directories,
            original,
        })
    }

    fn verify(&self, env: &SlateEnv) -> Result<()> {
        if fs::canonicalize(env.home())? != self.home {
            return Err(failure("HOME moved after preparation"));
        }
        for (path, identity) in &self.directories {
            if &optional_directory(path)? != identity {
                return Err(failure("target directory changed after preparation; retry"));
            }
        }
        if file_read::read(&self.path, MAX_BINARY_BYTES, Links::Reject).map_err(failure)?
            != self.original
        {
            return Err(failure(
                "target file changed after preparation; retry without overwriting external changes",
            ));
        }
        Ok(())
    }

    pub(super) fn publish(&mut self, env: &SlateEnv, bytes: &[u8]) -> Result<()> {
        self.verify(env)?;
        for index in 0..self.directories.len() {
            if self.directories[index].1.is_none() {
                // Create one component at a time. An unexpected concurrent
                // creation is an error, not a directory to silently adopt.
                self.verify(env)?;
                let path = &self.directories[index].0;
                fs::create_dir(path)?;
                self.directories[index].1 = Some(directory_identity(path)?);
            }
        }
        self.verify(env)?;
        // Replace rather than truncate, and use controlled executable bits
        // instead of inheriting special or writable-for-everyone archive modes.
        atomic_write_synced_mode(&self.path, bytes, Some(0o755)).map_err(|_| {
            // Publication failure is conservative: do not claim the rename
            // certainly did not happen, or continue setup as if it were known.
            SlateError::StarshipInstallUncertain("could not confirm binary publication".into())
        })
    }
}
