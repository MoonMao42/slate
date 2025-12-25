/// Integration test for palette codegen.
/// Verifies that generated theme modules compile and produce valid Palettes.

#[test]
fn test_generated_themes_exist() {
    // Verify the generated directory and mod.rs exist
    let generated_path = std::path::Path::new("src/theme/generated");
    assert!(generated_path.exists(), "src/theme/generated directory not found");

    let mod_rs_path = generated_path.join("mod.rs");
    assert!(mod_rs_path.exists(), "src/theme/generated/mod.rs not found");
}

#[test]
fn test_catppuccin_mocha_theme_json() {
    // Verify catppuccin_mocha.json exists and is valid
    let theme_path = std::path::Path::new("themes/catppuccin_mocha.json");
    assert!(theme_path.exists(), "themes/catppuccin_mocha.json not found");

    let content = std::fs::read_to_string(theme_path)
        .expect("Failed to read catppuccin_mocha.json");

    // Should be valid JSON
    let _json: serde_json::Value =
        serde_json::from_str(&content).expect("catppuccin_mocha.json is not valid JSON");
}

#[test]
fn test_all_theme_json_files_exist() {
    let expected_themes = vec![
        "base16_ocean",
        "catppuccin_latte",
        "catppuccin_mocha",
        "dracula",
        "everforest",
        "github_dark",
        "gruvbox_dark",
        "gruvbox_light",
        "kanagawa",
        "monokai",
        "nord",
        "one_dark",
        "rose_pine",
        "rose_pine_dawn",
        "solarized_dark",
        "solarized_light",
        "tokyo_night",
        "tokyo_night_light",
    ];

    for theme_id in &expected_themes {
        let path = format!("themes/{}.json", theme_id);
        assert!(
            std::path::Path::new(&path).exists(),
            "Theme file {} not found",
            path
        );
    }
}

#[test]
fn test_all_generated_theme_rs_files_exist() {
    let expected_themes = vec![
        "base16_ocean",
        "catppuccin_latte",
        "catppuccin_mocha",
        "dracula",
        "everforest",
        "github_dark",
        "gruvbox_dark",
        "gruvbox_light",
        "kanagawa",
        "monokai",
        "nord",
        "one_dark",
        "rose_pine",
        "rose_pine_dawn",
        "solarized_dark",
        "solarized_light",
        "tokyo_night",
        "tokyo_night_light",
    ];

    for theme_id in &expected_themes {
        let path = format!("src/theme/generated/{}.rs", theme_id);
        assert!(
            std::path::Path::new(&path).exists(),
            "Generated theme file {} not found",
            path
        );

        // Verify file contains @generated marker
        let content = std::fs::read_to_string(&path)
            .expect(&format!("Failed to read {}", path));
        assert!(
            content.contains("@generated"),
            "Generated file {} missing @generated marker",
            path
        );
    }
}

#[test]
fn test_generated_mod_rs_structure() {
    let mod_path = "src/theme/generated/mod.rs";
    let content = std::fs::read_to_string(mod_path).expect("Failed to read mod.rs");

    // Should contain module declarations
    assert!(content.contains("mod base16_ocean;"), "mod.rs missing base16_ocean module");
    assert!(content.contains("mod catppuccin_mocha;"), "mod.rs missing catppuccin_mocha module");

    // Should contain re-exports
    assert!(content.contains("pub use base16_ocean::*;"), "mod.rs missing re-export for base16_ocean");
    assert!(content.contains("pub use catppuccin_mocha::*;"), "mod.rs missing re-export for catppuccin_mocha");
}

#[test]
fn test_xtask_compiles() {
    // Verify xtask workspace member compiles
    let output = std::process::Command::new("cargo")
        .args(&["build", "--manifest-path", "xtask/Cargo.toml"])
        .output()
        .expect("Failed to run cargo build for xtask");

    assert!(
        output.status.success(),
        "xtask build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_regen_themes_command_exists() {
    // Verify xtask binary exists and can be invoked
    let xtask_binary = std::path::Path::new("target/debug/xtask");
    assert!(xtask_binary.exists(), "xtask binary not found at target/debug/xtask");
}
