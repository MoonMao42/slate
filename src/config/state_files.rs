use crate::error::{Result, SlateError};
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::Path;

pub(super) fn read_optional_state_file(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    Ok(Some(trimmed.to_string()))
}

pub(super) fn write_state_file(path: &Path, content: &str) -> Result<()> {
    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(content.as_bytes())?;
    file.commit()?;
    Ok(())
}

pub(super) fn write_managed_file(dir: &Path, filename: &str, content: &str) -> Result<()> {
    fs::create_dir_all(dir)?;

    let canonical_dir = fs::canonicalize(dir)?;
    let path = canonical_dir.join(filename);

    if path.exists() && fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(SlateError::InvalidConfig(format!(
            "Refusing to write managed config through symlink: {}",
            path.display()
        )));
    }

    let mut file = AtomicWriteFile::open(&path)?;
    file.write_all(content.as_bytes())?;
    file.commit()?;
    Ok(())
}
