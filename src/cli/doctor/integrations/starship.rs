use super::{tool_files, Report};
use crate::{
    adapter::{starship::themed_config_from_content, StarshipAdapter},
    config::{
        file_read::{self, MAX_DOCUMENT_BYTES, MAX_TOOL_CONFIG_BYTES},
        prompt::{self, PromptStyle},
    },
    env::SlateEnv,
    theme::ThemeVariant,
};
use std::path::Path;

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    report.scope = "Read-only Starship file checks using the captured invocation environment; no tool is launched or configuration changed. Shell startup, custom commands, native compatibility, fonts and the running prompt are not evaluated. Files are observed separately, not atomically; palette and preset-field matches do not prove live appearance.";
    tool_files::availability(report, env, "starship");
    let theme = tool_files::saved_theme(report, env);
    let style = preferences(report, env);
    let primary = StarshipAdapter::integration_config_path_with_env(env);
    let fallback = env.managed_file("managed/starship/plain.toml");
    let path = env.starship_config_override().unwrap_or(&primary);
    report.add_code(
        "selected_config",
        "info",
        if env.starship_config_override().is_some() {
            "STARSHIP_CONFIG selects this path for this invocation"
        } else {
            "No captured STARSHIP_CONFIG override; inspecting the standard Starship configuration"
        },
        path,
        None,
    );
    report.add_code("shell_startup", "info", "Shell initialization and the currently rendered prompt have not been verified", path,
        Some("If the prompt does not change, review `slate doctor zsh`, `slate doctor bash` or `slate doctor fish` for your shell, then open a fresh shell after applying changes. This check does not source startup files.".into()));
    if env.session().is_isolated() {
        report.add_code(
            "isolated_profile",
            "info",
            "Isolated profile: host STARSHIP_CONFIG is ignored",
            path,
            None,
        );
    }
    if !path.is_absolute() {
        report.add_code("relative_override", "warning", "Relative STARSHIP_CONFIG is not resolved or read; no effective file match is claimed", path,
            Some("Review the override locally. An absolute path makes the selected file unambiguous across working directories.".into()));
        return;
    }
    let same_path = |other: &Path| {
        path == other
            || file_read::directory_alias_target(path)
                .zip(file_read::directory_alias_target(other))
                .is_some_and(|(left, right)| left == right)
    };
    let plain = same_path(&fallback);
    if plain {
        report.add_code("plain_fallback", "info", "The captured environment selects Slate's plain fallback profile", path,
            Some("Rainbow uses a plain layout here; the other presets retain their selected layouts. Font/terminal selection can explain why this differs from the standard profile.".into()));
    } else if !same_path(&primary) {
        report.add_code("custom_override", "warning", "A custom override selects a file outside Slate's two prompt write targets", path,
            Some("`slate prompt` edits standard starship.toml and Slate's plain fallback, not this override. Keep it if intentional, or review your STARSHIP_CONFIG before expecting a preset change to appear.".into()));
    }
    let content = match tool_files::read(
        report,
        env,
        path,
        MAX_TOOL_CONFIG_BYTES,
        "config_file",
        "Selected Starship configuration",
    ) {
        Ok(Some(content)) => content,
        Ok(None) => {
            report.add_code("prompt_connection", "warning", "The selected file is absent; no Slate palette or layout is confirmed", path,
                Some("Review the selected path first. `slate prompt` can create Slate's standard profiles but does not install or enable Starship.".into()));
            return;
        }
        Err(()) => return,
    };
    let Some(doc) = tool_files::parse_toml(report, path, &content, "config_syntax") else {
        return;
    };
    let selected = doc.get("palette").and_then(toml::Value::as_str) == Some("slate");
    report.add_code(
        "palette_selection",
        if selected { "ok" } else { "warning" },
        if selected {
            "This file selects the slate palette"
        } else {
            "This file does not select the slate palette; a saved theme alone will not activate it"
        },
        path,
        None,
    );
    if let Some(theme) = theme {
        check_palette(report, path, &doc, &theme);
    }
    if let Some(style) = style {
        match prompt::matches_layout(&content, style, plain) {
            Ok(matches) => report.add_code("layout_match", if matches { "ok" } else { "warning" },
                if matches { "Presentation fields match the saved preset for this profile; custom modules and live output are not evaluated" }
                else { "Presentation fields differ from the saved preset; this may be an intentional personal layout" }, path,
                (!matches).then(|| "Keep personal edits if intentional. `slate prompt --list` lists layouts; review a preset with `slate prompt <style> --dry-run` before replacing presentation fields.".into())),
            Err(_) => report.add_code("layout_match", "error", "Cannot compare preset fields; review malformed module tables locally (file contents omitted)", path, None),
        }
    }
}

