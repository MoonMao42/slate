//! Shared Caskroom/download file publication. Existing files are never replaced;
//! stage the whole family before publication and undo only our unchanged additions.
use crate::{
    config::file_read::{self, FileIdentity, Links},
    env::SlateEnv,
    error::{Result, SlateError},
};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub(super) const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
// IosevkaTerm includes three variants of every style; its expanded family can
// exceed 512 MiB. Share these limits with the archive preflight.
pub(super) const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(super) const MAX_ENTRIES: usize = 10_000;
pub(super) const MAX_FONTS: usize = 512;
pub(super) const MAX_DEPTH: usize = 16;

fn failure(reason: impl std::fmt::Display) -> SlateError {
    SlateError::Internal(format!(
        "Font file installation {reason}; file contents omitted"
    ))
}

fn named_failure(reason: &str, path: &Path) -> SlateError {
    failure(format!(
        "{reason}: {}",
        path.display().to_string().escape_default()
    ))
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Report {
    pub installed: usize,
    pub unchanged: usize,
}

pub(super) fn install(source: &Path, env: &SlateEnv) -> Result<Report> {
    install_with(source, env, |_| Ok(()))
}

fn install_with(
    source: &Path,
    env: &SlateEnv,
    before_publish: impl FnMut(usize) -> Result<()>,
) -> Result<Report> {
    install_for_backend(
        source,
        env,
        crate::platform::fonts::backend(),
        before_publish,
    )
}

fn install_for_backend(
    source: &Path,
    env: &SlateEnv,
    backend: crate::platform::fonts::FontPlatformBackend,
    mut before_publish: impl FnMut(usize) -> Result<()>,
) -> Result<Report> {
    let sources = collect(source)?;
    let target = Target::prepare(env, backend)?;
    let scratch = Scratch::new(&target.path)?;
    let mut staged = Vec::new();
    let mut total = 0u64;
    // No font is published until every source and existing destination passes.
    // Disk staging bounds live memory to individual reads, not the whole family.
    for source in sources {
        let bytes = read_source(&source)?;
        total = account_bytes(total, bytes.len() as u64)?;
        let destination = target.path.join(source.file_name().unwrap());
        let existing = existing_matches(&destination, &bytes)?;
        let mut file = tempfile::NamedTempFile::new_in(scratch.path())?;
        // One identity-checked directory owner handles cleanup. Individual path
        // destructors must not unlink files in a substituted font directory.
        file.disable_cleanup(true);
        file.write_all(&bytes)?;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o644))?;
        file.as_file().sync_all()?;
        staged.push(Staged {
            source,
            destination,
            file,
            existing,
        });
    }
    // Detect source or destination edits that occurred during the staging pass.
    target.check(env)?;
    for item in &staged {
        let bytes = read_source(&item.source)?;
        if bytes != read_bytes(item.file.path())? {
            return Err(named_failure(
                "source changed during preparation",
                &item.source,
            ));
        }
        existing_matches(&item.destination, &bytes)?;
    }

    let mut published = Vec::new();
    let mut unchanged = 0;
    let result = (|| {
        for item in staged {
            before_publish(published.len())?;
            target.check(env)?;
            let bytes = read_bytes(item.file.path())?;
            if existing_matches(&item.destination, &bytes)? {
                unchanged += 1;
                continue;
            }
            if item.existing {
                return Err(named_failure(
                    "an existing font disappeared during installation",
                    &item.destination,
                ));
            }
            let identity = FileIdentity::from_metadata(&item.file.as_file().metadata()?);
            let file = match item.file.persist_noclobber(&item.destination) {
                Ok(file) => file,
                Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if existing_matches(&item.destination, &bytes)? {
                        unchanged += 1;
                        continue;
                    }
                    return Err(named_failure(
                        "destination changed during publication",
                        &item.destination,
                    ));
                }
                Err(e) => {
                    return Err(named_failure(
                        &format!("could not publish ({:?})", e.error.kind()),
                        &item.destination,
                    ))
                }
            };
            published.push(Published {
                path: item.destination,
                source: item.source,
                identity,
                _file: file,
            });
        }
        Ok(Report {
            installed: published.len(),
            unchanged,
        })
    })();
    if let Err(error) = result {
        let mut remaining = Vec::new();
        for item in published.iter().rev() {
            if target.check(env).is_err() || item.undo().is_err() {
                remaining.push(item.path.display().to_string().escape_default().to_string());
            }
        }
        if remaining.is_empty() {
            return Err(failure(format!(
                "stopped; {} new file(s) rolled back; existing files kept. {error}",
                published.len()
            )));
        }
        return Err(failure(format!("stopped; cleanup was incomplete. Preserve and review these paths before retrying: {}. {error}", remaining.join(", "))));
    }
    if fs::File::open(&target.path)
        .and_then(|dir| dir.sync_all())
        .is_err()
    {
        eprintln!("warning: font files were installed, but their directory could not be synced");
    }
    result
}

struct Staged {
    source: PathBuf,
    destination: PathBuf,
    file: tempfile::NamedTempFile,
    existing: bool,
}

struct Scratch {
    directory: Option<tempfile::TempDir>,
    identity: FileIdentity,
}

impl Scratch {
    fn new(target: &Path) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix(".slate-font-install-")
            .tempdir_in(target)?;
        let identity = FileIdentity::from_metadata(&fs::metadata(directory.path())?);
        Ok(Self {
            directory: Some(directory),
            identity,
        })
    }
    fn path(&self) -> &Path {
        self.directory.as_ref().unwrap().path()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let directory = self.directory.take().unwrap();
        let ours = fs::symlink_metadata(directory.path())
            .is_ok_and(|meta| meta.is_dir() && FileIdentity::from_metadata(&meta) == self.identity)
            && check_directories(directory.path()).is_ok();
        if !ours {
            // The original private directory may have moved. Retain it instead
            // of recursively cleaning an unrelated directory now at this path.
            let _ = directory.keep();
            eprintln!("warning: font staging directory moved or changed; temporary files were retained for manual review");
        }
    }
}

