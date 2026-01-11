use crate::adapter::font::FontAdapter;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::Result;

/// Handle `slate font` command
/// Supports two modes:
/// 1. `slate font <name>` — Apply explicit font directly
/// 2. `slate font` (no args) — Launch interactive font picker with Nerd + System groups
pub fn handle_font(font_name: Option<&str>) -> Result<()> {
    if let Some(name) = font_name {
        // Direct apply path: validate and apply font
        let env = SlateEnv::from_process()?;

        // Validate font exists in either nerd or system lists (picker assembles display)
        let discovery = FontAdapter::discover_all_fonts()?;
        let all_fonts: Vec<&String> = discovery
            .nerd_fonts
            .iter()
            .chain(discovery.system_fonts.iter())
            .collect();

        if !all_fonts.contains(&&name.to_string()) {
            eprintln!(
                "{} Font '{}' not found. Run 'slate font' to see available options.",
                Symbols::FAILURE,
                name
            );
            return Ok(());
        }

        // Apply font
        FontAdapter::apply_font(&env, name)?;

        println!("{} Font changed to '{}'", Symbols::SUCCESS, name);
        Ok(())
    } else {
        // Picker path: show font picker UI per 
        show_font_picker()
    }
}

/// Show interactive font picker with two groups: Nerd Fonts and System Fonts 
fn show_font_picker() -> Result<()> {
    let env = SlateEnv::from_process()?;
    let discovery = FontAdapter::discover_all_fonts()?;

    // Fail loud only if BOTH lists empty
    if discovery.nerd_fonts.is_empty() && discovery.system_fonts.is_empty() {
        eprintln!(
            "{} No supported fonts found. Run 'slate setup' to install the recommended Nerd Fonts.",
            Symbols::FAILURE
        );
        return Ok(());
    }

    // Build picker items with proper cliclack select pattern 
    // Store (key, display_label, font_name, is_system, is_group_header) for selection logic
    let mut picker_items: Vec<(String, String, String, bool, bool)> = Vec::new();

    // Group 1: Nerd Fonts with JetBrainsMono recommendation 
    if !discovery.nerd_fonts.is_empty() {
        picker_items.push((
            "header_nerd".to_string(),
            "━━ Nerd Fonts ━━".to_string(),
            String::new(),
            false,
            true,
        ));

        // Check if JetBrainsMono installed (place first + mark with ✦)
        for font in discovery.nerd_fonts.iter() {
            if font.contains("JetBrainsMono") {
                picker_items.push((
                    "nerd_jetbrains".to_string(),
                    format!("✦ {} (recommended)", font),
                    font.clone(),
                    false,
                    false,
                ));
                break;
            }
        }

        // Add other Nerd Fonts (except JetBrainsMono which is already placed)
        for (idx, font) in discovery.nerd_fonts.iter().enumerate() {
            if !font.contains("JetBrainsMono") {
                picker_items.push((
                    format!("nerd_{}", idx),
                    font.clone(),
                    font.clone(),
                    false,
                    false,
                ));
            }
        }
    }

    // Group 2: System Fonts with soft warning 
    if !discovery.system_fonts.is_empty() {
        picker_items.push((
            "header_system".to_string(),
            "━━ System Fonts (no icons) ━━".to_string(),
            String::new(),
            true,
            true,
        ));
        for (idx, font) in discovery.system_fonts.iter().enumerate() {
            picker_items.push((
                format!("system_{}", idx),
                font.clone(),
                font.clone(),
                true,
                false,
            ));
        }
    }

    // Show hint if no Nerd Fonts but system fonts available (/)
    if discovery.nerd_fonts.is_empty() && !discovery.system_fonts.is_empty() {
        println!("(i) Run 'slate setup' to install the recommended JetBrainsMono Nerd Font");
    }

    // Launch picker using cliclack select pattern
    cliclack::intro("✦ Change Font")?;

    // Build menu using cliclack select
    let mut menu_builder = cliclack::select("Select font:");

    for (key, display_label, _, _, _) in &picker_items {
        menu_builder = menu_builder.item(key.as_str(), display_label.as_str(), "");
    }

    let selected = menu_builder.interact()?;

    // Find the selected item by key
    for (key, display_label, _, is_system, is_header) in &picker_items {
        if key == selected {
            // Skip group headers
            if *is_header {
                return Ok(());
            }

            // Extract bare font name (remove ✦ and (recommended) markers from display label)
            let bare_name = display_label
                .trim_start_matches("✦ ")
                .trim_end_matches(" (recommended)")
                .to_string();

            // Show system fonts warning per (soft warning, not failure)
            if *is_system {
                println!("(i) System fonts lack Nerd Font icons — starship/eza/lazygit glyphs may render as '?'");
            }

            // Apply font
            FontAdapter::apply_font(&env, &bare_name)?;

            println!("{} Font changed to '{}'", Symbols::SUCCESS, bare_name);
            println!("Font will be used on next terminal session.");
            break;
        }
    }

    Ok(())
}
