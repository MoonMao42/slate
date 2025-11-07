use slate_cli::theme::ThemeRegistry;

#[test]
fn test_all_8_themes_load() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");
    let all_themes = registry.all();

    assert_eq!(all_themes.len(), 8, "Expected 8 theme variants");
}

#[test]
fn test_theme_ids_correct() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");
    let ids = registry.list_ids();

    assert!(ids.contains(&"catppuccin-latte".to_string()));
    assert!(ids.contains(&"catppuccin-frappe".to_string()));
    assert!(ids.contains(&"catppuccin-macchiato".to_string()));
    assert!(ids.contains(&"catppuccin-mocha".to_string()));
    assert!(ids.contains(&"tokyo-night-light".to_string()));
    assert!(ids.contains(&"tokyo-night-dark".to_string()));
    assert!(ids.contains(&"dracula".to_string()));
    assert!(ids.contains(&"nord".to_string()));
}

#[test]
fn test_catppuccin_family_has_4_variants() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");
    let families = registry.by_family();

    let catppuccin = families.get("Catppuccin").expect("Catppuccin family not found");
    assert_eq!(catppuccin.len(), 4);
}

#[test]
fn test_tokyo_night_family_has_2_variants() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");
    let families = registry.by_family();

    let tokyo_night = families.get("Tokyo Night").expect("Tokyo Night family not found");
    assert_eq!(tokyo_night.len(), 2);
}

#[test]
fn test_theme_validation() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    for theme in registry.all() {
        let result = theme.validate();
        assert!(result.is_ok(), "Theme {} failed validation: {:?}", theme.id, result);
    }
}

#[test]
fn test_theme_tool_refs_consistency() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    for theme in registry.all() {
        // Verify each tool_ref is non-empty
        assert!(!theme.tool_refs.ghostty.is_empty(), "Ghostty ref empty for {}", theme.id);
        assert!(!theme.tool_refs.alacritty.is_empty(), "Alacritty ref empty for {}", theme.id);
        assert!(!theme.tool_refs.bat.is_empty(), "bat ref empty for {}", theme.id);

        // Verify tool_refs.get() works for all tools
        assert!(theme.tool_refs.get("ghostty").is_some());
        assert!(theme.tool_refs.get("bat").is_some());
    }
}

#[test]
fn test_palette_has_required_colors() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    for theme in registry.all() {
        let palette = &theme.palette;

        // All themes must have primary colors
        assert!(!palette.foreground.is_empty(), "Foreground empty for {}", theme.id);
        assert!(!palette.background.is_empty(), "Background empty for {}", theme.id);
        assert!(!palette.red.is_empty(), "Red empty for {}", theme.id);
        assert!(!palette.green.is_empty(), "Green empty for {}", theme.id);
        assert!(!palette.blue.is_empty(), "Blue empty for {}", theme.id);

        // Verify colors are hex format (basic check)
        assert!(palette.foreground.starts_with('#'), "Foreground not hex for {}", theme.id);
    }
}

#[test]
fn test_theme_registry_get_by_id() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    let mocha = registry.get("catppuccin-mocha");
    assert!(mocha.is_some());
    assert_eq!(mocha.unwrap().name, "Catppuccin Mocha");

    let unknown = registry.get("nonexistent");
    assert!(unknown.is_none());
}

#[test]
fn test_catppuccin_specific_colors() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    let mocha = registry.get("catppuccin-mocha").expect("Catppuccin Mocha not found");
    
    // Catppuccin should have specific colors
    assert!(mocha.palette.rosewater.is_some());
    assert!(mocha.palette.flamingo.is_some());
    assert!(mocha.palette.mauve.is_some());
}
