//! Inspect exactly the entry Slate would edit, not OpenCode's runtime merge.
use super::{tool_files, Report};
use crate::adapter::{opencode::config::Document, OpencodeAdapter};
use crate::config::file_read::{self, Links, MAX_TOOL_CONFIG_BYTES};
use crate::env::SlateEnv;

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    report.scope = "Read-only inspection of Slate's OpenCode 2 CLI file and generated palette. Inline overrides, terminal color support and the running application are not checked; no tool is launched or configuration changed.";
    let selected = OpencodeAdapter::cli_config_path(env);
    report.add_code(
        "selected_config",
        "info",
        "OpenCode 2 CLI entry selected by Slate",
        &selected,
        None,
    );
    let content = match file_read::read(&selected, MAX_TOOL_CONFIG_BYTES, Links::Reject) {
        Ok(Some(source)) => match String::from_utf8(source.bytes) {
            Ok(content) => content,
            Err(_) => {
                report.add_code("invalid_encoding", "error", "OpenCode 2 CLI config is not UTF-8; file contents omitted", &selected,
                    Some("Repair the encoding locally before applying a theme; keep a backup of the original.".into()));
                return;
            }
        },
        Ok(None) => {
            report.add_code("missing_config", "warning", "The selected OpenCode 2 CLI file is absent; no theme connection is confirmed", &selected,
                Some("Initialize OpenCode's user configuration if needed, then run `slate tools sync opencode` to connect Slate colors. Slate skips an uninitialized default config directory.".into()));
            return;
        }
        Err(error) => {
            report.add_code("unsafe_config", "error", format!("Cannot safely read OpenCode 2 CLI config: {error}"), &selected,
                Some("Check the file and its parent paths: Slate requires a regular, non-symlink file up to 8 MiB. Review linked dotfiles manually; do not replace them blindly.".into()));
            return;
        }
    };
    let document = match Document::parse(&content, &selected) {
        Ok(document) => document,
        Err(error) => {
            report.add_code("invalid_config", "error", error.to_string(), &selected,
                Some("Repair JSON/JSONC syntax, duplicate keys or an invalid theme.name before applying; diagnostic output omits configuration values.".into()));
            return;
        }
    };
    report.add_code(
        "valid_config",
        "ok",
        "OpenCode 2 CLI JSON/JSONC passes Slate's syntax and theme-field checks",
        &selected,
        None,
    );
    match document.has_slate_theme() {
        Some(true) => report.add_code("slate_theme", "ok", "The theme.name is \"slate-sync\" in this file", &selected,
            Some("If the display differs, review OPENCODE_CLI_CONFIG_CONTENT overrides and the terminal palette. This file alone does not prove the live theme or that Slate originally set it.".into())),
        Some(false) => report.add_code("custom_theme", "warning", "Another theme is selected; Slate colors are not configured here", &selected,
            Some("Keep the custom theme if intentional. Run `slate tools sync opencode` to generate and connect the Slate palette.".into())),
        None => report.add_code("unset_theme", "warning", "No explicit theme.name is set in this file", &selected,
            Some("Run `slate tools sync opencode` to generate and connect the Slate palette. A missing field does not tell us which theme OpenCode currently uses.".into())),
    }
    let asset = OpencodeAdapter::theme_path(env);
    if let Ok(content) = tool_files::read(
        report,
        env,
        &asset,
        MAX_TOOL_CONFIG_BYTES,
        "asset_file",
        "Slate OpenCode theme",
    ) {
        match content {
            Some(content) if crate::adapter::opencode::palette::is_owned(content.as_bytes()) => {
                if let Some(theme) = tool_files::saved_theme(report, env) {
                    let matches = crate::adapter::opencode::palette::render(&theme).is_ok_and(|expected| expected == content);
                    report.add_code("palette_match", if matches {"ok"} else {"warning"},
                        if matches {"Palette matches the saved Slate theme"} else {"Palette differs from the saved Slate theme"},
                        &asset, (!matches).then(|| "Run `slate tools sync opencode`, then reopen OpenCode.".into()));
                }
            }
            Some(_) => report.add_code("asset_ownership", "warning", "The same-named theme file is not Slate-owned", &asset, None),
            None => report.add_code("asset_ownership", "warning", "Slate's OpenCode theme file is missing", &asset,
                Some("Run `slate tools sync opencode` to generate the palette, then reopen OpenCode.".into())),
        }
    }
}
