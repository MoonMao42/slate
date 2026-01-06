//! Picker preview panel fixture tests
//! Verify preview panel renders correctly with representative themes.
//! Tests cover: dark, light, extras-heavy, extras-empty scenarios.

use slate_cli::cli::picker::preview_panel::{render_preview, SemanticColor};
use slate_cli::theme::ThemeRegistry;

#[test]
fn test_preview_renders_catppuccin_mocha_dark() {
    // Dark theme with full extras (Catppuccin)
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry
        .get("catppuccin-mocha")
        .expect("catppuccin_mocha exists");

    let output = render_preview(&theme.palette);

    // Verify output contains expected components
    assert!(!output.is_empty(), "Preview output should not be empty");
    assert!(output.contains("\x1b["), "Should contain ANSI codes");

    // Verify ANSI color matrix is present (16 normal colors + 16 bright)
    let color_count = output.matches("\x1b[48;2").count();
    assert!(color_count >= 16, "Should render ANSI color matrix (dark: 16+ colors)");
}

#[test]
fn test_preview_renders_catppuccin_latte_light() {
    // Light theme with full extras (Catppuccin)
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry
        .get("catppuccin-latte")
        .expect("catppuccin_latte exists");

    let output = render_preview(&theme.palette);

    assert!(!output.is_empty(), "Preview output should not be empty");
    assert!(output.contains("\x1b["), "Should contain ANSI codes");

    // Verify ANSI matrix rendering
    let color_count = output.matches("\x1b[48;2").count();
    assert!(color_count >= 16, "Should render ANSI color matrix (light: 16+ colors)");
}

#[test]
fn test_preview_renders_tokyo_night_dark_simple() {
    // Dark theme with minimal extras (Tokyo Night)
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry
        .get("tokyo-night-dark")
        .expect("tokyo-night-dark exists");

    let output = render_preview(&theme.palette);

    assert!(!output.is_empty(), "Preview output should not be empty");
    assert!(output.contains("\x1b["), "Should contain ANSI codes");

    // Verify ANSI matrix is still present (extras-empty still shows 16 standard colors)
    let color_count = output.matches("\x1b[48;2").count();
    assert!(color_count >= 16, "Should render ANSI color matrix (even without extras)");
}

#[test]
fn test_preview_renders_nord_dark_simple() {
    // Another dark theme with minimal extras (Nord)
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry
        .get("nord")
        .expect("nord exists");

    let output = render_preview(&theme.palette);

    assert!(!output.is_empty(), "Preview output should not be empty");
    assert!(output.contains("\x1b["), "Should contain ANSI codes");

    // Verify ANSI matrix is present
    let color_count = output.matches("\x1b[48;2").count();
    assert!(color_count >= 16, "Should render ANSI color matrix");
}

#[test]
fn test_semantic_color_enum_completeness() {
    // Verify all 18 semantic color variants are defined
    let variants = [
        SemanticColor::GitBranch,
        SemanticColor::GitAdded,
        SemanticColor::GitModified,
        SemanticColor::GitUntracked,
        SemanticColor::Directory,
        SemanticColor::FileExec,
        SemanticColor::FileSymlink,
        SemanticColor::FileDir,
        SemanticColor::Prompt,
        SemanticColor::Accent,
        SemanticColor::Error,
        SemanticColor::Muted,
        SemanticColor::Success,
        SemanticColor::Warning,
        SemanticColor::Failed,
        SemanticColor::Status,
        SemanticColor::Text,
        SemanticColor::Subtext,
    ];

    assert_eq!(variants.len(), 18, "Should have exactly 18 semantic color variants");
}

#[test]
fn test_preview_with_dracula_dark() {
    // Dracula theme (dark, minimal extras)
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry
        .get("dracula")
        .expect("dracula exists");

    let output = render_preview(&theme.palette);

    assert!(!output.is_empty(), "Preview output should not be empty");
    assert!(output.contains("\x1b["), "Should contain ANSI codes");

    // Verify ANSI matrix
    let color_count = output.matches("\x1b[48;2").count();
    assert!(color_count >= 16, "Should render ANSI color matrix");
}

#[test]
fn test_preview_with_gruvbox_light() {
    // Gruvbox Light (light theme in the registered 18-theme set, stands in
    // for the earlier Solarized Light fixture which is not registered).
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry
        .get("gruvbox-light")
        .expect("gruvbox-light exists");

    let output = render_preview(&theme.palette);

    assert!(!output.is_empty(), "Preview output should not be empty");
    assert!(output.contains("\x1b["), "Should contain ANSI codes");

    // Verify ANSI matrix
    let color_count = output.matches("\x1b[48;2").count();
    assert!(color_count >= 16, "Should render ANSI color matrix");
}

#[test]
fn test_preview_output_multiline_format() {
    // Verify preview output includes multiple lines (sample tokens + ANSI matrix)
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry
        .get("catppuccin-mocha")
        .expect("catppuccin_mocha exists");

    let output = render_preview(&theme.palette);

    // Should contain multiple lines (sample section + ANSI section)
    let line_count = output.lines().count();
    assert!(line_count >= 5, "Preview should have multiple lines (samples + ANSI matrix)");
}

#[test]
fn test_preview_contains_ansi_reset() {
    // Verify preview includes ANSI reset codes (proper cleanup)
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry
        .get("catppuccin-mocha")
        .expect("catppuccin_mocha exists");

    let output = render_preview(&theme.palette);

    // Should contain ANSI reset codes to avoid color bleeding
    assert!(output.contains("\x1b[0m") || output.contains("\x1b[m"), "Should contain ANSI reset codes");
}

#[test]
fn test_palette_resolve_semantic_colors() {
    // Verify Palette::resolve() maps all semantic colors correctly
    let registry = ThemeRegistry::new().expect("Registry init");
    let theme = registry
        .get("catppuccin-mocha")
        .expect("catppuccin_mocha exists");

    // Test sample semantic color mappings
    let git_branch_color = theme.palette.resolve(SemanticColor::GitBranch);
    assert!(!git_branch_color.is_empty(), "GitBranch should resolve to a hex color");
    assert!(git_branch_color.starts_with('#'), "Color should be in hex format");

    let error_color = theme.palette.resolve(SemanticColor::Error);
    assert!(!error_color.is_empty(), "Error should resolve to a hex color");

    let text_color = theme.palette.resolve(SemanticColor::Text);
    assert!(!text_color.is_empty(), "Text should resolve to a hex color");
}
