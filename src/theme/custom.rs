//! User-defined custom theme loading from `~/.config/slate/themes/*.toml`.
//!
//! Custom themes allow users to drop a `.toml` palette file into their Slate
//! config directory without recompiling the binary. Embedded themes always take
//! precedence on ID collision so built-in invariants remain deterministic.

use super::{EmbeddedThemes, Palette, ThemeAppearance, ThemeVariant, ToolRefs, REQUIRED_TOOL_REFS};
use crate::adapter::palette_renderer::PaletteRenderer;
use crate::config::file_read::{self, Links, MAX_DOCUMENT_BYTES};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

const MAX_CUSTOM_THEME_FILES: usize = 128;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawCustomThemeDocument {
    Multi { theme: Vec<RawCustomTheme> },
    Single(Box<RawCustomTheme>),
}

#[derive(Debug, Deserialize)]
struct RawCustomTheme {
    id: String,
    name: String,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    appearance: Option<String>,
    #[serde(default)]
    auto_pair: Option<String>,
    #[serde(default)]
    tool_refs: ToolRefs,
    palette: RawCustomPalette,
}

#[derive(Debug, Deserialize)]
struct RawCustomPalette {
    foreground: String,
    background: String,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    selection_bg: Option<String>,
    #[serde(default)]
    selection_fg: Option<String>,
    #[serde(default)]
    brand_accent: Option<String>,

    black: String,
    red: String,
    green: String,
    yellow: String,
    blue: String,
    magenta: String,
    cyan: String,
    white: String,

    bright_black: String,
    bright_red: String,
    bright_green: String,
    bright_yellow: String,
    bright_blue: String,
    bright_magenta: String,
    bright_cyan: String,
    bright_white: String,

    #[serde(default)]
    bg_dim: Option<String>,
    #[serde(default)]
    bg_darker: Option<String>,
    #[serde(default)]
    bg_darkest: Option<String>,

    #[serde(default)]
    rosewater: Option<String>,
    #[serde(default)]
    flamingo: Option<String>,
    #[serde(default)]
    pink: Option<String>,
    #[serde(default)]
    mauve: Option<String>,
    #[serde(default)]
    lavender: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    subtext1: Option<String>,
    #[serde(default)]
    subtext0: Option<String>,
    #[serde(default)]
    overlay2: Option<String>,
    #[serde(default)]
    overlay1: Option<String>,
    #[serde(default)]
    overlay0: Option<String>,
    #[serde(default)]
    surface2: Option<String>,
    #[serde(default)]
    surface1: Option<String>,
    #[serde(default)]
    surface0: Option<String>,

    #[serde(default)]
    extras: HashMap<String, String>,
}

fn is_valid_custom_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id != "none"
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && !id.starts_with(['-', '_'])
        && !id.ends_with(['-', '_'])
}

fn is_safe_display_field(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.len() <= 80 && !trimmed.chars().any(char::is_control)
}

fn infer_appearance(background_hex: &str) -> ThemeAppearance {
    if let Ok((r, g, b)) = PaletteRenderer::hex_to_rgb(background_hex) {
        // Perceived luminance approximation in [0, 255]
        let luma = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
        if luma >= 140 {
            return ThemeAppearance::Light;
        }
    }
    ThemeAppearance::Dark
}

fn parse_appearance(raw: Option<&str>, background_hex: &str) -> Result<ThemeAppearance> {
    match raw.map(str::trim) {
        None | Some("") => Ok(infer_appearance(background_hex)),
        Some(value) if value.eq_ignore_ascii_case("dark") => Ok(ThemeAppearance::Dark),
        Some(value) if value.eq_ignore_ascii_case("light") => Ok(ThemeAppearance::Light),
        Some(other) => Err(SlateError::InvalidThemeData(format!(
            "invalid theme appearance '{other}': expected 'Dark' or 'Light'"
        ))),
    }
}

fn populate_default_tool_refs(id: &str, name: &str, tool_refs: &mut ToolRefs) {
    for &key in REQUIRED_TOOL_REFS {
        let needs_default = tool_refs
            .get(key)
            .is_none_or(|value| value.trim().is_empty());
        if needs_default {
            let default_ref = match key {
                "bat" | "delta" => format!("slate-{id}"),
                "ghostty" => name.to_string(),
                "alacritty" | "tmux" => id.replace('-', "_"),
                _ => id.to_string(),
            };
            tool_refs.insert(key.to_string(), default_ref);
        }
    }
}

