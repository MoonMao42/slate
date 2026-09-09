use crate::adapter::palette_renderer::PaletteRenderer;
use crate::brand::render_context::{detect_render_mode, RenderContext, RenderMode};
use crate::brand::roles::Roles;
use crate::error::{Result, SlateError};
use crate::theme::{
    get_theme_description, ThemeAppearance, ThemeRegistry, ThemeVariant, FAMILY_SORT_ORDER,
};
use serde::Serialize;
use std::io::Write;

/// Options for the read-only theme catalog. The old `theme --list` alias
/// delegates to the defaults; search and machine output belong to `list`.
#[derive(clap::Args, Debug, Default)]
pub struct ListOptions {
    /// Search ID, display name or family (quote multiple words)
    pub query: Option<String>,
    /// Keep only dark or light themes
    #[arg(long, value_enum)]
    pub appearance: Option<AppearanceFilter>,
    /// Emit a versioned JSON catalog, without reading saved settings
    #[arg(long, conflicts_with = "ids")]
    pub json: bool,
    /// Print only canonical theme IDs, one per line
    #[arg(long, conflicts_with = "json")]
    pub ids: bool,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AppearanceFilter {
    Dark,
    Light,
}

impl AppearanceFilter {
    fn matches(self, appearance: ThemeAppearance) -> bool {
        matches!(
            (self, appearance),
            (Self::Dark, ThemeAppearance::Dark) | (Self::Light, ThemeAppearance::Light)
        )
    }
}

fn appearance_name(appearance: ThemeAppearance) -> &'static str {
    match appearance {
        ThemeAppearance::Dark => "dark",
        ThemeAppearance::Light => "light",
    }
}

#[derive(Serialize)]
struct Catalog<'a> {
    schema_version: u8,
    query: Option<&'a str>,
    appearance: Option<AppearanceFilter>,
    count: usize,
    themes: Vec<CatalogTheme<'a>>,
}

#[derive(Serialize)]
struct CatalogTheme<'a> {
    id: &'a str,
    name: &'a str,
    family: &'a str,
    appearance: &'static str,
    description: Option<&'static str>,
    auto_pair: Option<&'a str>,
}

fn catalog<'a>(themes: &[&'a ThemeVariant], options: &'a ListOptions) -> Catalog<'a> {
    Catalog {
        schema_version: 1,
        query: options.query.as_deref(),
        appearance: options.appearance,
        count: themes.len(),
        themes: themes
            .iter()
            .map(|theme| CatalogTheme {
                id: &theme.id,
                name: &theme.name,
                family: &theme.family,
                appearance: appearance_name(theme.appearance),
                description: get_theme_description(&theme.id),
                auto_pair: theme.auto_pair.as_deref(),
            })
            .collect(),
    }
}

/// Preserve the original no-argument handler for the compatibility alias.
pub fn handle(_args: &[&str]) -> Result<()> {
    handle_with_options(&ListOptions::default())
}

pub fn handle_with_options(options: &ListOptions) -> Result<()> {
    // Also validate direct library calls that bypass clap.
    if options.json && options.ids {
        return Err(SlateError::InvalidConfig(
            "`--json` and `--ids` cannot be used together".into(),
        ));
    }
    let registry = ThemeRegistry::new()?;
    let mut themes = registry.search(options.query.as_deref().unwrap_or_default());
    themes.retain(|theme| {
        options
            .appearance
            .is_none_or(|filter| filter.matches(theme.appearance))
    });
    // Keep the established family order and embedded variant order. Unknown
    // future families remain visible (sorted by name), not silently omitted.
    themes.sort_by_key(|theme| {
        (
            FAMILY_SORT_ORDER
                .iter()
                .position(|family| *family == theme.family)
                .unwrap_or(usize::MAX),
            theme.family.as_str(),
        )
    });

    let output = if options.json {
        format!(
            "{}\n",
            serde_json::to_string_pretty(&catalog(&themes, options))?
        )
    } else if options.ids {
        themes
            .iter()
            .map(|theme| format!("{}\n", theme.id))
            .collect()
    } else {
        // No saved-config read is needed for pipes, NO_COLOR or TERM=dumb.
        let ctx = (detect_render_mode() != RenderMode::None)
            .then(|| RenderContext::from_active_theme().ok())
            .flatten();
        render_text(&themes, ctx.as_ref())
    };

    match std::io::stdout().lock().write_all(output.as_bytes()) {
        Ok(()) => Ok(()),
        // Normal shell consumers may stop after the first few IDs/rows.
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn render_text(themes: &[&ThemeVariant], ctx: Option<&RenderContext<'_>>) -> String {
    if themes.is_empty() {
        return "No matching themes. Run `slate list` to see all themes, or broaden the search/filter.\n".into();
    }
    let roles = ctx.map(Roles::new);
    let mode = ctx.map_or(RenderMode::None, |ctx| ctx.mode);
    let mut output = String::from("\n");
    let mut previous_family = None;
    for theme in themes {
        if previous_family != Some(theme.family.as_str()) {
            if previous_family.is_some() {
                output.push('\n');
            }
            output.push_str(&format!(
                "  {}\n",
                heading_text(roles.as_ref(), &theme.family)
            ));
            previous_family = Some(theme.family.as_str());
        }
        output.push_str("    ");
        if mode == RenderMode::Truecolor {
            output.push_str(&color_blocks(&theme.palette));
            output.push_str("  ");
        }
        output.push_str(&format!(
            "{}  {}  [{}]",
            theme_name_text(roles.as_ref(), &theme.name),
            path_text(roles.as_ref(), &theme.id),
            appearance_name(theme.appearance),
        ));
        if let Some(description) = get_theme_description(&theme.id) {
            output.push_str(&format!("  {}", path_text(roles.as_ref(), description)));
        }
        output.push('\n');
    }
    output.push('\n');
    output
}

/// Render `◆ title` via `Roles::heading`, falling back to plain ◆ text
/// when Roles is unavailable (graceful degrade).
fn heading_text(r: Option<&Roles<'_>>, title: &str) -> String {
    match r {
        Some(r) => r.heading(title),
        None => format!("◆ {}", title),
    }
}

/// Render a theme display name through `Roles::theme_name` (active
/// theme's `brand_accent` per daily chrome).
fn theme_name_text(r: Option<&Roles<'_>>, name: &str) -> String {
    match r {
        Some(r) => r.theme_name(name),
        None => name.to_string(),
    }
}

/// Render a description / metadata blurb through `Roles::path` (dim +
/// italic per Sketch 002), falling back to bare text when Roles is
/// unavailable.
fn path_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.path(text),
        None => text.to_string(),
    }
}