struct Published {
    path: PathBuf,
    source: PathBuf,
    identity: FileIdentity,
    // Keep the inode alive until success/undo so an external replacement cannot
    // recycle its identity and be mistaken for this operation's new file.
    _file: fs::File,
}

impl Published {
    fn undo(&self) -> Result<()> {
        let Some(current) =
            file_read::read(&self.path, MAX_FONT_BYTES, Links::Reject).map_err(failure)?
        else {
            return Ok(());
        };
        // Sources normally remain in the owned extraction directory. If a shared
        // Caskroom source changed/disappeared, keep the output for manual review.
        // Rechecking identity and exact bytes avoids undoing observed outside edits.
        if current.identity != self.identity || current.bytes != read_source(&self.source)? {
            return Err(named_failure(
                "will not remove an externally changed file",
                &self.path,
            ));
        }
        let metadata = fs::symlink_metadata(&self.path)?;
        if !metadata.is_file() || FileIdentity::from_metadata(&metadata) != self.identity {
            return Err(named_failure("will not remove a replaced file", &self.path));
        }
        fs::remove_file(&self.path)?;
        Ok(())
    }
}

fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    file_read::read(path, MAX_FONT_BYTES, Links::Reject)
        .map_err(failure)?
        .map(|source| source.bytes)
        .ok_or_else(|| named_failure("source is missing", path))
}

fn account_bytes(total: u64, next: u64) -> Result<u64> {
    total
        .checked_add(next)
        .filter(|value| *value <= MAX_TOTAL_BYTES)
        .ok_or_else(|| failure("exceeded the 2 GiB family limit"))
}

fn read_source(path: &Path) -> Result<Vec<u8>> {
    let bytes = read_bytes(path)?;
    // Identify SFNT/OpenType/collections only; this is not full font validation,
    // table/checksum checking or a guarantee that an OS will activate the family.
    if bytes.len() <= 12 || !crate::platform::fonts::has_sfnt_signature(&bytes) {
        return Err(named_failure(
            "source is empty or has an unsupported font signature",
            path,
        ));
    }
    Ok(bytes)
}

fn existing_matches(path: &Path, bytes: &[u8]) -> Result<bool> {
    match file_read::read(path, MAX_FONT_BYTES, Links::Reject).map_err(failure)? {
        Some(existing) if existing.bytes == bytes => Ok(true),
        Some(_) => Err(named_failure("found a different existing font; no files replaced; preserve or relocate it before retrying", path)),
        None => Ok(false),
    }
}

fn collect(root: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![(root.to_owned(), 0)];
    let mut files = Vec::new();
    let mut names = BTreeSet::new();
    let mut entries = 0;
    while let Some((directory, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err(failure("source tree exceeded the depth limit"));
        }
        if !fs::symlink_metadata(&directory)?.is_dir() {
            return Err(named_failure(
                "source directory is linked or not a directory",
                &directory,
            ));
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            entries += 1;
            if entries > MAX_ENTRIES {
                return Err(failure("source tree exceeded the entry limit"));
            }
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(named_failure("source tree contains a link", &path));
            }
            if kind.is_dir() {
                pending.push((path, depth + 1));
            } else if is_font(&path) {
                if !kind.is_file() {
                    return Err(named_failure("source is not a regular font file", &path));
                }
                let name = entry.file_name();
                let name = name
                    .to_str()
                    .filter(|name| !name.chars().any(char::is_control))
                    .ok_or_else(|| failure("source font name is not valid printable UTF-8"))?;
                if !names.insert(name.to_lowercase()) {
                    return Err(failure(
                        "source fonts have conflicting basenames; no files installed",
                    ));
                }
                files.push(path);
                if files.len() > MAX_FONTS {
                    return Err(failure("source tree exceeded the font-count limit"));
                }
            }
        }
    }
    if files.is_empty() {
        return Err(failure("source contains no font files"));
    }
    files.sort();
    Ok(files)
}

fn is_font(path: &Path) -> bool {
    crate::platform::fonts::supported_font_extension(path)
}

struct Target {
    path: PathBuf,
    identity: FileIdentity,
    backend: crate::platform::fonts::FontPlatformBackend,
}
impl Target {
    fn prepare(
        env: &SlateEnv,
        backend: crate::platform::fonts::FontPlatformBackend,
    ) -> Result<Self> {
        let path = target_path(env, backend)?;
        check_directories(&path)?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)?;
        check_directories(&path)?;
        let identity = FileIdentity::from_metadata(&fs::metadata(&path)?);
        Ok(Self {
            path,
            identity,
            backend,
        })
    }
    fn check(&self, env: &SlateEnv) -> Result<()> {
        if target_path(env, self.backend)? != self.path {
            return Err(failure("font directory moved"));
        }
        check_directories(&self.path)?;
        if FileIdentity::from_metadata(&fs::metadata(&self.path)?) != self.identity {
            return Err(failure("font directory was replaced"));
        }
        Ok(())
    }
}

fn target_path(
    env: &SlateEnv,
    backend: crate::platform::fonts::FontPlatformBackend,
) -> Result<PathBuf> {
    crate::platform::fonts::install_directory(env, backend).map_err(|error| failure(error.kind()))
}

fn check_directories(path: &Path) -> Result<()> {
    // The selected HOME/data root was resolved; its suffix must not redirect writes.
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(named_failure(
                    "target directory is linked or not a directory",
                    ancestor,
                ))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