/// Parse and validate one custom theme TOML string into `ThemeVariant` items.
pub fn parse_custom_themes_str(toml_str: &str) -> Result<Vec<ThemeVariant>> {
    let raw_doc: RawCustomThemeDocument = toml::from_str(toml_str).map_err(|err| {
        SlateError::InvalidThemeData(format!("failed to parse custom theme TOML: {err}"))
    })?;
    let raw_themes = match raw_doc {
        RawCustomThemeDocument::Multi { theme } => theme,
        RawCustomThemeDocument::Single(single) => vec![*single],
    };
    if raw_themes.is_empty() {
        return Err(SlateError::InvalidThemeData(
            "custom theme TOML did not contain any theme definitions".into(),
        ));
    }

    let mut out = Vec::with_capacity(raw_themes.len());
    for raw in raw_themes {
        let id = raw.id.trim().to_string();
        if !is_valid_custom_id(&id) {
            return Err(SlateError::InvalidThemeData(format!(
                "invalid custom theme id '{id}': use lowercase ASCII letters, digits, '-' or '_'"
            )));
        }
        let name = raw.name.trim().to_string();
        if !is_safe_display_field(&name) {
            return Err(SlateError::InvalidThemeData(format!(
                "invalid custom theme display name for '{id}'"
            )));
        }
        let family = raw
            .family
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Custom")
            .to_string();
        if !is_safe_display_field(&family) {
            return Err(SlateError::InvalidThemeData(format!(
                "invalid custom theme family for '{id}'"
            )));
        }

        let appearance = parse_appearance(raw.appearance.as_deref(), &raw.palette.background)?;
        let brand_accent = raw
            .palette
            .brand_accent
            .clone()
            .unwrap_or_else(|| raw.palette.blue.clone());

        let palette = Palette {
            foreground: raw.palette.foreground,
            background: raw.palette.background,
            cursor: raw.palette.cursor,
            selection_bg: raw.palette.selection_bg,
            selection_fg: raw.palette.selection_fg,
            brand_accent,
            black: raw.palette.black,
            red: raw.palette.red,
            green: raw.palette.green,
            yellow: raw.palette.yellow,
            blue: raw.palette.blue,
            magenta: raw.palette.magenta,
            cyan: raw.palette.cyan,
            white: raw.palette.white,
            bright_black: raw.palette.bright_black,
            bright_red: raw.palette.bright_red,
            bright_green: raw.palette.bright_green,
            bright_yellow: raw.palette.bright_yellow,
            bright_blue: raw.palette.bright_blue,
            bright_magenta: raw.palette.bright_magenta,
            bright_cyan: raw.palette.bright_cyan,
            bright_white: raw.palette.bright_white,
            bg_dim: raw.palette.bg_dim,
            bg_darker: raw.palette.bg_darker,
            bg_darkest: raw.palette.bg_darkest,
            rosewater: raw.palette.rosewater,
            flamingo: raw.palette.flamingo,
            pink: raw.palette.pink,
            mauve: raw.palette.mauve,
            lavender: raw.palette.lavender,
            text: raw.palette.text,
            subtext1: raw.palette.subtext1,
            subtext0: raw.palette.subtext0,
            overlay2: raw.palette.overlay2,
            overlay1: raw.palette.overlay1,
            overlay0: raw.palette.overlay0,
            surface2: raw.palette.surface2,
            surface1: raw.palette.surface1,
            surface0: raw.palette.surface0,
            extras: raw.palette.extras,
        };

        let mut tool_refs = raw.tool_refs;
        populate_default_tool_refs(&id, &name, &mut tool_refs);

        let variant = ThemeVariant {
            id,
            name,
            family,
            tool_refs,
            palette,
            appearance,
            auto_pair: raw
                .auto_pair
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        };
        variant.validate()?;
        out.push(variant);
    }

    Ok(out)
}

fn load_file(path: &Path) -> Result<Vec<ThemeVariant>> {
    let Some(source) = file_read::read(path, MAX_DOCUMENT_BYTES, Links::Reject)
        .map_err(|e| SlateError::InvalidThemeData(format!("cannot read custom theme file: {e}")))?
    else {
        return Ok(Vec::new());
    };
    let text = String::from_utf8(source.bytes)
        .map_err(|_| SlateError::InvalidThemeData("custom theme file is not valid UTF-8".into()))?;
    parse_custom_themes_str(&text)
}

