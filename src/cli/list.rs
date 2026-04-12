use crate::adapter::palette_renderer::PaletteRenderer;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::error::Result;
use crate::theme::{get_theme_description, ThemeRegistry, FAMILY_SORT_ORDER};
use std::collections::HashMap;

/// Handle `slate list` command
/// Displays themes grouped by family with descriptions and color blocks
/// Families displayed in opinionated sort order (Catppuccin → Tokyo Night → Rosé Pine → Kanagawa → Everforest → Dracula → Nord → Gruvbox)
/// migration: family headings + theme name + description route
/// through the Roles API; the per-row 4-color palette swatch stays raw
/// (the swatch IS the visual contract — see `print_color_blocks`).
pub fn handle(_args: &[&str]) -> Result<()> {
    let registry = ThemeRegistry::new()?;

    // Bootstrap Roles up-front so every family heading + row shares the
    // same byte contract (sketch 003 daily chrome). Graceful
    // degrade per — plain text when the registry fails to load.
    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);

    // Blank line above
    println!();

    // Group themes by family
    let mut families: HashMap<String, Vec<&crate::theme::ThemeVariant>> = HashMap::new();

    for theme in registry.all() {
        families
            .entry(theme.family.clone())
            .or_default()
            .push(theme);
    }

    // Render families in static sort order
    for family_name in FAMILY_SORT_ORDER {
        if let Some(themes) = families.get(*family_name) {
            // Family heading: brand-anchor lavender ◆ + family name (sketch 003)
            println!("  {}", heading_text(r.as_ref(), family_name));

            for theme in themes {
                // Each line: 4 color blocks + display name + id + description.
                // The display name renders through `r.theme_name(...)` so it
                // carries the active theme's `brand_accent` ; the id and
                // description route through `r.path(...)` for the dim italic
                // treatment.
                print!("{}  ", " ".repeat(2)); // 4 spaces indent per 
                print_color_blocks(&theme.palette);
                print!("  {}", theme_name_text(r.as_ref(), &theme.name));
                print!("  {}", path_text(r.as_ref(), &theme.id));

                // Description from get_theme_description
                if let Some(desc) = get_theme_description(&theme.id) {
                    print!("  {}", path_text(r.as_ref(), desc));
                }

                println!();
            }

            println!(); // Blank line between families
        }
    }

    // Blank line below
    println!();

    Ok(())
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

// SWATCH-RENDERER: per-row 4-color palette swatch (fg / bg / blue / red).
// The bytes ARE the palette preview — `\x1b[38;2;R;G;B;m████\x1b[0m` is
// the visual contract every list row depends on. Migrating chrome text
// (theme name, description, family heading) was the goal of
// this helper stays raw by design.
fn print_color_blocks(palette: &crate::theme::Palette) {
    let colors = vec![
        &palette.foreground,
        &palette.background,
        &palette.blue,
        &palette.red,
    ];

    for hex in colors {
        if let Ok((r, g, b)) = PaletteRenderer::hex_to_rgb(hex) {
            print!("\x1b[38;2;{};{};{}m████\x1b[0m", r, g, b);
        }
    }
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
