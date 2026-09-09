//! Inspect exactly the entry Slate would edit, not OpenCode's runtime merge.
use super::Report;
use crate::adapter::{opencode::config::Document, OpencodeAdapter};
use crate::config::file_read::{self, Links, MAX_TOOL_CONFIG_BYTES};
use crate::env::SlateEnv;
use std::fs;

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    report.scope = "Read-only inspection of Slate's selected OpenCode TUI file. Project configuration, runtime overrides, terminal color support and the running application are not checked; no tool is launched or configuration changed.";
    let selected = OpencodeAdapter::tui_config_path(env);
    report.add_code(
        "selected_config",
        "info",
        "OpenCode TUI entry selected by Slate",
        &selected,
        None,
    );
    if let Some(error) = env.opencode_tui_config_error() {
        report.add_code("unresolved_config", "error", error, &selected,
            Some("Correct the explicit file path or unset OPENCODE_TUI_CONFIG deliberately. Directory traversal must be resolvable, and file paths must not end in '/' or '/.'.".into()));
        return;
    }
    if env.opencode_tui_config_was_relative() {
        report.add_code("relative_config", "info", "Relative OPENCODE_TUI_CONFIG was resolved to this absolute path when the environment was captured", &selected,
            Some("Launching Slate from another directory can select a different file. Use an absolute override for consistent selection across invocations.".into()));
    }
    if env.session().is_isolated() {
        report.add_code(
            "isolated_profile",
            "info",
            "Isolated profile: host OPENCODE_TUI_CONFIG is ignored",
            &selected,
            None,
        );
    }
    for candidate in OpencodeAdapter::tui_config_paths(env)
        .into_iter()
        .filter(|p| *p != selected)
    {
        match fs::symlink_metadata(&candidate) {
            Ok(_) => report.add_code("alternate_config", "warning", "Another TUI candidate exists; it is not the entry selected by Slate", &candidate,
                Some("Review other user and project TUI configs if OpenCode still looks different. Slate does not resolve OpenCode's runtime configuration merge.".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && file_read::confirm_missing(&candidate).is_ok() => {},
            Err(_) => report.add_code("alternate_unreadable", "warning", "Cannot inspect an alternate TUI candidate", &candidate,
                Some("Check its parent directories and permissions; its absence is not confirmed.".into())),
        }
    }
    let content = match file_read::read(&selected, MAX_TOOL_CONFIG_BYTES, Links::Reject) {
        Ok(Some(source)) => match String::from_utf8(source.bytes) {
            Ok(content) => content,
            Err(_) => {
                report.add_code("invalid_encoding", "error", "OpenCode TUI config is not UTF-8; file contents omitted", &selected,
                    Some("Repair the encoding locally before applying a theme; keep a backup of the original.".into()));
                return;
            }
        },
        Ok(None) => {
            report.add_code("missing_config", "warning", "The selected OpenCode TUI file is absent; no theme connection is confirmed", &selected,
                Some("Initialize OpenCode's user configuration if needed, then set the root theme to \"system\" in this TUI file if you want terminal-following colors. Slate skips an uninitialized default config directory.".into()));
            return;
        }
        Err(error) => {
            report.add_code("unsafe_config", "error", format!("Cannot safely read OpenCode TUI config: {error}"), &selected,
                Some("Check the file and its parent paths: Slate requires a regular, non-symlink file up to 8 MiB. Review linked dotfiles manually; do not replace them blindly.".into()));
            return;
        }
    };
    let document = match Document::parse(&content, &selected) {
        Ok(document) => document,
        Err(error) => {
            report.add_code("invalid_config", "error", error.to_string(), &selected,
                Some("Repair JSON/JSONC syntax, duplicate root keys or a non-string theme before applying; diagnostic output omits configuration values.".into()));
            return;
        }
    };
    report.add_code(
        "valid_config",
        "ok",
        "OpenCode TUI JSON/JSONC passes Slate's syntax and root-field checks",
        &selected,
        None,
    );
    match document.has_system_theme() {
        Some(true) => report.add_code("system_theme", "ok", "The root theme is \"system\" in this file", &selected,
            Some("If the display differs, review project/runtime overrides and the terminal palette. This file alone does not prove the live theme or that Slate originally set it.".into())),
        Some(false) => report.add_code("custom_theme", "warning", "A custom theme is selected; terminal-following colors are not configured here", &selected,
            Some("Keep the custom theme if intentional. For terminal-following colors, set the root theme to \"system\" in this TUI file; unrelated fields and comments need not change.".into())),
        None => report.add_code("unset_theme", "warning", "No explicit root theme is set in this file", &selected,
            Some("If you want terminal-following colors, set the root theme to \"system\". A missing field does not tell us which theme OpenCode currently uses.".into())),
    }
}
