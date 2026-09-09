//! Read-only projection of clean's file effects. No checkpoint, locks or processes.
use super::{
    edits::{self, Edit},
    preflight,
};
use crate::config::{read_snapshot_source, MAX_SNAPSHOT_BYTES};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use serde::Serialize;
use std::io::Write;
use std::path::Path;

#[derive(Serialize)]
struct DisplayPath {
    path: String,
    path_is_lossy: bool,
}

impl From<&Path> for DisplayPath {
    fn from(path: &Path) -> Self {
        Self {
            path: path.display().to_string(),
            path_is_lossy: path.to_str().is_none(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Remove,
    Rewrite,
    Unchanged,
    Blocked,
}

#[derive(Serialize)]
struct Change {
    #[serde(flatten)]
    target: DisplayPath,
    action: Action,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Default, Serialize)]
struct Counts {
    remove: usize,
    rewrite: usize,
    unchanged: usize,
    blocked: usize,
    warnings: usize,
}

#[derive(Serialize)]
struct Preview {
    schema_version: u32,
    changes: Vec<Change>,
    directories_to_remove: Vec<DisplayPath>,
    preserved_user_directory: DisplayPath,
    summary: Counts,
    scan_complete: bool,
    issues: Vec<String>,
    snapshot_required: bool,
    snapshot_write_checked: bool,
    target_writes_checked: bool,
    writer_and_recovery_checked: bool,
    watcher_action: &'static str,
    terminal_reload: &'static str,
    uninstalls_tools: bool,
}

fn owned_file(env: &SlateEnv, path: &Path) -> bool {
    (path.starts_with(env.config_dir()) && !path.starts_with(env.config_dir().join("user")))
        || path.starts_with(env.nvim_config_dir().join("lua/slate"))
        || path == env.fish_loader_path()
        || path == env.slate_cache_dir().join("current_theme.lua")
        || (path.parent() == Some(env.nvim_config_dir().join("colors").as_path())
            && path.file_name().is_some_and(preflight::is_nvim_shim))
}

fn project(env: &SlateEnv, path: &Path, bytes: &[u8]) -> Result<Edit> {
    let mut marker_paths = vec![
        env.zshrc_path(),
        env.home().join(".gitconfig"),
        env.nvim_config_dir().join("init.lua"),
        env.nvim_config_dir().join("init.vim"),
    ];
    marker_paths.extend(env.bash_startup_paths());
    marker_paths.extend(env.tmux_config_candidates());
    // Run any integration transform first, even when an unusual profile places
    // the file inside an owned tree: a parse failure also stops real cleanup.
    let edited = if marker_paths.iter().any(|candidate| candidate == path) {
        Edit::replaced(
            bytes,
            crate::adapter::marker_block::strip_managed_blocks_bytes(bytes)?,
        )
    } else if crate::adapter::GhosttyAdapter
        .integration_candidate_paths_with_env(env)?
        .iter()
        .any(|candidate| candidate == path)
    {
        let prefix = env
            .config_dir()
            .join("managed/ghostty")
            .to_string_lossy()
            .into_owned();
        Edit::replaced(
            bytes,
            crate::adapter::GhosttyAdapter::strip_managed_references_from_bytes(
                bytes,
                prefix.as_bytes(),
            ),
        )
    } else if path == crate::adapter::KittyAdapter::resolve_config_path_with_env(env) {
        edits::kitty(env, bytes)?
    } else if crate::adapter::AlacrittyAdapter::integration_candidate_paths_with_env(env)
        .iter()
        .any(|candidate| candidate == path)
    {
        edits::alacritty(env, bytes)?
    } else if path == crate::adapter::StarshipAdapter::integration_config_path_with_env(env) {
        edits::starship(bytes)?
    } else if path == crate::adapter::BtopAdapter::config_path(env) {
        edits::btop(env, bytes)?
    } else if path == crate::adapter::BtopAdapter::theme_path(env) {
        edits::btop_theme(bytes)?
    } else if path == crate::adapter::YaziAdapter::config_path(env) {
        edits::yazi(bytes)?
    } else if path == crate::adapter::YaziAdapter::flavor_path(env) {
        edits::yazi_asset(bytes, false)?
    } else if path == crate::adapter::YaziAdapter::syntax_path(env) {
        edits::yazi_asset(bytes, true)?
    } else if path == crate::adapter::ZellijAdapter::config_path(env)? {
        edits::zellij(bytes)?
    } else if path == crate::adapter::ZellijAdapter::paths(env)?[1] {
        edits::zellij_theme(bytes)?
    } else if crate::adapter::OpencodeAdapter::tui_config_paths(env)
        .iter()
        .any(|candidate| candidate == path)
    {
        edits::opencode(bytes, path)?
    } else if owned_file(env, path) {
        Edit::Remove
    } else {
        return Err(SlateError::InvalidConfig(
            "No clean projection for this target".into(),
        ));
    };
    Ok(if owned_file(env, path) {
        Edit::Remove
    } else {
        edited
    })
}

fn inspect(env: &SlateEnv) -> Preview {
    let mut preview = Preview {
        schema_version: 1,
        changes: Vec::new(),
        directories_to_remove: Vec::new(),
        preserved_user_directory: env.config_dir().join("user").as_path().into(),
        summary: Counts::default(),
        scan_complete: true,
        issues: Vec::new(),
        snapshot_required: true,
        snapshot_write_checked: false,
        target_writes_checked: false,
        writer_and_recovery_checked: false,
        watcher_action: if env.session().is_isolated() {
            "leave_untouched"
        } else {
            "request_stop"
        },
        terminal_reload: if env.session().can_reload_terminal() {
            "best_effort_ghostty"
        } else {
            "skip"
        },
        uninstalls_tools: false,
    };
    let targets = match preflight::validate_storage_paths(env)
        .and_then(|_| preflight::inspect_targets(env))
    {
        Ok(targets) => targets,
        Err(error) => {
            preview.scan_complete = false;
            preview.issues.push(error.to_string());
            return preview;
        }
    };
    preview.directories_to_remove = targets
        .directories
        .iter()
        .map(|path| path.as_path().into())
        .collect();
    let mut remaining = MAX_SNAPSHOT_BYTES;
    for entry in targets.files {
        let path = entry.original_path;
        let result = (|| -> Result<(Action, Option<String>)> {
            if path.to_str().is_none() || !crate::config::restore_path_is_safe(&path) {
                return Err(SlateError::InvalidConfig(
                    "Path cannot be stored safely in a restore manifest".into(),
                ));
            }
            let Some(source) = read_snapshot_source(&path, &mut remaining)? else {
                return Ok((Action::Unchanged, Some("File is absent".into())));
            };
            Ok(match project(env, &path, &source.bytes)? {
                Edit::Keep(note) => {
                    if note.is_some() {
                        preview.summary.warnings += 1;
                    }
                    (Action::Unchanged, note.map(str::to_owned))
                }
                Edit::Replace(_) => (Action::Rewrite, None),
                Edit::Remove => (Action::Remove, None),
            })
        })();
        let (action, reason) = match result {
            Ok(change) => change,
            Err(error) => (Action::Blocked, Some(error.to_string())),
        };
        match action {
            Action::Remove => preview.summary.remove += 1,
            Action::Rewrite => preview.summary.rewrite += 1,
            Action::Unchanged => preview.summary.unchanged += 1,
            Action::Blocked => preview.summary.blocked += 1,
        }
        preview.changes.push(Change {
            target: path.as_path().into(),
            action,
            reason,
        });
    }
    preview
}

fn terminal_text(text: &str) -> String {
    let mut output = String::new();
    for c in text.chars() {
        if c.is_control() || matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            output.extend(c.escape_default());
        } else {
            output.push(c);
        }
    }
    output
}

fn text_report(preview: &Preview) -> String {
    use std::fmt::Write;
    let mut output = String::from("◆ Clean preview (read-only)\n");
    for change in &preview.changes {
        let action = match change.action {
            Action::Remove => "remove",
            Action::Rewrite => "rewrite",
            Action::Unchanged => "unchanged",
            Action::Blocked => "blocked",
        };
        writeln!(
            output,
            "  {action:9} {}",
            terminal_text(&change.target.path)
        )
        .unwrap();
        if let Some(reason) = &change.reason {
            writeln!(output, "            {}", terminal_text(reason)).unwrap();
        }
    }
    for directory in &preview.directories_to_remove {
        writeln!(output, "  remove-dir {}", terminal_text(&directory.path)).unwrap();
    }
    writeln!(
        output,
        "Preserved if present (not scanned): {}",
        terminal_text(&preview.preserved_user_directory.path)
    )
    .unwrap();
    writeln!(
        output,
        "{} remove, {} rewrite, {} unchanged, {} blocked; {} warning(s).",
        preview.summary.remove,
        preview.summary.rewrite,
        preview.summary.unchanged,
        preview.summary.blocked,
        preview.summary.warnings
    )
    .unwrap();
    for issue in &preview.issues {
        writeln!(output, "Incomplete scan: {}", terminal_text(issue)).unwrap();
    }
    writeln!(
        output,
        "On execution: watcher={}, terminal reload={}; installed tools are not uninstalled.",
        preview.watcher_action, preview.terminal_reload
    )
    .unwrap();
    output.push_str("A pre-clean snapshot is required. Backup/target writes, writer availability and pending recovery are not checked here.\nPreview only: no files, snapshots or processes were changed. Execution rechecks current inputs; this is not an atomic transaction.\n");
    output
}

pub(super) fn handle(env: &SlateEnv, json: bool) -> Result<()> {
    let preview = inspect(env);
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&preview)?)
    } else {
        text_report(&preview)
    };
    match std::io::stdout().lock().write_all(output.as_bytes()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(error) => return Err(error.into()),
    }
    if !preview.scan_complete || preview.summary.blocked > 0 {
        return Err(SlateError::InvalidConfig(
            "Clean preview is blocked or incomplete; no files were changed".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn clean_preview_display_paths_mark_lossy_names_and_escape_controls() {
        let path =
            std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/fixture/\xff".to_vec()));
        let display = DisplayPath::from(path.as_path());
        assert!(display.path_is_lossy);
        let encoded = serde_json::to_value(display).unwrap();
        assert_eq!(encoded["path_is_lossy"], true);
        assert_eq!(
            terminal_text("中文\u{1b}\n\u{202e}"),
            "中文\\u{1b}\\n\\u{202e}"
        );
    }
}
