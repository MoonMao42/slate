use super::{tool_files, Report};
use crate::{
    adapter::{btop::config, BtopAdapter},
    config::file_read::MAX_TOOL_CONFIG_BYTES,
    env::SlateEnv,
};

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    report.scope = "Read-only btop file checks; no tool is launched or configuration changed. Only Slate's standard btop.conf and enumerated theme asset are inspected. Custom --config/theme-directory options, named-theme resolution, native compatibility and running appearance are not checked. Files are observed separately, not atomically.";
    tool_files::availability(report, env, "btop");
    let theme = tool_files::saved_theme(report, env);
    let path = BtopAdapter::config_path(env);
    if let Ok(content) = tool_files::read(
        report,
        env,
        &path,
        MAX_TOOL_CONFIG_BYTES,
        "config_file",
        "btop configuration",
    ) {
        match content.as_deref().map(|text| config::references_owned_theme(env, text)).transpose() {
            Ok(Some(true)) => report.add_code("theme_reference", "ok", "color_theme points directly to Slate's exact theme asset", &path, None),
            Ok(_) => report.add_code("theme_reference", "warning", "No exact Slate theme reference is confirmed; custom or named themes are not resolved", &path,
                Some("If Slate colors are wanted, review `slate tools sync btop --dry-run` and the asset ownership check first.".into())),
            Err(_) => report.add_code("theme_reference", "error", "Cannot interpret btop assignments unambiguously; file contents omitted", &path,
                Some("Review malformed or duplicate assignments locally. Comments alone do not establish a connection.".into())),
        }
    }
    let asset = BtopAdapter::theme_path(env);
    if let Ok(content) = tool_files::read(
        report,
        env,
        &asset,
        MAX_TOOL_CONFIG_BYTES,
        "asset_file",
        "Slate btop theme asset",
    ) {
        if let Some(content) = content {
            let owned = config::is_owned_theme(content.as_bytes());
            report.add_code("asset_ownership", if owned { "ok" } else { "warning" },
                if owned { "Slate's generated-file marker is present; this is not an authenticity check" }
                else { "The same-named asset lacks Slate's marker; synchronization will not overwrite it" }, &asset,
                (!owned).then(|| "Preserve and review this file manually before connecting. No file was renamed or replaced.".into()));
            if owned {
                if let Some(theme) = theme {
                    match config::matches_generated_theme(content.as_bytes(), &theme) {
                        Ok(matches) => report.add_code("palette_match", if matches { "ok" } else { "warning" },
                            if matches { "Asset exactly matches the generated palette for the saved theme" }
                            else { "Asset differs from the generated file for the saved theme; comments or formatting can also cause a difference" }, &asset,
                            (!matches).then(|| "Review `slate tools sync btop --dry-run` if you want to regenerate Slate's asset.".into())),
                        Err(_) => report.add_code("palette_match", "error", "Unable to generate a comparison palette; no color match is claimed", &asset, None),
                    }
                }
            }
        } else {
            report.add_code(
                "asset_ownership",
                "warning",
                "Slate's btop asset is absent; a reference alone cannot load its colors",
                &asset,
                None,
            );
        }
    }
    report.add_code("reload", "info", "A running btop instance has not been inspected or reloaded", &path,
        Some("After syncing, reopen btop. An instance already running during a sync may save its old theme choice on exit; if it changes back, close btop, sync again, then reopen it.".into()));
}
