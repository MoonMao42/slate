//! Resolve every file clean can edit/remove before any destructive action.
use crate::config::{OriginalFileState, RestoreEntry, MAX_RESTORE_ENTRIES};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn is_nvim_shim(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with("slate-") && name.ends_with(".lua")
}

fn blocked(path: &Path, reason: &str) -> SlateError {
    SlateError::InvalidConfig(format!(
        "Clean cancelled before removing files: {}: {reason}",
        path.display(),
    ))
}

/// SLATE_HOME is a boundary, not merely a prefix. A linked parent must not
/// redirect cleanup or its backup into the real HOME. System aliases above the
/// fixture root (e.g. /var -> /private/var on macOS) are resolved on both sides.
fn check_isolated_path(env: &SlateEnv, path: &Path) -> Result<()> {
    if !env.session().is_isolated() {
        return Ok(());
    }
    let home = fs::canonicalize(env.home())?;
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                let resolved = fs::canonicalize(ancestor)?;
                return if resolved.starts_with(&home) {
                    Ok(())
                } else {
                    Err(blocked(path, "path escapes the isolated SLATE_HOME"))
                };
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(blocked(path, "cannot resolve path"))
}

fn metadata(env: &SlateEnv, path: &Path) -> Result<Option<fs::Metadata>> {
    check_isolated_path(env, path)?;
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(blocked(
            path,
            "symbolic links are not supported by cleanup snapshots; preserve or relocate the link first",
        )),
        Ok(meta) => Ok(Some(meta)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub(super) fn validate_storage_paths(env: &SlateEnv) -> Result<()> {
    env.validate_opencode_tui_config()?;
    for path in [
        env.config_dir().to_owned(),
        env.slate_cache_dir().to_owned(),
        env.slate_cache_dir().join("backups"),
    ] {
        if metadata(env, &path)?.is_some_and(|meta| !meta.is_dir()) {
            return Err(blocked(&path, "expected a configuration/cache directory"));
        }
    }
    // A custom XDG_CACHE_HOME nested under a cleanup tree would delete the
    // backup and even unlink the live writer lock. Reject that layout early.
    let cache = resolve_with_missing_suffix(env.slate_cache_dir())?;
    for removed_root in [
        env.config_dir().to_owned(),
        env.nvim_config_dir().join("lua/slate"),
    ] {
        if cache.starts_with(resolve_with_missing_suffix(&removed_root)?) {
            return Err(blocked(
                env.slate_cache_dir(),
                "cache is inside a directory clean would remove",
            ));
        }
    }
    Ok(())
}

fn resolve_with_missing_suffix(path: &Path) -> Result<PathBuf> {
    for ancestor in path.ancestors() {
        match fs::canonicalize(ancestor) {
            Ok(root) => return Ok(root.join(path.strip_prefix(ancestor).expect("ancestor"))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(blocked(path, "cannot resolve storage path"))
}

fn add_file(env: &SlateEnv, path: PathBuf, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    if path.starts_with(env.config_dir().join("user")) {
        return Err(blocked(
            &path,
            "explicit cleanup target overlaps the preserved user tier",
        ));
    }
    if metadata(env, &path)?.is_some_and(|meta| !meta.is_file()) {
        return Err(blocked(&path, "expected a regular file"));
    }
    files.insert(path);
    if files.len() > MAX_RESTORE_ENTRIES {
        return Err(SlateError::InvalidConfig(
            "Clean target limit exceeded (512 files); no files removed".into(),
        ));
    }
    Ok(())
}

fn add_tree(
    env: &SlateEnv,
    path: &Path,
    files: &mut BTreeSet<PathBuf>,
    directories: &mut BTreeSet<PathBuf>,
    depth: usize,
) -> Result<()> {
    let Some(meta) = metadata(env, path)? else {
        return Ok(());
    };
    if !meta.is_dir() {
        return Err(blocked(path, "expected a managed directory"));
    }
    if depth > 64 || directories.len() >= 4096 {
        return Err(blocked(
            path,
            "managed directory scan limit exceeded (64 levels / 4096 directories)",
        ));
    }
    directories.insert(path.to_owned());
    let mut preserved_user = false;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        // Preserve the entire user tier, including links and unusual entries.
        if path == env.config_dir() && entry.file_name() == "user" {
            preserved_user = true;
            continue;
        }
        let child = entry.path();
        if metadata(env, &child)?.is_some_and(|meta| meta.is_dir()) {
            add_tree(env, &child, files, directories, depth + 1)?;
        } else {
            add_file(env, child, files)?;
        }
    }
    if preserved_user {
        directories.remove(path);
    }
    Ok(())
}

pub(super) fn snapshot_targets(env: &SlateEnv) -> Result<Vec<RestoreEntry>> {
    Ok(inspect_targets(env)?.files)
}

pub(super) struct CleanTargets {
    pub files: Vec<RestoreEntry>,
    pub directories: Vec<PathBuf>,
}

pub(super) fn inspect_targets(env: &SlateEnv) -> Result<CleanTargets> {
    env.validate_opencode_tui_config()?;
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    let mut paths = vec![
        env.zshrc_path(),
        env.fish_loader_path(),
        env.home().join(".gitconfig"),
        crate::adapter::StarshipAdapter::integration_config_path_with_env(env),
        crate::adapter::KittyAdapter::resolve_config_path_with_env(env),
        env.nvim_config_dir().join("init.lua"),
        env.nvim_config_dir().join("init.vim"),
        env.slate_cache_dir().join("current_theme.lua"),
        env.managed_file("config.toml"),
        crate::adapter::BtopAdapter::config_path(env),
        crate::adapter::BtopAdapter::theme_path(env),
    ];
    paths.extend(env.bash_startup_paths());
    paths.extend(crate::adapter::YaziAdapter::paths(env));
    paths.extend(crate::adapter::ZellijAdapter::paths(env)?);
    paths.extend(env.tmux_config_candidates());
    paths.extend(crate::adapter::AlacrittyAdapter::integration_candidate_paths_with_env(env));
    paths.extend(crate::adapter::GhosttyAdapter.integration_candidate_paths_with_env(env)?);
    paths.extend(crate::adapter::OpencodeAdapter::tui_config_paths(env));
    for path in paths {
        add_file(env, path, &mut files)?;
    }
    add_tree(env, env.config_dir(), &mut files, &mut directories, 0)?;
    add_tree(
        env,
        &env.nvim_config_dir().join("lua/slate"),
        &mut files,
        &mut directories,
        0,
    )?;
    let colors = env.nvim_config_dir().join("colors");
    if let Some(meta) = metadata(env, &colors)? {
        if !meta.is_dir() {
            return Err(blocked(&colors, "expected a colors directory"));
        }
        for entry in fs::read_dir(colors)? {
            let entry = entry?;
            if is_nvim_shim(&entry.file_name()) {
                add_file(env, entry.path(), &mut files)?;
            }
        }
    }
    // Backups must not be redirected through a link (especially one into a
    // tree this operation is about to delete).
    let backups = env.slate_cache_dir().join("backups");
    if metadata(env, &backups)?.is_some_and(|meta| !meta.is_dir()) {
        return Err(blocked(&backups, "expected a backup directory"));
    }
    let files = files
        .into_iter()
        .enumerate()
        .map(|(index, path)| RestoreEntry {
            tool_key: format!("clean-{index:04}"),
            display_tool: "Pre-clean files".into(),
            original_path: path,
            backup_path: None,
            original_state: OriginalFileState::Absent,
            unix_mode: None,
        })
        .collect();
    Ok(CleanTargets {
        files,
        directories: directories.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_preflight_rejects_cache_nested_in_deleted_trees() {
        let td = tempfile::tempdir().unwrap();
        for (root, blocked) in [
            (".config/slate/managed/cache", true),
            (".config/nvim/lua/slate/cache", true),
            (".config/slate-other/cache", false),
        ] {
            let env = SlateEnv::from_vars(|key| match key {
                "HOME" => Some(td.path().as_os_str().to_owned()),
                "XDG_CACHE_HOME" => Some(td.path().join(root).into_os_string()),
                _ => None,
            })
            .unwrap();
            assert_eq!(validate_storage_paths(&env).is_err(), blocked);
            assert_eq!(fs::read_dir(td.path()).unwrap().count(), 0);
        }
    }
}
