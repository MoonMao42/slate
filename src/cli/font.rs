use crate::adapter::font::{FontAdapter, FontDiscovery};
use crate::cli::font_selection::FontCatalog;
use crate::design::symbols::Symbols;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};

fn font_uses_basic_prompt(font_name: &str) -> bool {
    !FontAdapter::is_nerd_font_name(font_name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedFontChoice {
    Installed(String),
    Catalog(String),
}

impl ResolvedFontChoice {
    pub(crate) fn font_name(&self) -> &str {
        match self {
            Self::Installed(name) | Self::Catalog(name) => name,
        }
    }
}

fn find_installed_font(discovery: &FontDiscovery, requested_key: &str) -> Option<String> {
    discovery
        .nerd_fonts
        .iter()
        .chain(discovery.system_fonts.iter())
        .find(|font| FontAdapter::family_match_key(font) == requested_key)
        .cloned()
}

fn resolve_font_choice_with_discovery(
    name: &str,
    discovery: &FontDiscovery,
) -> Result<ResolvedFontChoice> {
    let requested_key = FontAdapter::family_match_key(name);

    if let Some(installed) = find_installed_font(discovery, &requested_key) {
        return Ok(ResolvedFontChoice::Installed(installed));
    }

    if let Some(catalog_font) = FontCatalog::all_fonts().into_iter().find(|font| {
        font.name == name
            || font.id == name
            || FontAdapter::family_match_key(font.name) == requested_key
            || FontAdapter::family_match_key(font.id) == requested_key
    }) {
        let canonical_key = FontAdapter::family_match_key(catalog_font.name);
        if let Some(installed) = find_installed_font(discovery, &canonical_key) {
            return Ok(ResolvedFontChoice::Installed(installed));
        }

        return Ok(ResolvedFontChoice::Catalog(catalog_font.name.to_string()));
    }

    Err(SlateError::InvalidConfig(format!(
        "Font '{}' not found. Run 'slate font' to see available options.",
        name
    )))
}

pub(crate) fn resolve_font_choice(name: &str) -> Result<ResolvedFontChoice> {
    let discovery = FontAdapter::discover_all_fonts()?;
    resolve_font_choice_with_discovery(name, &discovery)
}

/// Handle `slate font` command
///
/// Supports two modes:
/// 1. `slate font <name>` — Apply explicit font directly
/// 2. `slate font` (no args) — Launch interactive font picker with Nerd + System groups
pub fn handle_font(font_name: Option<&str>) -> Result<()> {
    if let Some(name) = font_name {
        // Direct apply path: validate and apply font
        let env = SlateEnv::from_process()?;
        let selection = resolve_font_choice(name)?;
        let resolved_font = selection.font_name().to_string();

        if matches!(selection, ResolvedFontChoice::Catalog(_)) {
            eprintln!("Downloading {}...", resolved_font);
            download_catalog_font(&resolved_font, &env).map_err(|err| {
                SlateError::InvalidConfig(format!(
                    "Font '{}' could not be installed: {}",
                    resolved_font, err
                ))
            })?;
            eprintln!("{} {} downloaded", Symbols::SUCCESS, resolved_font);
        }

        FontAdapter::apply_font(&env, &resolved_font)?;

        println!(
            "{} Updated font to {} in Slate-managed terminal configs.",
            Symbols::SUCCESS,
            resolved_font
        );
        if font_uses_basic_prompt(&resolved_font) {
            println!("(i) Basic Starship mode enabled for new shells because this font does not include Nerd Font glyphs.");
        } else {
            println!("{}", crate::platform::fonts::activation_hint());
        }
        Ok(())
    } else {
        // Picker path: show font picker UI
        show_font_picker()
    }
}

/// Show interactive font picker with installed fonts + catalog fonts available for download.
fn show_font_picker() -> Result<()> {
    let env = SlateEnv::from_process()?;
    let discovery = FontAdapter::discover_all_fonts()?;

    // Build the set of installed nerd font family keys for quick lookup
    let installed_keys: std::collections::HashSet<String> = discovery
        .nerd_fonts
        .iter()
        .map(|f| FontAdapter::family_match_key(f))
        .collect();

    // Build picker items: (key, display_label, font_name, is_system, needs_install, hint)
    let mut picker_items: Vec<(String, String, String, bool, bool, &str)> = Vec::new();

    // ── Group 1: Nerd Fonts (installed) ──
    let mut is_first_nerd = true;
    if !discovery.nerd_fonts.is_empty() {
        // JetBrainsMono first + recommended marker
        for font in discovery.nerd_fonts.iter() {
            if font.contains("JetBrainsMono") {
                let hint = if is_first_nerd { "Nerd Fonts" } else { "" };
                is_first_nerd = false;
                picker_items.push((
                    "nerd_jetbrains".to_string(),
                    format!("✦ {} (recommended)", font),
                    font.clone(),
                    false,
                    false,
                    hint,
                ));
                break;
            }
        }

        for (idx, font) in discovery.nerd_fonts.iter().enumerate() {
            if !font.contains("JetBrainsMono") {
                let hint = if is_first_nerd { "Nerd Fonts" } else { "" };
                is_first_nerd = false;
                picker_items.push((
                    format!("nerd_{}", idx),
                    font.clone(),
                    font.clone(),
                    false,
                    false,
                    hint,
                ));
            }
        }
    }

    // ── Group 2: Catalog fonts not yet installed (available for download) ──
    let catalog_fonts = FontCatalog::all_fonts();
    let mut has_downloadable = false;
    for (idx, cat_font) in catalog_fonts.iter().enumerate() {
        let cat_key = FontAdapter::family_match_key(cat_font.name);
        if !installed_keys.contains(&cat_key) {
            let hint = if !has_downloadable {
                has_downloadable = true;
                "Available to Download"
            } else {
                ""
            };
            picker_items.push((
                format!("catalog_{}", idx),
                format!("{} (not installed)", cat_font.name),
                cat_font.name.to_string(),
                false,
                true,
                hint,
            ));
        }
    }

    // ── Group 3: System Fonts ──
    if !discovery.system_fonts.is_empty() {
        let mut is_first_system = true;
        for (idx, font) in discovery.system_fonts.iter().enumerate() {
            let hint = if is_first_system {
                is_first_system = false;
                "System (no icons)"
            } else {
                ""
            };
            picker_items.push((
                format!("system_{}", idx),
                font.clone(),
                font.clone(),
                true,
                false,
                hint,
            ));
        }
    }

    if picker_items.is_empty() {
        eprintln!(
            "{} No supported fonts found. Run 'slate setup' to install the recommended Nerd Fonts.",
            Symbols::FAILURE
        );
        return Ok(());
    }

    // Hint if no installed Nerd Fonts
    if discovery.nerd_fonts.is_empty() && !has_downloadable {
        println!("(i) Run 'slate setup' to install the recommended Nerd Fonts");
    }

    // Launch picker
    cliclack::intro("✦ Change Font")?;

    let mut menu_builder = cliclack::select("Select font:");
    for (key, display_label, _, _, _, hint) in &picker_items {
        menu_builder = menu_builder.item(key.as_str(), display_label.as_str(), *hint);
    }

    let selected = menu_builder.interact()?;

    // Find the selected item
    for (key, display_label, font_name, is_system, needs_install, _) in &picker_items {
        if key != selected {
            continue;
        }

        // Extract bare font name (remove markers)
        let bare_name = display_label
            .trim_start_matches("✦ ")
            .trim_end_matches(" (recommended)")
            .trim_end_matches(" (not installed)")
            .to_string();

        // Download if needed
        if *needs_install {
            let spinner = cliclack::spinner();
            spinner.start(format!("Downloading {}...", bare_name));

            match download_catalog_font(font_name, &env) {
                Ok(_) => {
                    spinner.stop(format!("{} {} downloaded", Symbols::SUCCESS, bare_name));
                }
                Err(e) => {
                    spinner.error(format!("{} Download failed: {}", Symbols::FAILURE, e));
                    return Ok(());
                }
            }
        }

        // Show system fonts warning
        if *is_system {
            println!("(i) System fonts lack Nerd Font icons. Slate will switch new shells to the basic Starship profile.");
        }

        // Apply font
        FontAdapter::apply_font(&env, &bare_name)?;

        println!(
            "{} Updated font to {} in Slate-managed terminal configs.",
            Symbols::SUCCESS,
            bare_name
        );

        if font_uses_basic_prompt(&bare_name) {
            println!("(i) Basic Starship mode enabled for new shells because this font does not include Nerd Font glyphs.");
        } else {
            println!("{}", crate::platform::fonts::activation_hint());
        }
        break;
    }

    Ok(())
}

/// Download a catalog font using the same fallback chain as setup.
fn download_catalog_font(font_name: &str, env: &SlateEnv) -> std::result::Result<(), String> {
    use crate::cli::setup_executor::{
        copy_font_from_caskroom, download_font_release, install_font,
    };

    // Resolve display name ("Hack Nerd Font") to catalog ID ("hack")
    // because download functions look up by ID.
    let font_id = FontCatalog::all_fonts()
        .into_iter()
        .find(|f| f.name == font_name)
        .map(|f| f.id.to_string());
    let lookup = font_id.as_deref().unwrap_or(font_name);

    if matches!(
        crate::platform::packages::detect_backend(),
        crate::platform::packages::PackageManagerBackend::Homebrew
    ) {
        if install_font(lookup).is_ok() {
            return Ok(());
        }

        if copy_font_from_caskroom(lookup, env).is_ok() {
            return Ok(());
        }
    }

    // Download from Nerd Fonts releases
    download_font_release(lookup, env).map_err(|e| {
        let full = e.to_string();
        full.strip_prefix("Internal error: ")
            .unwrap_or(&full)
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::{resolve_font_choice_with_discovery, ResolvedFontChoice};
    use crate::adapter::font::FontDiscovery;

    #[test]
    fn test_resolve_font_choice_matches_catalog_id_to_installed_font() {
        let discovery = FontDiscovery {
            nerd_fonts: vec!["JetBrains Mono Nerd Font".to_string()],
            system_fonts: vec![],
        };

        let choice = resolve_font_choice_with_discovery("jetbrains-mono", &discovery).unwrap();

        assert_eq!(
            choice,
            ResolvedFontChoice::Installed("JetBrains Mono Nerd Font".to_string())
        );
    }

    #[test]
    fn test_resolve_font_choice_rejects_unknown_font() {
        let discovery = FontDiscovery {
            nerd_fonts: vec![],
            system_fonts: vec!["Menlo".to_string()],
        };

        let err = resolve_font_choice_with_discovery("Definitely Not A Font", &discovery)
            .unwrap_err()
            .to_string();

        assert!(err.contains("Font 'Definitely Not A Font' not found"));
    }
}
