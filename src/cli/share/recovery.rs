//! Import's file-write contract. Keep this list aligned with the selected
//! adapters; an unrecognized adapter fails closed until its paths are defined.
use super::ImportRequest;
use crate::adapter::{write_paths, ApplyStrategy, ToolRegistry};
use crate::config::RestorePoint;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::collections::BTreeSet;

pub(super) struct Checkpoint {
    pub point: RestorePoint,
    pub theme_tools: Vec<String>,
}

/// Called before the writer lock creates its cache path, and repeated by the
/// library entrypoint. Parent-directory links are allowed inside SLATE_HOME;
/// final links cannot be represented faithfully by ordinary restore manifests.
pub fn validate_storage_paths(env: &SlateEnv) -> Result<()> {
    crate::config::recovery_paths::validate_storage_paths(env, "Import")
}

pub(super) fn create(env: &SlateEnv, request: &ImportRequest) -> Result<Checkpoint> {
    validate_storage_paths(env)?;
    let mut files = BTreeSet::new();
    // Shared integration refresh reads these saved values as well as writing
    // flags/env files. Capture them even when a URI keeps a current value.
    for name in [
        "config.toml",
        "current",
        "current-font",
        "current-opacity",
        "auto.toml",
        "autorun-fastfetch",
        "managed/shell/env.zsh",
        "managed/shell/env.bash",
        "managed/shell/env.fish",
        "managed/starship/plain.toml",
    ] {
        files.insert(env.managed_file(name));
    }
    if request.opacity.is_some() {
        files.extend(crate::opacity::managed_paths(env));
    }
    if request.font.is_some() {
        // Font application considers all three terminals, independently of the
        // theme adapter presence probes, and may create their managed font files.
        for tool in ["ghostty", "alacritty", "kitty"] {
            write_paths::terminal_paths(env, tool, true, &mut files)?;
        }
    }
    let mut theme_tools = Vec::new();
    if request.theme.is_some() {
        env.validate_opencode_tui_config()?;
        files.insert(env.slate_cache_dir().join("current_theme.lua"));
        for adapter in ToolRegistry::default().adapters() {
            if adapter.apply_strategy() == ApplyStrategy::DetectAndInstall
                || !adapter.is_installed_with_env(env)?
            {
                continue;
            }
            let tool = adapter.tool_name();
            write_paths::add_theme_paths(env, tool, &mut files)?;
            theme_tools.push(tool.to_owned());
        }
    }
    let targets = crate::config::recovery_paths::targets(env, files, "Import")?;
    let point = crate::config::snapshot_import_targets_with_env(env, &targets).map_err(|err| {
        SlateError::InvalidConfig(format!(
            "Import cancelled before applying settings: recovery checkpoint failed: {err}"
        ))
    })?;
    Ok(Checkpoint { point, theme_tools })
}
