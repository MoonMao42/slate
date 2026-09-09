//! File-only diagnostics; never run eza or source the caller's shell.
use super::{tool_files, Report};
use crate::{adapter::EzaAdapter, config::file_read::MAX_TOOL_CONFIG_BYTES, env::SlateEnv};

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    report.scope = "Read-only eza file and captured environment checks; no tool is launched or configuration changed. Only the saved theme (4 KiB) and managed palette (8 MiB) are read, separately rather than atomically. Personal YAML, shell startup, aliases, command-line options, color expressions and native appearance are not evaluated.";
    tool_files::availability(report, env, "eza");
    let theme = tool_files::saved_theme(report, env);
    let generated_overrides_match = theme.as_ref().is_some_and(|theme| {
        let (ls, eza) = crate::adapter::ls_colors::render_strings(&theme.palette);
        env.eza_colors_match(&eza, &ls)
    });
    let managed = EzaAdapter::theme_path(env);
    let palette = tool_files::read(
        report,
        env,
        &managed,
        MAX_TOOL_CONFIG_BYTES,
        "palette_file",
        "Slate eza palette",
    );
    if let Ok(Some(content)) = &palette {
        if let Some(theme) = theme {
            let same = content == &EzaAdapter::render_eza_yaml(&theme);
            report.add_code("palette_match", if same { "ok" } else { "warning" },
                if same { "Palette exactly matches the saved theme's generated colors; this does not prove it is loaded" }
                else { "Palette differs from current generated colors; an older schema, edits or formatting may explain the difference" }, &managed,
                (!same).then(|| "Review `slate tools sync eza --dry-run`. Sync changes only the managed palette, not shell startup.".into()));
        }
    }
    if env.session().is_isolated() {
        report.add_code(
            "isolated_profile",
            "info",
            "Isolated profile: host eza directory and color overrides are ignored",
            &managed,
            None,
        );
    }
    let selected = env.eza_config_home() == managed.parent().expect("managed asset has a parent");
    if selected && matches!(palette, Ok(None)) {
        report.add_code("selected_palette_missing", "error",
            "Captured directory selects Slate, but its managed theme.yml is absent; eza may use defaults or environment colors instead",
            &managed,
            Some("If a recognized theme is saved, review `slate tools sync eza --dry-run` to recreate the palette. Otherwise choose a theme first. This check does not create the file.".into()));
    }
    report.add_code("environment_selection", if selected { "info" } else { "warning" },
        if selected { "Captured directory names Slate's exact managed directory; file and override checks are separate" }
        else { "Captured directory does not name Slate's exact managed directory; custom paths and aliases are not resolved" }, &managed,
        (!selected).then(|| "Run this check from the shell used to launch eza. Keep intentional personal settings; review `slate setup` if Slate shell integration is missing, then open a fresh shell.".into()));
    report.add_code("color_overrides", if env.eza_color_overrides() && !generated_overrides_match { "warning" } else { "info" },
        if generated_overrides_match { "Captured color overrides exactly match Slate's saved-theme exports; this is not proof of live appearance. Values were not displayed" }
        else if env.eza_color_overrides() { "Captured color overrides could not be matched to the saved theme's generated exports; personal or stale values may override file colors. Values were not interpreted or displayed" }
        else { "No nonempty color override was captured; this alone does not prove effective colors" }, &managed, None);
    report.add_code("live_colors", "info", "Native rendering and the caller's shell startup remain unverified", &managed,
        Some("A new eza invocation reads its launch environment and theme file. Existing shell color exports may remain stale after palette-only sync; check from a fresh shell after updating integration.".into()));
}
