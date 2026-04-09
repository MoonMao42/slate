use slate_cli::theme::ThemeRegistry;

/// `slate demo` command has been retired. Re-adding a Demo
/// variant to the `Commands` enum (or an `emit_demo_hint_once` call site)
/// would silently resurrect DEMO-02 and betray the CONTEXT
/// §domain "Gemini: previewing is a purchasing behavior, not a possession
/// behavior". This test locks the absence of the symbols at the source
/// level (sibling to `brand::migration::tests::no_raw_styling_ansi_...`).
#[test]
fn slate_demo_surface_stays_retired_post_phase_19() {
    use std::fs;
    use std::path::Path;

    fn read_all_rust_files(dir: &Path, out: &mut Vec<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip target/ and .git/
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name == "target" || name == ".git" || name.starts_with('.') {
                        continue;
                    }
                    read_all_rust_files(&path, out);
                } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        out.push(format!("{}\n{}", path.display(), content));
                    }
                }
            }
        }
    }

    let mut bundle: Vec<String> = Vec::new();
    read_all_rust_files(Path::new("src"), &mut bundle);
    let haystack = bundle.join("\n---FILE---\n");

    // Commands::Demo variant must not reappear (benches + tests allowed
    // to reference deleted symbols only in *comments* — we scan src/ only).
    assert!(
        !haystack.contains("Commands::Demo"),
        " `Commands::Demo` enum variant must stay retired — found a reference in src/"
    );
    assert!(
        !haystack.contains("emit_demo_hint_once"),
        " `emit_demo_hint_once` must stay retired — found a reference in src/"
    );
    assert!(
        !haystack.contains("suppress_demo_hint_for_this_process"),
        " `suppress_demo_hint_for_this_process` must stay retired — found a reference in src/"
    );
    assert!(
        !haystack.contains("Language::DEMO_HINT") && !haystack.contains("pub const DEMO_HINT"),
        " `DEMO_HINT` Language constant must stay retired"
    );
}

#[test]
fn test_all_themes_load() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");
    let all_themes = registry.all();

    assert_eq!(
        all_themes.len(),
        20,
        "Expected 20 theme variants (4 Catppuccin + 2 Tokyo Night + 3 Rosé Pine + 3 Kanagawa + 2 Everforest + 1 Dracula + 1 Nord + 2 Gruvbox + 2 Solarized)"
    );
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
    assert!(ids.contains(&"gruvbox-dark".to_string()));
    assert!(ids.contains(&"gruvbox-light".to_string()));
}

#[test]
fn test_catppuccin_family_has_4_variants() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");
    let families = registry.by_family();

    let catppuccin = families
        .get("Catppuccin")
        .expect("Catppuccin family not found");
    assert_eq!(catppuccin.len(), 4);
}

#[test]
fn test_tokyo_night_family_has_2_variants() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");
    let families = registry.by_family();

    let tokyo_night = families
        .get("Tokyo Night")
        .expect("Tokyo Night family not found");
    assert_eq!(tokyo_night.len(), 2);
}

#[test]
fn test_theme_validation() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    for theme in registry.all() {
        let result = theme.validate();
        assert!(
            result.is_ok(),
            "Theme {} failed validation: {:?}",
            theme.id,
            result
        );
    }
}

#[test]
fn test_theme_tool_refs_consistency() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    for theme in registry.all() {
        // Verify each tool_ref is non-empty
        assert!(
            theme.tool_refs.contains_key("ghostty"),
            "Ghostty ref empty for {}",
            theme.id
        );
        assert!(
            theme.tool_refs.contains_key("alacritty"),
            "Alacritty ref empty for {}",
            theme.id
        );
        assert!(
            theme.tool_refs.contains_key("bat"),
            "bat ref empty for {}",
            theme.id
        );

        // Verify tool_refs.get() works for all tools
        assert!(theme.tool_refs.contains_key("ghostty"));
        assert!(theme.tool_refs.contains_key("bat"));
    }
}

#[test]
fn test_palette_has_required_colors() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    for theme in registry.all() {
        let palette = &theme.palette;

        // All themes must have primary colors
        assert!(
            !palette.foreground.is_empty(),
            "Foreground empty for {}",
            theme.id
        );
        assert!(
            !palette.background.is_empty(),
            "Background empty for {}",
            theme.id
        );
        assert!(!palette.red.is_empty(), "Red empty for {}", theme.id);
        assert!(!palette.green.is_empty(), "Green empty for {}", theme.id);
        assert!(!palette.blue.is_empty(), "Blue empty for {}", theme.id);

        // Verify colors are hex format (basic check)
        assert!(
            palette.foreground.starts_with('#'),
            "Foreground not hex for {}",
            theme.id
        );
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

    let mocha = registry
        .get("catppuccin-mocha")
        .expect("Catppuccin Mocha not found");

    // Catppuccin should have specific colors in both optional fields and extras
    assert!(mocha.palette.rosewater.is_some());
    assert!(mocha.palette.flamingo.is_some());
    assert!(mocha.palette.mauve.is_some());

    // Verify extras HashMap is populated for Catppuccin
    assert!(
        !mocha.palette.extras.is_empty(),
        "Catppuccin extras should be populated"
    );
    assert_eq!(
        mocha.palette.extras.get("rosewater").map(String::as_str),
        Some("#f2d5cf")
    );
}

