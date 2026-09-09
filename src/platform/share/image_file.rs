//! Screenshot file handoff, not an image decoder. Keep immutable captured bytes
//! separate from external tools and publish complete files without replacement.
use crate::{
    config::file_read::{self, Links},
    error::{Result, SlateError},
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[cfg(test)]
#[path = "../../../tests/fixtures/share_image.rs"]
mod fixture;
#[cfg(test)]
pub(crate) use fixture::PNG as FIXTURE_PNG;

#[derive(Debug)]
pub(crate) struct CapturedImage {
    bytes: Vec<u8>,
}

fn failure(reason: impl std::fmt::Display) -> SlateError {
    SlateError::PlatformError(format!("Screenshot file {reason}; image contents omitted"))
}

impl CapturedImage {
    pub(crate) fn read(path: &Path) -> Result<Option<Self>> {
        let Some(source) =
            file_read::read(path, MAX_IMAGE_BYTES, Links::Reject).map_err(failure)?
        else {
            return Ok(None);
        };
        // Identify PNGs before handing them to optional tools. This is only a
        // signature check, not full decoding/CRC or decompression-bomb validation.
        if !source.bytes.starts_with(PNG_SIGNATURE) || source.bytes.len() == PNG_SIGNATURE.len() {
            return Err(failure("is empty or does not have a PNG signature"));
        }
        Ok(Some(Self {
            bytes: source.bytes,
        }))
    }

    pub(crate) fn save_new(&self, output: &Path) -> Result<()> {
        self.publish(output, false).map(|_| ())
    }

    pub(crate) fn save_unique(&self, output: &Path) -> Result<PathBuf> {
        self.publish(output, true)
    }

    fn publish(&self, output: &Path, unique: bool) -> Result<PathBuf> {
        let output = std::path::absolute(output)?;
        let parent = output
            .parent()
            .ok_or_else(|| failure("has no output directory"))?;
        fs::create_dir_all(parent)?;
        let parent = fs::canonicalize(parent)?;
        let name = output
            .file_name()
            .ok_or_else(|| failure("has no output name"))?;
        // The temporary pathname remains inside a private directory, including
        // on platforms where no-clobber persistence uses hard-link + unlink.
        let scratch = tempfile::Builder::new()
            .prefix(".slate-share-")
            .tempdir_in(&parent)?;
        let mut file = tempfile::NamedTempFile::new_in(scratch.path())?;
        file.write_all(&self.bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.as_file().sync_all()?;
        for index in 1..=10_000 {
            let target = if index == 1 {
                parent.join(name)
            } else {
                let mut name = output.file_stem().unwrap_or(name).to_os_string();
                name.push(format!("-{index}"));
                if let Some(extension) = output.extension() {
                    name.push(".");
                    name.push(extension);
                }
                parent.join(name)
            };
            match file.persist_noclobber(&target) {
                Ok(_) => {
                    // Publication already succeeded; directory durability is
                    // best effort and must not turn it into an apparent failure.
                    if fs::File::open(&parent)
                        .and_then(|dir| dir.sync_all())
                        .is_err()
                    {
                        eprintln!(
                            "warning: screenshot was saved, but its directory could not be synced"
                        );
                    }
                    return Ok(target);
                }
                Err(error) if unique && error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    file = error.file;
                }
                Err(error) => {
                    return Err(failure(format!(
                        "could not be saved without replacing an existing path ({:?})",
                        error.error.kind()
                    )))
                }
            }
        }
        Err(failure(
            "could not find an unused output name after 10000 attempts",
        ))
    }
}

#[cfg(test)]
#[path = "image_file_tests.rs"]
mod tests;
