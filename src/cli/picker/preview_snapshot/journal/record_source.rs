//! Bind a plan to a bounded, private record. This detects ordinary external edits;
//! metadata/path checks are not an atomic lock against non-cooperating editors.
use super::*;
use crate::config::{file_read::directory_alias_target, recovery_paths::validate_file_path};
use std::os::unix::fs::MetadataExt;

#[derive(PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    mode: u32,
    owner: u32,
    length: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Stamp {
    fn of(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            owner: metadata.uid(),
            length: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }
}

// Intentionally no Debug: the captured bytes contain private configuration.
#[derive(PartialEq, Eq)]
pub(super) struct RecordSource {
    stamp: Stamp,
    resolved: PathBuf,
    // Oversized regular/private records remain explicitly discardable. Their
    // binding is metadata-only; never allocate/read an unbounded corrupt record.
    bytes: Option<Vec<u8>>,
}

impl RecordSource {
    pub(super) fn capture(env: &SlateEnv) -> Result<Option<Self>> {
        let path = record_path(env);
        validate_file_path(env, &path, "Cannot inspect preview recovery record")?;
        let resolved = directory_alias_target(&path).ok_or_else(changed)?;
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(&path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        check_private_file(&file)?;
        let stamp = Stamp::of(&file.metadata()?);
        let bytes = if stamp.length > MAX_RECORD_BYTES {
            None
        } else {
            let mut bytes = Vec::new();
            (&file).take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_RECORD_BYTES {
                return Err(changed());
            }
            Some(bytes)
        };
        validate_file_path(env, &path, "Cannot inspect preview recovery record")?;
        if Stamp::of(&file.metadata()?) != stamp
            || Stamp::of(&fs::symlink_metadata(&path)?) != stamp
            || directory_alias_target(&path).as_ref() != Some(&resolved)
        {
            return Err(changed());
        }
        Ok(Some(Self {
            stamp,
            resolved,
            bytes,
        }))
    }

    pub(super) fn parse(&self, env: &SlateEnv) -> Result<Record> {
        let bytes = self
            .bytes
            .as_ref()
            .ok_or_else(|| invalid("Preview recovery record is too large"))?;
        let record: Record = serde_json::from_slice(bytes).map_err(|err| {
            // Serde type errors can quote private bytes. Report only location.
            invalid(&format!("Cannot parse preview recovery record at line {}, column {}; the saved record was not changed", err.line(), err.column()))
        })?;
        record.validate(env)?;
        Ok(record)
    }

    pub(super) fn verify(&self, env: &SlateEnv) -> Result<()> {
        match Self::capture(env) {
            Ok(Some(current)) if current == *self => Ok(()),
            _ => Err(changed()),
        }
    }
}

fn changed() -> SlateError {
    invalid("Recovery record changed since inspection or cannot be verified; leaving the current record untouched. Inspect it again before proceeding.")
}

pub(super) fn verify_lock(env: &SlateEnv, file: &File) -> Result<()> {
    let result = (|| -> Result<()> {
        let path = crate::config::write_guard::lock_path(env);
        validate_file_path(env, &path, "Cannot verify preview recovery lock")?;
        check_private_file(file)?;
        let held = file.metadata()?;
        let current = fs::symlink_metadata(path)?;
        if held.dev() != current.dev() || held.ino() != current.ino() {
            return Err(changed());
        }
        Ok(())
    })();
    result.map_err(|_| invalid("Recovery lock changed since inspection or cannot be verified; inspect recovery again before proceeding."))
}
