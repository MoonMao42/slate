//! Tests for fastfetch JSONC generation
//! Per and Verify all theme variants produce valid, consistent output

use slate_cli::adapter::fastfetch::FastfetchAdapter;
use slate_cli::theme::ThemeRegistry;

#[test]
fn test_generates_valid_json_for_all_themes() {
    let adapter = FastfetchAdapter;
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");

    let all_themes = registry.all();
    assert_eq!(all_themes.len(), 18, "Expected 18 theme variants");

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

        // Per fastfetch 2.x schema: `logo` is a top-level field with `source`
        // (not `display.logo.name` — that is the old schema that fastfetch
        // silently rejected with `JsonConfig Error: Unknown display property`).
        let logo = parsed
            .get("logo")
            .expect(&format!("Missing top-level logo for theme {}", theme.id));

        assert_eq!(
            logo.get("type").and_then(|v| v.as_str()),
            Some("builtin"),
            "Logo type should be 'builtin' for theme {}",
            theme.id
        );

        assert_eq!(
            logo.get("source").and_then(|v| v.as_str()),
            Some("apple"),
            "Logo source should be 'apple' for theme {}",
            theme.id
        );

        assert_eq!(
            logo.get("preserveAspectRatio").and_then(|v| v.as_bool()),
            Some(true),
            "Logo preserveAspectRatio should be true for theme {}",
            theme.id
        );

        // Defensive: the invalid legacy location must NOT be present. If
        // someone accidentally reintroduces `display.logo`, fastfetch
        // rejects the whole display block and the apple logo disappears.
        assert!(
            parsed.get("display").and_then(|d| d.get("logo")).is_none(),
            "theme {} still has the legacy `display.logo` nesting — fastfetch 2.x requires top-level `logo`",
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

    // Per fastfetch 2.x schema: color overrides live under `display.color`.
    let color_obj = parsed
        .get("display")
        .and_then(|d| d.get("color"))
        .expect("Missing display.color object in JSONC");

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

    let color_obj = parsed
        .get("display")
        .and_then(|d| d.get("color"))
        .expect("Missing display.color object");

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

/// End-to-end guard: hand our generated jsonc to the real `fastfetch` binary
/// and fail if fastfetch prints `JsonConfig Error`. This catches schema drift
/// that JSON-level assertions miss — e.g., the pre-fix bug where
/// `display.logo` and `display.key-width` were structurally valid JSON but
/// silently rejected by fastfetch 2.x, so the Apple logo never rendered.
/// Only runs when `fastfetch` is installed; otherwise skipped so CI without
/// fastfetch still passes.
#[test]
fn test_generated_config_parses_in_real_fastfetch() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Locate fastfetch; skip if unavailable
    let fastfetch_bin = match Command::new("fastfetch").arg("--version").output() {
        Ok(out) if out.status.success() => "fastfetch".to_string(),
        _ => match Command::new("/opt/homebrew/bin/fastfetch")
            .arg("--version")
            .output()
        {
            Ok(out) if out.status.success() => "/opt/homebrew/bin/fastfetch".to_string(),
            _ => {
                eprintln!("skipping: fastfetch binary not found");
                return;
            }
        },
    };

    let adapter = FastfetchAdapter;
    let registry = ThemeRegistry::new().expect("ThemeRegistry init failed");
    let theme = registry
        .get("catppuccin-mocha")
        .expect("catppuccin-mocha theme not found");
    let jsonc = adapter
        .generate_jsonc_config(theme)
        .expect("Failed to generate JSONC");

    // Write to a temp file and invoke fastfetch -c <path>. Running with only
    // the title module keeps the test cheap and avoids depending on GPU/etc.
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg_path = dir.path().join("config.jsonc");
    std::fs::write(&cfg_path, &jsonc).expect("write jsonc");

    let output = Command::new(&fastfetch_bin)
        .arg("-c")
        .arg(&cfg_path)
        .arg("-l")
        .arg("none") // skip logo drawing to keep stdout small/stable
        .stdin(Stdio::null())
        .output()
        .expect("spawn fastfetch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    assert!(
        !combined.contains("JsonConfig Error"),
        "fastfetch rejected the generated config — schema drift.\n\
         jsonc:\n{}\n\nfastfetch stdout:\n{}\n\nfastfetch stderr:\n{}",
        jsonc,
        stdout,
        stderr,
    );

    // Silence unused warning on the writer helper path
    let _ = std::io::stdout().flush();
}
