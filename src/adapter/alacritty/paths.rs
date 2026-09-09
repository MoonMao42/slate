//! User-level TOML candidates from Alacritty's documented Unix search order.
//! System configs, XDG_CONFIG_DIRS, YAML and per-process --config overrides are
//! deliberately not automatic write targets.
use crate::config::file_read::{confirm_missing, directory_alias_target};
use crate::env::SlateEnv;
use std::fs;
use std::path::PathBuf;

pub(super) fn candidates(env: &SlateEnv) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut destinations = std::collections::BTreeSet::new();
    for path in [
        env.xdg_config_home().join("alacritty/alacritty.toml"),
        env.xdg_config_home().join("alacritty.toml"),
        env.home().join(".config/alacritty/alacritty.toml"),
        env.home().join(".alacritty.toml"),
    ] {
        let destination = directory_alias_target(&path).unwrap_or_else(|| path.clone());
        if !paths.contains(&path) && destinations.insert(destination) {
            paths.push(path);
        }
    }
    paths
}

pub(super) fn resolve(env: &SlateEnv) -> PathBuf {
    let paths = candidates(env);
    for path in &paths {
        match fs::symlink_metadata(path) {
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && confirm_missing(path).is_ok() =>
            {
                continue
            }
            _ => {}
        }
        // Keep an obstructed candidate visible to the bounded reader/setup
        // instead of silently selecting or creating a lower-priority file.
        return path.clone();
    }
    paths[0].clone()
}