// Exact palette swatches are emitted only in truecolor mode.
// SWATCH-RENDERER: these blocks display the selected palette's exact RGB values.
fn color_blocks(palette: &crate::theme::Palette) -> String {
    let colors = [
        &palette.foreground,
        &palette.background,
        &palette.blue,
        &palette.red,
    ];

    let mut output = String::new();
    for hex in colors {
        if let Ok((r, g, b)) = PaletteRenderer::hex_to_rgb(hex) {
            output.push_str(&format!("\x1b[38;2;{};{};{}m████\x1b[0m", r, g, b));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

    #[test]
    fn test_handle_no_args() {
        let result = handle(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_text_respects_render_mode_and_labels_appearance() {
        let theme = mock_theme();
        for mode in [RenderMode::None, RenderMode::Basic, RenderMode::Truecolor] {
            let ctx = mock_context_with_mode(&theme, mode);
            let output = render_text(&[&theme], Some(&ctx));
            assert!(output.contains("[dark]"));
            assert_eq!(output.contains("████"), mode == RenderMode::Truecolor);
            if mode == RenderMode::None {
                assert!(!output.contains('\x1b'));
            }
        }
        assert!(!render_text(&[&theme], None).contains('\x1b'));
        assert!(render_text(&[], None).contains("No matching themes"));
    }

    /// chrome contract — family heading carries brand-lavender
    /// bytes in truecolor (Sketch 002 anchor for grouped listings).
    #[test]
    fn list_family_heading_carries_brand_lavender_in_truecolor() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let line = heading_text(Some(&r), "Catppuccin");
        assert!(
            line.contains("38;2;114;135;253"),
            "family heading must carry brand-lavender bytes in truecolor, got: {line:?}"
        );
    }

    /// daily chrome — `theme_name_text` carries the mock theme's
    /// `brand_accent` byte triple (`#7287fd` → 114;135;253; same value
    /// as the brand anchor for the mock fixture, but the call routes
    /// through `Roles::theme_name`, which is the daily-chrome path).
    #[test]
    fn list_theme_name_uses_brand_accent_in_truecolor() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let name = theme_name_text(Some(&r), "catppuccin-mocha");
        assert!(
            name.contains("38;2;114;135;253"),
            "theme name must carry brand-accent bytes in truecolor, got: {name:?}"
        );
    }

    /// graceful degrade — every chrome helper falls back to plain
    /// text when Roles is unavailable; zero ANSI bytes leak through.
    #[test]
    fn list_chrome_helpers_fall_back_to_plain_when_roles_absent() {
        let heading = heading_text(None, "Catppuccin");
        let name = theme_name_text(None, "catppuccin-mocha");
        let desc = path_text(None, "Smooth pastel theme");
        assert_eq!(heading, "◆ Catppuccin");
        assert_eq!(name, "catppuccin-mocha");
        assert_eq!(desc, "Smooth pastel theme");
        for s in [heading, name, desc] {
            assert!(!s.contains('\x1b'));
        }
    }
}