/// Discover and load valid user themes from `<config_dir>/themes/*.toml`.
/// Non-existent directory, non-regular files, or invalid custom files are
/// ignored so a half-written user file never breaks built-in commands.
pub(super) fn load_custom_themes(env: &SlateEnv, embedded: &EmbeddedThemes) -> Vec<ThemeVariant> {
    let themes_dir = env.config_dir().join("themes");
    let Ok(meta) = fs::symlink_metadata(&themes_dir) else {
        return Vec::new();
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(&themes_dir) else {
        return Vec::new();
    };

    let mut paths = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.len() > MAX_CUSTOM_THEME_FILES {
        eprintln!(
            "Slate: found {} custom theme files in {}; loading the first {} in path order",
            paths.len(),
            themes_dir.display(),
            MAX_CUSTOM_THEME_FILES
        );
        paths.truncate(MAX_CUSTOM_THEME_FILES);
    }

    let mut seen_ids: HashSet<String> = embedded.all().map(|t| t.id.clone()).collect();
    let mut loaded = Vec::new();

    for path in paths {
        let Ok(variants) = load_file(&path) else {
            continue;
        };
        for variant in variants {
            if seen_ids.insert(variant.id.clone()) {
                loaded.push(variant);
            }
        }
    }

    // Clear any custom `auto_pair` that references an unknown ID or same-appearance variant.
    let all_appearance: HashMap<String, ThemeAppearance> = embedded
        .all()
        .map(|t| (t.id.clone(), t.appearance))
        .chain(loaded.iter().map(|t| (t.id.clone(), t.appearance)))
        .collect();
    for variant in &mut loaded {
        if let Some(pair_id) = variant.auto_pair.as_deref() {
            let valid = all_appearance
                .get(pair_id)
                .is_some_and(|&pair_app| pair_app != variant.appearance);
            if !valid {
                variant.auto_pair = None;
            }
        }
    }

    loaded
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const SAMPLE_CUSTOM_TOML: &str = r##"
id = "cyberpunk-neon"
name = "Cyberpunk Neon"
family = "Cyberpunk"

[palette]
foreground = "#e0e0ff"
background = "#0a0a18"
black = "#101020"
red = "#ff2a85"
green = "#00ff9f"
yellow = "#ffe600"
blue = "#00b8ff"
magenta = "#d600ff"
cyan = "#00fff9"
white = "#d0d0f0"
bright_black = "#404060"
bright_red = "#ff5ca1"
bright_green = "#5cffbe"
bright_yellow = "#ffef5c"
bright_blue = "#5cd1ff"
bright_magenta = "#e45cff"
bright_cyan = "#5cfffb"
bright_white = "#ffffff"
"##;

    #[test]
    fn custom_theme_parser_populates_defaults_and_infers_appearance() {
        let themes = parse_custom_themes_str(SAMPLE_CUSTOM_TOML).unwrap();
        assert_eq!(themes.len(), 1);
        let theme = &themes[0];
        assert_eq!(theme.id, "cyberpunk-neon");
        assert_eq!(theme.name, "Cyberpunk Neon");
        assert_eq!(theme.family, "Cyberpunk");
        assert_eq!(theme.appearance, ThemeAppearance::Dark);
        assert_eq!(theme.palette.brand_accent, "#00b8ff");
        assert_eq!(
            theme.tool_refs.get("ghostty").map(String::as_str),
            Some("Cyberpunk Neon")
        );
        assert_eq!(
            theme.tool_refs.get("alacritty").map(String::as_str),
            Some("cyberpunk_neon")
        );
    }

    #[test]
    fn custom_theme_directory_loads_valid_files_and_ignores_embedded_collisions() {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let themes_dir = env.config_dir().join("themes");
        fs::create_dir_all(&themes_dir).unwrap();
        fs::write(themes_dir.join("cyberpunk.toml"), SAMPLE_CUSTOM_TOML).unwrap();
        fs::write(themes_dir.join("broken.toml"), "not valid toml [").unwrap();
        let collision = SAMPLE_CUSTOM_TOML.replace("cyberpunk-neon", "nord");
        fs::write(themes_dir.join("collision.toml"), collision).unwrap();

        let registry = crate::theme::ThemeRegistry::with_env(&env).unwrap();
        assert!(registry.get("cyberpunk-neon").is_some());
        assert_eq!(
            registry.get("nord").unwrap().name,
            "Nord",
            "embedded theme must not be shadowed by custom file"
        );
    }

    #[test]
    fn unrelated_files_and_non_regular_toml_entries_do_not_use_theme_limit() {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let themes_dir = env.config_dir().join("themes");
        fs::create_dir_all(&themes_dir).unwrap();
        for index in 0..256 {
            fs::write(
                themes_dir.join(format!("unrelated-{index:03}.txt")),
                "ignored",
            )
            .unwrap();
        }
        fs::create_dir(themes_dir.join("not-a-file.toml")).unwrap();
        fs::write(themes_dir.join("custom.toml"), SAMPLE_CUSTOM_TOML).unwrap();

        let registry = crate::theme::ThemeRegistry::with_env(&env).unwrap();
        assert!(registry.get("cyberpunk-neon").is_some());
    }

    #[test]
    fn custom_theme_limit_selects_first_regular_toml_files_in_path_order() {
        let home = TempDir::new().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let themes_dir = env.config_dir().join("themes");
        fs::create_dir_all(&themes_dir).unwrap();
        for index in (0..=MAX_CUSTOM_THEME_FILES).rev() {
            let source =
                SAMPLE_CUSTOM_TOML.replace("cyberpunk-neon", &format!("custom-{index:03}"));
            fs::write(themes_dir.join(format!("{index:03}.toml")), source).unwrap();
        }

        let registry = crate::theme::ThemeRegistry::with_env(&env).unwrap();
        assert!(registry.get("custom-000").is_some());
        assert!(registry.get("custom-127").is_some());
        assert!(registry.get("custom-128").is_none());
    }
}
