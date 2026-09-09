//! Resolve the same installation target for publication and cache refresh.
//! Only an explicitly configured data root may be outside HOME or itself aliased;
//! the managed suffix below the chosen root never opts into following links.
use super::FontPlatformBackend;
use crate::env::SlateEnv;
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

pub(crate) fn install_directory(
    env: &SlateEnv,
    backend: FontPlatformBackend,
) -> io::Result<PathBuf> {
    let (root, suffix) = match backend {
        FontPlatformBackend::Macos => (fs::canonicalize(env.home())?, "Library/Fonts"),
        FontPlatformBackend::Fontconfig if env.xdg_data_home_overridden() => {
            (resolve_configured_root(env.xdg_data_home())?, "fonts")
        }
        FontPlatformBackend::Fontconfig => (fs::canonicalize(env.home())?, ".local/share/fonts"),
    };
    Ok(root.join(suffix))
}

fn resolve_configured_root(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "font data root must be absolute without parent traversal",
        ));
    }
    // Resolve only the existing part of the user's chosen root. Missing normal
    // components are appended without creating anything during this read-only step.
    // A dangling link or inaccessible ancestor is an error, never a fallback.
    let mut ancestor = path;
    let mut suffix = Vec::new();
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                let mut resolved = fs::canonicalize(ancestor)?;
                if !fs::metadata(&resolved)?.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        "font data root is not a directory",
                    ));
                }
                for part in suffix.into_iter().rev() {
                    resolved.push(part);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(name) = ancestor.file_name() else {
                    return Err(error);
                };
                suffix.push(name.to_owned());
                let Some(parent) = ancestor.parent() else {
                    return Err(error);
                };
                ancestor = parent;
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
