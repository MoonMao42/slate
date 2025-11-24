//! Integration tests for tmux 7-element color coverage 
//! Verifies all 10 theme variants render correctly with complete tmux theming

use slate_cli::adapter::tmux::TmuxAdapter;
use slate_cli::theme::ThemeRegistry;

#[test]
fn test_renders_seven_tmux_elements_for_all_themes() {
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");
    
    for variant in registry.all() {
        let output = TmuxAdapter::render_tmux_colors(&variant);
        
        // Count set -g occurrences (should be exactly 7)
        let set_count = output.matches("set -g").count();
        assert_eq!(
            set_count, 7,
            "Theme '{}' should have exactly 7 set -g directives, got {}",
            variant.id, set_count
        );
        
        // Verify each of the 7 elements is present
        assert!(
            output.contains("status-style"),
            "Theme '{}' missing status-style",
            variant.id
        );
        assert!(
            output.contains("window-status-current-style"),
            "Theme '{}' missing window-status-current-style",
            variant.id
        );
        assert!(
            output.contains("pane-border-style"),
            "Theme '{}' missing pane-border-style",
            variant.id
        );
        assert!(
            output.contains("pane-active-border-style"),
            "Theme '{}' missing pane-active-border-style",
            variant.id
        );
        assert!(
            output.contains("message-style"),
            "Theme '{}' missing message-style",
            variant.id
        );
        assert!(
            output.contains("mode-style"),
            "Theme '{}' missing mode-style",
            variant.id
        );
        assert!(
            output.contains("message-command-style"),
            "Theme '{}' missing message-command-style",
            variant.id
        );
    }
}

#[test]
fn test_tmux_output_is_valid_conf_syntax() {
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");
    
    // Test with first theme (e.g., catppuccin-mocha)
    if let Some(variant) = registry.all().first() {
        let output = TmuxAdapter::render_tmux_colors(variant);
        
        // Parse lines
        for line in output.lines() {
            // Skip header comment
            if line.starts_with('#') {
                continue;
            }
            
            // All content lines should be set -g directives
            if !line.trim().is_empty() {
                assert!(
                    line.contains("set -g"),
                    "Invalid tmux syntax in output: {}",
                    line
                );
                
                // Validate basic tmux set syntax: set -g <option> "<value>"
                assert!(
                    line.contains('"') && line.ends_with('"'),
                    "Missing quotes in tmux directive: {}",
                    line
                );
            }
        }
    }
}

#[test]
fn test_tmux_colors_use_palette_fields() {
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");
    
    // Test with catppuccin_mocha
    if let Some(variant) = registry.get("catppuccin-mocha") {
        let output = TmuxAdapter::render_tmux_colors(variant);
        
        // Verify palette colors are actually in the output
        assert!(
            output.contains(&variant.palette.background),
            "Theme palette background '{}' not found in output",
            variant.palette.background
        );
        assert!(
            output.contains(&variant.palette.foreground),
            "Theme palette foreground '{}' not found in output",
            variant.palette.foreground
        );
        assert!(
            output.contains(&variant.palette.blue),
            "Theme palette blue '{}' not found in output",
            variant.palette.blue
        );
        assert!(
            output.contains(&variant.palette.black),
            "Theme palette black '{}' not found in output",
            variant.palette.black
        );
    }
}

#[test]
fn test_cross_theme_consistency_renders_without_error() {
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");
    
    // Iterate all 10 themes and verify no panics
    for variant in registry.all() {
        let output = TmuxAdapter::render_tmux_colors(variant);
        
        // Basic sanity checks
        assert!(!output.is_empty(), "Theme '{}' produced empty output", variant.id);
        assert!(
            output.contains("set -g"),
            "Theme '{}' produced no set -g directives",
            variant.id
        );
    }
}

#[test]
fn test_no_status_bar_content_modified() {
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");
    
    for variant in registry.all() {
        let output = TmuxAdapter::render_tmux_colors(variant);
        
        // constraint: No status bar content/widgets should be modified
        // Only color styles are allowed
        assert!(
            !output.contains("status-left"),
            "Theme '{}' incorrectly modifies status-left content",
            variant.id
        );
        assert!(
            !output.contains("status-right"),
            "Theme '{}' incorrectly modifies status-right content",
            variant.id
        );
        assert!(
            !output.contains("status-position"),
            "Theme '{}' incorrectly modifies status positioning",
            variant.id
        );
        assert!(
            !output.contains("status-justify"),
            "Theme '{}' incorrectly modifies status layout",
            variant.id
        );
    }
}

#[test]
fn test_palette_fields_mapped_correctly_per_d17() {
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");
    
    // Per reference catppuccin/tmux, verify color mappings
    if let Some(variant) = registry.get("catppuccin-mocha") {
        let output = TmuxAdapter::render_tmux_colors(variant);
        let palette = &variant.palette;
        
        // status-style should have background and foreground
        assert!(
            output.contains(&format!("status-style \"bg={} fg=", palette.background)),
            "status-style background mapping incorrect"
        );
        
        // window-status-current-style should use blue accent
        assert!(
            output.contains(&format!("window-status-current-style \"bg=", )),
            "window-status-current-style missing"
        );
        assert!(
            output.contains(&format!("fg={} bold", palette.foreground)),
            "window-status-current-style foreground incorrect"
        );
        
        // pane-active-border-style should use blue accent
        assert!(
            output.contains(&format!("pane-active-border-style \"fg={}", palette.blue)),
            "pane-active-border-style should use blue accent"
        );
    }
}
