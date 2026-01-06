use crate::adapter::palette_renderer::PaletteRenderer;
use crate::error::Result;
use crate::theme::{get_theme_description, ThemeRegistry, FAMILY_SORT_ORDER};
use std::collections::HashMap;

/// Handle `slate list` command
/// Displays themes grouped by family with descriptions and color blocks 
/// Families displayed in opinionated sort order (Catppuccin → Tokyo Night → Rosé Pine → Kanagawa → Everforest → Dracula → Nord → Gruvbox)
pub fn handle(_args: &[&str]) -> Result<()> {
    let registry = ThemeRegistry::new()?;

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
            // Family separator: ━━ {Family} ━━
            println!("{}━━ {} ━━", " ".repeat(2), family_name);

            for theme in themes {
                // Each line: 4 color blocks + theme-id + description
                print!("{}  ", " ".repeat(2)); // 4 spaces indent per 
                print_color_blocks(&theme.palette);
                print!("  {}", theme.id);

                // Description from get_theme_description
                if let Some(desc) = get_theme_description(&theme.id) {
                    print!("  {}", desc);
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

/// Print 4 color blocks (fg, bg, accent, error) inline
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

    #[test]
    fn test_handle_no_args() {
        let result = handle(&[]);
        assert!(result.is_ok());
    }
}