fn preferences(report: &mut Report, env: &SlateEnv) -> Option<PromptStyle> {
    let path = env.managed_file("config.toml");
    let content = tool_files::read(
        report,
        env,
        &path,
        MAX_DOCUMENT_BYTES,
        "settings_file",
        "Slate preferences",
    )
    .ok()?;
    let doc = tool_files::parse_toml(
        report,
        &path,
        content.as_deref().unwrap_or(""),
        "settings_syntax",
    )?;
    let flag = match doc.get("tools") {
        None => Some(true),
        Some(tools) => tools
            .as_table()
            .and_then(|table| match table.get("starship") {
                None => Some(true),
                Some(flag) => flag.as_bool(),
            }),
    };
    match flag {
        Some(enabled) => report.add_code("activation_preference", if enabled { "info" } else { "warning" },
            if enabled { "Slate permits Starship initialization; this does not prove a startup hook is installed" }
            else { "Starship initialization is disabled in Slate preferences; generated prompt files alone do not enable it" }, &path,
            (!enabled).then(|| "If desired, use `slate` → Shell Preferences to enable Starship; review shell startup wiring separately.".into())),
        None => report.add_code("activation_preference", "error", "Cannot interpret [tools].starship as a boolean; no default is assumed", &path, None),
    }
    let style = match doc.get("prompt") {
        None => Ok(None),
        Some(section) => section.as_table().ok_or(()).and_then(|table| {
            table
                .get("style")
                .map(|value| {
                    value
                        .as_str()
                        .and_then(|id| PromptStyle::ALL.into_iter().find(|s| s.id() == id))
                        .ok_or(())
                })
                .transpose()
        }),
    };
    match style {
        Ok(Some(style)) => {
            report.add_code(
                "saved_layout",
                "info",
                format!(
                    "Saved preset: {} (intent, not proof of the active layout)",
                    style.label()
                ),
                &path,
                None,
            );
            Some(style)
        }
        Ok(None) => {
            report.add_code(
                "saved_layout",
                "info",
                "No preset is saved; a personal layout is allowed and no preset match is assumed",
                &path,
                None,
            );
            None
        }
        Err(()) => {
            report.add_code(
                "saved_layout",
                "error",
                format!(
                    "Invalid saved prompt style; expected {} (file contents omitted)",
                    PromptStyle::ALL.map(PromptStyle::id).join(", ")
                ),
                &path,
                None,
            );
            None
        }
    }
}

fn check_palette(report: &mut Report, path: &Path, doc: &toml::Value, theme: &ThemeVariant) {
    let expected = themed_config_from_content("", theme)
        .ok()
        .and_then(|s| s.parse::<toml::Value>().ok());
    let palette = |value: &toml::Value| value.get("palettes")?.get("slate")?.as_table().cloned();
    let Some(expected) = expected.as_ref().and_then(palette) else {
        report.add_code(
            "palette_match",
            "error",
            "Unable to generate a comparison palette; no color match is claimed",
            path,
            None,
        );
        return;
    };
    let actual = palette(doc);
    let matches = actual.is_some_and(|actual| {
        expected.iter().all(|(key, color)| {
            actual
                .get(key)
                .and_then(toml::Value::as_str)
                .zip(color.as_str())
                .is_some_and(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
        })
    });
    report.add_code("palette_match", if matches { "ok" } else { "warning" },
        if matches { "Slate palette entries match the saved theme; extra personal palette entries are allowed" }
        else { "Slate palette entries are absent, invalid or differ from the saved theme" }, path,
        (!matches).then(|| "For Slate's standard profile, review `slate tools sync starship --dry-run`. Custom overrides are not edited by that command; no live prompt match is claimed.".into()));
}