#[test]
fn test_non_catppuccin_themes_have_semantic_bg_fields() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    // Test Tokyo Night Dark
    let tokyo_dark = registry
        .get("tokyo-night-dark")
        .expect("Tokyo Night Dark not found");
    assert!(
        tokyo_dark.palette.bg_dim.is_some(),
        "bg_dim should be populated"
    );
    assert!(
        tokyo_dark.palette.bg_darker.is_some(),
        "bg_darker should be populated"
    );
    assert!(
        tokyo_dark.palette.bg_darkest.is_some(),
        "bg_darkest should be populated"
    );

    // Test Dracula
    let dracula = registry.get("dracula").expect("Dracula not found");
    assert!(
        dracula.palette.bg_dim.is_some(),
        "Dracula bg_dim should be populated"
    );
    assert!(
        dracula.palette.bg_darker.is_some(),
        "Dracula bg_darker should be populated"
    );
}

/// · VALIDATION row 1 — PickerState must surface its `theme_ids`
/// array grouped by `FAMILY_SORT_ORDER`.
/// We walk the returned ids, resolve each to its family, and assert the
/// family-index in `FAMILY_SORT_ORDER` is monotonically non-decreasing.
/// If the order ever regresses — e.g. a Catppuccin variant appearing after
/// a Tokyo Night one — the family grouping contract is broken.
/// No filesystem writes; pure data check on in-memory state.
#[test]
fn picker_launches_with_family_grouping() {
    use slate_cli::cli::picker::PickerState;
    use slate_cli::opacity::OpacityPreset;
    use slate_cli::theme::{ThemeRegistry, FAMILY_SORT_ORDER};

    let state = PickerState::new("catppuccin-mocha", OpacityPreset::Solid)
        .expect("picker state must build");
    let registry = ThemeRegistry::new().expect("registry");

    let mut last_family_idx: Option<usize> = None;
    let mut distinct_families: std::collections::BTreeSet<usize> =
        std::collections::BTreeSet::new();

    for id in state.theme_ids() {
        let theme = registry.get(id).expect("id from registry");
        let idx = FAMILY_SORT_ORDER
            .iter()
            .position(|f| *f == theme.family.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "theme {id} family {:?} must appear in FAMILY_SORT_ORDER",
                    theme.family
                )
            });
        if let Some(last) = last_family_idx {
            assert!(
                idx >= last,
                "family order violated at {id}: family {} (idx {idx}) came after idx {last}",
                theme.family
            );
        }
        last_family_idx = Some(idx);
        distinct_families.insert(idx);
    }

    assert!(
        distinct_families.len() >= 2,
        "picker must surface at least 2 families; got {distinct_families:?}"
    );
}

#[test]
fn test_catppuccin_extras_mapping() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    for theme_id in &[
        "catppuccin-latte",
        "catppuccin-frappe",
        "catppuccin-macchiato",
        "catppuccin-mocha",
    ] {
        let theme = registry
            .get(theme_id)
            .unwrap_or_else(|| panic!("Theme {} not found", theme_id));

        // All Catppuccin themes should have extras populated
        assert!(
            !theme.palette.extras.is_empty(),
            "Theme {} should have extras",
            theme_id
        );

        // Verify key Catppuccin colors are in extras
        assert!(
            theme.palette.extras.contains_key("rosewater"),
            "Theme {} missing rosewater in extras",
            theme_id
        );
        assert!(
            theme.palette.extras.contains_key("blue"),
            "Theme {} missing blue in extras",
            theme_id
        );
    }
}

/// Regression guard: bat + delta are the two adapters that consume
/// `tool_refs.<name>` verbatim into shipped configs (BAT_THEME env /
/// `[delta] syntax-theme`). Their values MUST match the upstream tool's
/// bundled theme corpus exactly. This test pins the expected strings
/// against future drift (e.g. someone "tidying"
/// `Solarized (dark)` to `solarized-dark`).
/// Verified upstream (21-RESEARCH §3, 2026-04-27):
/// bat 1337 `Solarized (dark)` / `Solarized (light)`
/// delta 0.19 `Solarized (dark)` / `Solarized (light)`
#[test]
fn test_solarized_bat_delta_match_upstream() {
    let registry = ThemeRegistry::new().expect("Failed to create registry");

    let dark = registry
        .get("solarized-dark")
        .expect("solarized-dark variant must exist");
    assert_eq!(
        dark.tool_refs.get("bat").map(String::as_str),
        Some("Solarized (dark)"),
        "bat: solarized-dark.tool_refs.bat must be 'Solarized (dark)' to match shipped bat tmTheme upstream corpus"
    );
    assert_eq!(
        dark.tool_refs.get("delta").map(String::as_str),
        Some("Solarized (dark)"),
        "delta: solarized-dark.tool_refs.delta must be 'Solarized (dark)' to match shipped delta (bat-backed) syntax-theme upstream corpus"
    );

    let light = registry
        .get("solarized-light")
        .expect("solarized-light variant must exist");
    assert_eq!(
        light.tool_refs.get("bat").map(String::as_str),
        Some("Solarized (light)"),
        "bat: solarized-light.tool_refs.bat must be 'Solarized (light)' to match shipped bat tmTheme upstream corpus"
    );
    assert_eq!(
        light.tool_refs.get("delta").map(String::as_str),
        Some("Solarized (light)"),
        "delta: solarized-light.tool_refs.delta must be 'Solarized (light)' to match shipped delta (bat-backed) syntax-theme upstream corpus"
    );
}
