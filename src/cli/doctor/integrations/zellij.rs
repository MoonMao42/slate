use super::{tool_files, Report};
use crate::{
    adapter::{zellij::config, ZellijAdapter},
    config::file_read::MAX_TOOL_CONFIG_BYTES,
    env::SlateEnv,
};
use std::path::Path;

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    report.scope = "Read-only Zellij file checks; no tool is launched or configuration changed. Checks use Zellij 0.45.1 KDL v1 semantics, not a native version probe. Same-name conflicts are checked in inline themes and other .kdl files (256 directory entries / 8 MiB total). Layout/CLI overrides, effective native merging and running appearance are not verified. Files are observed separately, not atomically.";
    tool_files::availability(report, env, "zellij");
    let theme = tool_files::saved_theme(report, env);
    let path = match ZellijAdapter::config_path(env) {
        Ok(path) => path,
        Err(_) => {
            report.add_code("config_path", "error", "Cannot safely resolve the selected Zellij profile; no config or theme file was read", Path::new(""),
                Some("Use explicit absolute ZELLIJ_CONFIG_DIR/FILE paths; empty/relative overrides and the implicit system profile are not inspected. Reopen Slate if directory precedence changed.".into()));
            return;
        }
    };
    report.add_code("reload", "info", "No running Zellij session was inspected or commanded", &path,
        Some("Native file watching may apply saved colors. Layout/CLI overrides can still win; if needed, start a new session using this configuration. No live reload is claimed.".into()));
    let Ok(content) = tool_files::read(
        report,
        env,
        &path,
        MAX_TOOL_CONFIG_BYTES,
        "config_file",
        "Zellij configuration",
    ) else {
        return;
    };
    let bytes = content.as_deref().unwrap_or("").as_bytes();
    match config::inspect_selection(bytes) {
        Ok(selected) => {
            for (index, code) in ["theme_static", "theme_dark", "theme_light"]
                .into_iter()
                .enumerate()
            {
                let slot = ["theme", "theme_dark", "theme_light"][index];
                report.add_code(code, if selected[index] { "ok" } else { "warning" },
                    if selected[index] { format!("{slot} selects Slate's named theme") }
                    else { format!("{slot} does not select Slate; static/dark/light choices can differ") }, &path,
                    (!selected[index]).then(|| "Slate sync sets all three slots to its saved theme. Review `slate tools sync zellij --dry-run` if that is wanted; native dark/light choices can override the static choice.".into()));
            }
        }
        Err(error) => {
            report.add_code("config_syntax", "error", format!("Cannot interpret Zellij KDL v1/theme slots: {error}"), &path,
                Some("Review syntax, duplicate/non-string choices and any reported complexity limit locally. No theme connection is inferred or repair attempted.".into()));
            return;
        }
    }
    let asset = match config::theme_path(env, bytes) {
        Ok(asset) => asset,
        Err(_) => {
            report.add_code("theme_path", "error", "Cannot resolve the Zellij theme directory; no asset or other theme file was read", &path,
                Some("Use a single absolute theme_dir string or the standard theme directory. Relative/tilde paths are not guessed.".into()));
            return;
        }
    };
    // Same resolver/limits as sync, without pinning any write destinations.
    match config::check_collisions(env, bytes, &asset) {
        Ok(()) => report.add_code("theme_conflicts", "ok", "No other inline or directory theme named slate-sync was found in the bounded scan", &asset, None),
        Err(error) => report.add_code("theme_conflicts", "error", format!("Theme conflict check did not pass: {error}"), &asset,
            Some("Review inline themes and other .kdl files locally, including same-name definitions. Sync will refuse unresolved conflicts or unsafe files. No file was renamed.".into())),
    }
    let expected = theme
        .as_ref()
        .and_then(|theme| match config::generated_theme(theme) {
            Ok(asset) => Some(asset),
            Err(_) => {
                report.add_code(
                    "palette_generation",
                    "error",
                    "Cannot generate a comparison palette; no match is claimed",
                    &asset,
                    None,
                );
                None
            }
        });
    tool_files::generated_asset(
        report,
        env,
        "zellij",
        &asset,
        ["asset_file", "asset_ownership", "palette_match"],
        config::owns_theme,
        expected.as_deref(),
    );
}
