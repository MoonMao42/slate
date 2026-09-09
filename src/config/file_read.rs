//! Bounded regular-file reads. Preserve linked dotfiles for ordinary reads;
//! checkpoints with a no-link contract opt out explicitly. These checks detect
//! common concurrent replacements, but do not lock out external editors.
use crate::error::{Result, SlateError};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

/// Deduplicate directory aliases without following the final file link. Include
/// missing suffixes so a not-yet-created entry still has one capture target.
/// This is an identity hint, not file validation or an authorization to write.
pub(crate) fn directory_alias_target(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    for ancestor in parent.ancestors() {
        if let Ok(resolved) = fs::canonicalize(ancestor) {
            if !resolved.is_dir() {
                return None;
            }
            return Some(
                resolved
                    .join(parent.strip_prefix(ancestor).ok()?)
                    .join(path.file_name()?),
            );
        }
    }
    None
}

pub(crate) const MAX_STATE_BYTES: u64 = 4 * 1024;
pub(crate) const MAX_DOCUMENT_BYTES: u64 = 256 * 1024;
pub(crate) const MAX_TOOL_CONFIG_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) enum Links {
    Follow,
    Reject,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Source {
    pub bytes: Vec<u8>,
    pub mode: Option<u32>,
    pub identity: FileIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(not(unix))]
    created: Option<std::time::SystemTime>,
}

impl FileIdentity {
    pub(crate) fn from_metadata(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                device: metadata.dev(),
                inode: metadata.ino(),
            }
        }
        #[cfg(not(unix))]
        Self {
            created: metadata.created().ok(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    #[error("file size limit exceeded ({0} bytes)")]
    TooLarge(u64),
    #[error("{0}")]
    Unsafe(&'static str),
}

fn metadata(path: &Path, links: Links) -> std::io::Result<fs::Metadata> {
    match links {
        Links::Follow => fs::metadata(path),
        Links::Reject => fs::symlink_metadata(path),
    }
}

// ENOENT can mean a dangling link, not just an unset preference. Walk only as
// far as the nearest existing ancestor, allowing normal directory aliases.
pub(crate) fn confirm_missing(path: &Path) -> std::result::Result<(), ReadError> {
    for ancestor in path.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        match fs::symlink_metadata(ancestor) {
            Ok(_) if ancestor == path => {
                return Err(ReadError::Unsafe(
                    "path changed or symlink target is missing",
                ));
            }
            Ok(_) => {
                return match fs::metadata(ancestor) {
                    Ok(meta) if meta.is_dir() => Ok(()),
                    _ => Err(ReadError::Unsafe("parent is not an accessible directory")),
                };
            }
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(_) => return Err(ReadError::Unsafe("cannot inspect parent directory")),
        }
    }
    Ok(())
}

pub(crate) fn read(
    path: &Path,
    limit: u64,
    links: Links,
) -> std::result::Result<Option<Source>, ReadError> {
    read_with_metadata(path, limit, links).map(|source| source.map(|(source, _)| source))
}

/// Return metadata from the same verified open descriptor. Callers that need
/// legacy full Unix modes must not combine captured bytes with a later path stat.
pub(crate) fn read_with_metadata(
    path: &Path,
    limit: u64,
    links: Links,
) -> std::result::Result<Option<(Source, fs::Metadata)>, ReadError> {
    let before = match metadata(path, links) {
        Ok(meta) => meta,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            confirm_missing(path)?;
            return Ok(None);
        }
        Err(_) => return Err(ReadError::Unsafe("cannot inspect file")),
    };
    if !before.is_file() {
        return Err(ReadError::Unsafe("expected a regular file"));
    }
    if before.len() > limit {
        return Err(ReadError::TooLarge(limit));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut flags = libc::O_NONBLOCK | libc::O_CLOEXEC;
        if matches!(links, Links::Reject) {
            flags |= libc::O_NOFOLLOW;
        }
        options.custom_flags(flags);
    }
    let file = options
        .open(path)
        .map_err(|_| ReadError::Unsafe("cannot open file safely"))?;
    let opened = file
        .metadata()
        .map_err(|_| ReadError::Unsafe("cannot inspect open file"))?;
    if !same_file(&before, &opened) {
        return Err(ReadError::Unsafe("file changed while reading; retry"));
    }
    let mut bytes = Vec::new();
    (&file)
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ReadError::Unsafe("cannot read file"))?;
    if bytes.len() as u64 > limit {
        return Err(ReadError::TooLarge(limit));
    }
    let after = file
        .metadata()
        .map_err(|_| ReadError::Unsafe("cannot inspect read file"))?;
    let current =
        metadata(path, links).map_err(|_| ReadError::Unsafe("file moved while reading"))?;
    if bytes.len() as u64 != opened.len()
        || !same_file(&opened, &after)
        || !same_file(&opened, &current)
    {
        return Err(ReadError::Unsafe("file changed while reading; retry"));
    }
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(opened.permissions().mode() & 0o777)
    };
    #[cfg(not(unix))]
    let mode = None;
    Ok(Some((
        Source {
            bytes,
            mode,
            identity: FileIdentity::from_metadata(&opened),
        },
        opened,
    )))
}

fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (a.dev(), a.ino(), a.mode(), a.ctime(), a.ctime_nsec())
            != (b.dev(), b.ino(), b.mode(), b.ctime(), b.ctime_nsec())
        {
            return false;
        }
    }
    a.is_file() && b.is_file() && a.len() == b.len() && a.modified().ok() == b.modified().ok()
}

pub(crate) fn read_text(path: &Path, limit: u64) -> Result<Option<String>> {
    read_text_with_links(path, limit, Links::Follow)
}

pub(crate) fn read_text_with_links(
    path: &Path,
    limit: u64,
    links: Links,
) -> Result<Option<String>> {
    let failure = |reason: String| SlateError::ConfigReadError(path.display().to_string(), reason);
    read(path, limit, links)
        .map_err(|err| failure(err.to_string()))?
        .map(|source| {
            String::from_utf8(source.bytes).map_err(|_| failure("expected UTF-8 text".to_string()))
        })
        .transpose()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn shared_reader_metadata_retains_full_mode_without_changing_source_mode() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("config");
        fs::write(&path, [0xff, b'\n', 0]).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o1640)).unwrap();
        let original = fs::metadata(&path).unwrap();
        let (source, metadata) = read_with_metadata(&path, 3, Links::Reject)
            .unwrap()
            .unwrap();
        assert_eq!(source.bytes, [0xff, b'\n', 0]);
        assert_eq!(source.mode, Some(0o640));
        assert_eq!(metadata.permissions().mode(), original.permissions().mode());
        assert_eq!(source.identity, FileIdentity::from_metadata(&metadata));
        // Returned metadata belongs to the captured bytes, not a later path stat.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o1640);
    }

    #[test]
    fn shared_reader_distinguishes_missing_paths_links_and_special_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("directory")).unwrap();
        symlink(root.join("directory"), root.join("alias")).unwrap();
        symlink(root.join("missing"), root.join("broken")).unwrap();
        fs::write(root.join("file"), [b'a', 0xff]).unwrap();
        symlink(root.join("file"), root.join("link")).unwrap();
        for links in [Links::Follow, Links::Reject] {
            for missing in ["absent", "missing/child", "alias/absent/deep"] {
                assert!(
                    read(&root.join(missing), 10, links).unwrap().is_none(),
                    "{missing}"
                );
            }
            for invalid in ["broken", "broken/child", "file/child", "directory"] {
                assert!(read(&root.join(invalid), 10, links).is_err(), "{invalid}");
            }
            assert!(read(Path::new("/dev/null"), 10, links).is_err());
            assert!(matches!(
                read(&root.join("file"), 1, links),
                Err(ReadError::TooLarge(1))
            ));
        }
        assert_eq!(
            read(&root.join("link"), 2, Links::Follow)
                .unwrap()
                .unwrap()
                .bytes,
            [b'a', 0xff]
        );
        assert!(read(&root.join("link"), 2, Links::Reject).is_err());
        assert!(read_text(&root.join("link"), 2)
            .unwrap_err()
            .to_string()
            .contains("UTF-8"));
        fs::write(root.join("file"), b"ok").unwrap();
        assert_eq!(
            read_text(&root.join("link"), 2).unwrap().as_deref(),
            Some("ok")
        );
        assert!(read_text_with_links(&root.join("link"), 2, Links::Reject).is_err());
        assert_eq!(
            read_text_with_links(&root.join("file"), 2, Links::Reject)
                .unwrap()
                .as_deref(),
            Some("ok")
        );
    }
}
