use themectl::adapter::{BatAdapter, GhosttyAdapter, StarshipAdapter, ToolAdapter, ToolRegistry};
use themectl::config::backup::backup_directory;
use themectl::theme::get_theme;

#[test]
fn test_backup_directory_exists() {
    let result = backup_directory();
    assert!(result.is_ok());

    let backup_dir = result.unwrap();
    assert!(backup_dir.exists(), "Backup directory should exist");
    assert!(backup_dir.to_string_lossy().contains("themectl"));
}

#[test]
fn test_registry_detects_installed_tools() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(GhosttyAdapter));
    registry.register(Box::new(StarshipAdapter));
    registry.register(Box::new(BatAdapter));

    let result = registry.detect_installed();
    assert!(result.is_ok());
    // The result may contain 0-3 tools depending on what's installed
    let detected = result.unwrap();
    assert!(detected.len() <= 3);
}

#[test]
fn test_all_adapters_have_tool_names() {
    let ghostty = GhosttyAdapter;
    let starship = StarshipAdapter;
    let bat = BatAdapter;

    assert!(!ghostty.tool_name().is_empty());
    assert!(!starship.tool_name().is_empty());
    assert!(!bat.tool_name().is_empty());

    assert_eq!(ghostty.tool_name(), "ghostty");
    assert_eq!(starship.tool_name(), "starship");
    assert_eq!(bat.tool_name(), "bat");
}

#[test]
fn test_theme_has_required_overrides() {
    let theme = get_theme("catppuccin-mocha").expect("Theme not found");

    // Verify all three adapters have overrides
    assert!(theme.colors.tool_overrides.contains_key("ghostty"));
    assert!(theme.colors.tool_overrides.contains_key("starship"));
    assert!(theme.colors.tool_overrides.contains_key("bat"));

    // Verify overrides are not empty
    assert!(!theme.colors.tool_overrides["ghostty"].is_empty());
    assert!(!theme.colors.tool_overrides["starship"].is_empty());
    assert!(!theme.colors.tool_overrides["bat"].is_empty());
}

#[test]
fn test_backup_directory_nested() {
    let backup_dir = backup_directory().expect("Backup directory creation failed");

    // Should be in ~/.cache/themectl/backups
    let path_str = backup_dir.to_string_lossy();
    assert!(path_str.contains("themectl"));
    assert!(path_str.contains("backups"));
}

#[test]
fn test_all_adapters_have_config_paths() {
    let ghostty = GhosttyAdapter;
    let starship = StarshipAdapter;
    let bat = BatAdapter;

    let ghostty_path = ghostty.config_path().expect("Ghostty path failed");
    let starship_path = starship.config_path().expect("Starship path failed");
    let bat_path = bat.config_path().expect("Bat path failed");

    assert!(ghostty_path.to_string_lossy().contains("ghostty"));
    assert!(starship_path.to_string_lossy().contains("starship"));
    assert!(bat_path.to_string_lossy().contains("bat"));
}

#[test]
fn test_adapter_names_are_unique() {
    let ghostty = GhosttyAdapter;
    let starship = StarshipAdapter;
    let bat = BatAdapter;

    let names = vec![ghostty.tool_name(), starship.tool_name(), bat.tool_name()];

    // Check uniqueness
    assert_ne!(names[0], names[1]);
    assert_ne!(names[1], names[2]);
    assert_ne!(names[0], names[2]);
}
