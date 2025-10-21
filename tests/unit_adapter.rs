use themectl::adapter::{BatAdapter, GhosttyAdapter, StarshipAdapter, ToolAdapter};
use themectl::theme::get_theme;

#[test]
fn test_ghostty_is_installed() {
    let adapter = GhosttyAdapter;
    let result = adapter.is_installed();
    assert!(result.is_ok());
    // May or may not be installed, but check doesn't fail
}

#[test]
fn test_ghostty_config_path_returns_path() {
    let adapter = GhosttyAdapter;
    let result = adapter.config_path();
    assert!(result.is_ok());
    let path = result.unwrap();
    assert!(path.to_string_lossy().contains("ghostty"));
}

#[test]
fn test_starship_is_installed() {
    let adapter = StarshipAdapter;
    let result = adapter.is_installed();
    assert!(result.is_ok());
}

#[test]
fn test_starship_config_path_returns_path() {
    let adapter = StarshipAdapter;
    let result = adapter.config_path();
    assert!(result.is_ok());
    let path = result.unwrap();
    assert!(path.to_string_lossy().contains("starship"));
    assert!(path.to_string_lossy().contains("starship.toml"));
}

#[test]
fn test_bat_is_installed() {
    let adapter = BatAdapter;
    let result = adapter.is_installed();
    assert!(result.is_ok());
}

#[test]
fn test_bat_config_path_returns_path() {
    let adapter = BatAdapter;
    let result = adapter.config_path();
    assert!(result.is_ok());
    let path = result.unwrap();
    assert!(path.to_string_lossy().contains("bat"));
}

#[test]
fn test_adapter_tool_names() {
    let ghostty = GhosttyAdapter;
    let starship = StarshipAdapter;
    let bat = BatAdapter;
    
    assert_eq!(ghostty.tool_name(), "ghostty");
    assert_eq!(starship.tool_name(), "starship");
    assert_eq!(bat.tool_name(), "bat");
}

#[test]
fn test_ghostty_with_catppuccin_theme() {
    let adapter = GhosttyAdapter;
    let theme = get_theme("catppuccin-mocha").expect("Theme not found");
    
    // Check that the adapter can identify the tool
    assert_eq!(adapter.tool_name(), "ghostty");
    
    // Check that the theme has a ghostty override
    assert!(theme.colors.tool_overrides.contains_key("ghostty"));
}

#[test]
fn test_starship_with_catppuccin_theme() {
    let adapter = StarshipAdapter;
    let theme = get_theme("catppuccin-mocha").expect("Theme not found");
    
    assert_eq!(adapter.tool_name(), "starship");
    assert!(theme.colors.tool_overrides.contains_key("starship"));
}

#[test]
fn test_bat_with_catppuccin_theme() {
    let adapter = BatAdapter;
    let theme = get_theme("catppuccin-mocha").expect("Theme not found");
    
    assert_eq!(adapter.tool_name(), "bat");
    assert!(theme.colors.tool_overrides.contains_key("bat"));
}

#[test]
fn test_all_theme_variants_have_all_adapters() {
    let themes = themectl::available_themes();
    let adapters = vec!["ghostty", "starship", "bat"];
    
    for theme_name in themes {
        let theme = get_theme(&theme_name).expect(&format!("Theme {} not found", theme_name));
        for adapter_name in &adapters {
            assert!(
                theme.colors.tool_overrides.contains_key(*adapter_name),
                "Theme {} missing {} adapter override",
                theme_name,
                adapter_name
            );
        }
    }
}
