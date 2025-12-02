//! Tests for fastfetch JSONC generation
//! Per and Verify all 10 theme variants produce valid, consistent output

use slate_cli::adapter::fastfetch::FastfetchAdapter;
use slate_cli::theme::ThemeRegistry;

#[test]
fn test_generates_valid_json_for_all_10_themes() {
    let adapter = FastfetchAdapter;
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");

    let all_themes = registry.all();
    assert_eq!(all_themes.len(), 10, "Expected 10 theme variants");

    for theme in all_themes {
        let jsonc = adapter
            .generate_jsonc_config(theme)
            .expect(&format!("Failed to generate JSONC for theme {}", theme.id));

        // Verify it's valid JSON (can be parsed)
        let parsed: serde_json::Value =
            serde_json::from_str(&jsonc).expect(&format!("Invalid JSON for theme {}", theme.id));

        // Verify it's a valid object
        assert!(
            parsed.is_object(),
            "JSONC for theme {} should be a JSON object",
            theme.id
        );
    }
}

#[test]
fn test_apple_logo_preserved_in_all_themes() {
    let adapter = FastfetchAdapter;
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");

    for theme in registry.all() {
        let jsonc = adapter
            .generate_jsonc_config(theme)
            .expect(&format!("Failed to generate JSONC for theme {}", theme.id));

        let parsed: serde_json::Value =
            serde_json::from_str(&jsonc).expect(&format!("Failed to parse JSON for {}", theme.id));

        // Check logo configuration
        let logo = parsed
            .get("display")
            .and_then(|d| d.get("logo"))
            .expect(&format!("Missing logo in display for theme {}", theme.id));

        assert_eq!(
            logo.get("type").and_then(|v| v.as_str()),
            Some("builtin"),
            "Logo type should be 'builtin' for theme {}",
            theme.id
        );

        assert_eq!(
            logo.get("name").and_then(|v| v.as_str()),
            Some("apple"),
            "Logo name should be 'apple' for theme {}",
            theme.id
        );

        assert_eq!(
            logo.get("preserve").and_then(|v| v.as_bool()),
            Some(true),
            "Logo preserve should be true for theme {}",
            theme.id
        );
    }
}

#[test]
fn test_color_codes_are_ansi_24bit_format() {
    let adapter = FastfetchAdapter;
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");

    // Test with catppuccin_mocha as representative theme
    let theme = registry
        .get("catppuccin-mocha")
        .expect("catppuccin-mocha theme not found");

    let jsonc = adapter
        .generate_jsonc_config(theme)
        .expect("Failed to generate JSONC for catppuccin-mocha");

    let parsed: serde_json::Value =
        serde_json::from_str(&jsonc).expect("Failed to parse JSON for catppuccin-mocha");

    // Check color format in the color object
    let color_obj = parsed.get("color").expect("Missing color object in JSONC");

    // ANSI 24-bit format regex: 38;2;R;G;B where R,G,B are 0-255
    let ansi_24bit_regex =
        regex::Regex::new(r"^38;2;\d{1,3};\d{1,3};\d{1,3}$").expect("Failed to compile regex");

    for (key, value) in color_obj.as_object().expect("color should be an object") {
        let color_str = value
            .as_str()
            .expect(&format!("Color value for '{}' should be a string", key));

        assert!(
            ansi_24bit_regex.is_match(color_str),
            "Color for '{}' ({}) does not match ANSI 24-bit format '38;2;R;G;B'",
            key,
            color_str
        );
    }
}

#[test]
fn test_modules_array_contains_at_least_6_items() {
    let adapter = FastfetchAdapter;
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");

    for theme in registry.all() {
        let jsonc = adapter
            .generate_jsonc_config(theme)
            .expect(&format!("Failed to generate JSONC for theme {}", theme.id));

        let parsed: serde_json::Value =
            serde_json::from_str(&jsonc).expect(&format!("Failed to parse JSON for {}", theme.id));

        let modules = parsed
            .get("modules")
            .expect(&format!("Missing modules array for theme {}", theme.id))
            .as_array()
            .expect(&format!(
                "modules should be an array for theme {}",
                theme.id
            ));

        assert!(
            modules.len() >= 6 && modules.len() <= 8,
            "Theme {} has {} modules, expected 6-8",
            theme.id,
            modules.len()
        );

        // Verify each module has a type field
        for (i, module) in modules.iter().enumerate() {
            assert!(
                module.get("type").is_some(),
                "Module {} in theme {} missing 'type' field",
                i,
                theme.id
            );
        }
    }
}

#[test]
fn test_gruvbox_dark_legibility_sanity_check() {
    let adapter = FastfetchAdapter;
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");

    let theme = registry
        .get("gruvbox-dark")
        .expect("gruvbox-dark theme not found");

    let jsonc = adapter
        .generate_jsonc_config(theme)
        .expect("Failed to generate JSONC for gruvbox-dark");

    let parsed: serde_json::Value =
        serde_json::from_str(&jsonc).expect("Failed to parse JSON for gruvbox-dark");

    let color_obj = parsed.get("color").expect("Missing color object");

    let keys_color = color_obj
        .get("keys")
        .and_then(|v| v.as_str())
        .expect("Missing or invalid 'keys' color");

    let separator_color = color_obj
        .get("separator")
        .and_then(|v| v.as_str())
        .expect("Missing or invalid 'separator' color");

    // Sanity check: keys and separator colors should be valid
    // For gruvbox-dark, both should use foreground color
    // The real legibility check is manual visual verification
    assert!(
        !keys_color.is_empty() && !separator_color.is_empty(),
        "Gruvbox-dark colors should not be empty"
    );

    println!(
        "Gruvbox-dark keys color: {}, separator color: {}",
        keys_color, separator_color
    );
}

#[test]
fn test_output_is_pretty_printed() {
    let adapter = FastfetchAdapter;
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");

    let theme = registry
        .get("tokyo-night-dark")
        .expect("tokyo-night-dark theme not found");

    let jsonc = adapter
        .generate_jsonc_config(theme)
        .expect("Failed to generate JSONC for tokyo-night-dark");

    // Check for newlines (pretty-printed, not minified)
    assert!(
        jsonc.contains('\n'),
        "JSONC output should contain newlines (pretty-printed)"
    );

    // Check for indentation (spaces or tabs)
    assert!(
        jsonc.contains("  ") || jsonc.contains('\t'),
        "JSONC output should be indented"
    );

    // Check readability: should have reasonable line length
    let lines: Vec<&str> = jsonc.lines().collect();
    let avg_line_length = lines.iter().map(|l| l.len()).sum::<usize>() / lines.len();
    assert!(
        avg_line_length < 200,
        "Average line length {} suggests output is not pretty-printed",
        avg_line_length
    );
}

#[test]
fn test_schema_reference_present() {
    let adapter = FastfetchAdapter;
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");

    let theme = registry
        .get("catppuccin-latte")
        .expect("catppuccin-latte theme not found");

    let jsonc = adapter
        .generate_jsonc_config(theme)
        .expect("Failed to generate JSONC for catppuccin-latte");

    let parsed: serde_json::Value =
        serde_json::from_str(&jsonc).expect("Failed to parse JSON for catppuccin-latte");

    let schema = parsed
        .get("$schema")
        .and_then(|v| v.as_str())
        .expect("Missing $schema field in JSONC output");

    assert!(
        schema.contains("fastfetch"),
        "Schema reference should point to fastfetch documentation"
    );
}
