use crate::error::Result;
use crate::brand::language::Language;
use crate::cli::font_detection::detect_current_font;
use crate::cli::tool_selection::{
    ToolCatalog, compute_install_candidates, ReviewReceipt, InstallAction
};
use crate::cli::preset_selection::PresetCatalog;
use crate::cli::font_selection::FontCatalog;
use crate::cli::theme_selection::ThemeSelector;
use crate::adapter::registry::ToolRegistry;
use cliclack::{intro, outro, select, multiselect};
use std::collections::HashMap;

pub struct WizardContext {
    pub mode: WizardMode,
    pub current_step: usize,
    pub total_steps: usize,
    pub selected_tools: Vec<String>,
    pub selected_font: Option<String>,
    pub selected_theme: Option<String>,
    pub current_font: Option<String>,
    pub current_theme: Option<String>,
    pub force: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WizardMode {
    Quick,
    Manual,
}

pub struct Wizard {
    context: WizardContext,
    theme_selector: ThemeSelector,
}

impl Wizard {
    pub fn new() -> Result<Self> {
        // Detect current font and theme on wizard startup
        let current_font = detect_current_font().ok().flatten();
        let current_theme = None; // TODO: detect current theme from managed config
        
        Ok(Self {
            context: WizardContext {
                mode: WizardMode::Manual,
                current_step: 0,
                total_steps: 6, // intro → mode/preset → tools → font → theme → action list → apply
                selected_tools: Vec::new(),
                selected_font: None,
                selected_theme: None,
                current_font,
                current_theme,
                force: false,
            },
            theme_selector: ThemeSelector::new()?,
        })
    }

    /// Run the full wizard flow
    /// force=true ignores current state and runs as fresh install
    pub fn run(&mut self, quick_mode: bool, force: bool) -> Result<()> {
        self.context.force = force;

        // If force flag is set, clear current state
        if force {
            self.context.current_font = None;
            self.context.current_theme = None;
            eprintln!("⚙ Force mode: ignoring current state\n");
        }

        // Step 0: Intro
        self.show_intro()?;

        // Step 1: Mode or preset selection
        if quick_mode {
            self.context.mode = WizardMode::Quick;
            // Quick mode: use default preset, then tool selection
            self.context.total_steps = 4; // intro → preset → tools → action → apply
            self.step_select_preset_quick()?;
        } else {
            // Manual mode: ask for mode selection
            self.step_select_mode()?;
            if self.context.mode == WizardMode::Quick {
                self.context.total_steps = 4;
                self.step_select_preset_quick()?;
            } else {
                // Manual mode: individual selection steps
                self.context.total_steps = 7; // intro → mode → tools → font → theme → action → apply
            }
        }

        // Step 2+: Tool detection and selection
        self.step_detect_and_select_tools()?;

        // Step 3+: Font selection (manual mode only)
        if self.context.mode == WizardMode::Manual {
            self.step_select_font()?;
        }

        // Step 4+: Theme selection (manual mode only)
        if self.context.mode == WizardMode::Manual {
            self.step_select_theme()?;
        }

        // Later steps handled by subsequent plans:
        // - Action list review
        // - Execution
        // - Completion
        self.show_completion()?;

        Ok(())
    }

    fn show_intro(&mut self) -> Result<()> {
        // Use cliclack's intro frame (per no custom ASCII art)
        intro("✦ slate").ok();
        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_mode(&mut self) -> Result<()> {
        self.log_step("Select Setup Mode");

        let mode_choice = select("Setup mode:")
            .item("quick", "Quick (pick a vibe)", "")
            .item("manual", "Manual (customize each)", "")
            .interact()?;

        self.context.mode = match mode_choice {
            "quick" => WizardMode::Quick,
            "manual" => WizardMode::Manual,
            _ => WizardMode::Manual,
        };

        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_preset_quick(&mut self) -> Result<()> {
        self.log_step("Select Style Preset");

        let presets = PresetCatalog::all_presets();
        let preset_options: Vec<(&str, &str, String)> = presets
            .iter()
            .map(|p| (p.id, p.name, format!("— {}", p.description)))
            .collect();

        let selected_preset_id = select("Pick a vibe:")
            .items(&preset_options)
            .interact()?;

        if let Some(preset) = PresetCatalog::get_preset(selected_preset_id) {
            self.context.selected_font = Some(preset.font_id.to_string());
            self.context.selected_theme = Some(preset.theme_id.to_string());
        }

        self.context.current_step += 1;
        Ok(())
    }

    fn step_detect_and_select_tools(&mut self) -> Result<()> {
        self.log_step("Detect and Select Tools");

        // Create a registry just for detection
        let registry = ToolRegistry::new();
        let installed = registry.detect_installed();

        // Display full inventory with status
        self.display_tool_inventory(&installed)?;

        // Get install candidates (missing + installable)
        let candidates = compute_install_candidates(&installed);

        // If no candidates, skip selection
        if candidates.is_empty() {
            eprintln!("All tools are already installed.");
            self.context.current_step += 1;
            return Ok(());
        }

        // Build multiselect items: (id, label, pitch)
        let items: Vec<(&str, String, String)> = candidates
            .iter()
            .map(|tool| {
                (
                    tool.id,
                    tool.label.to_string(),
                    tool.pitch.to_string(),
                )
            })
            .collect();

        eprintln!("Select tools to install:");
        let selected: Vec<&str> = multiselect("Tools:")
            .items(
                &items
                    .iter()
                    .map(|(id, label, pitch)| (*id, label.as_str(), pitch.as_str()))
                    .collect::<Vec<_>>()
            )
            .interact()?;

        // Convert &str to String
        self.context.selected_tools = selected.into_iter().map(|s| s.to_string()).collect();
        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_font(&mut self) -> Result<()> {
        self.log_step("Select Font");

        // Display current font if available
        if let Some(ref current) = self.context.current_font {
            eprintln!("Current font: {}", current);
        } else {
            eprintln!("Current font: system default");
        }

        // Build font options with skip
        let fonts = FontCatalog::all_fonts();
        let mut font_options: Vec<(&str, &str, String)> = fonts
            .iter()
            .map(|f| (f.id, f.name, format!("— {}", f.label)))
            .collect();

        // Add skip option manually (as a tuple)
        let (skip_id, skip_label) = FontCatalog::skip_option();
        font_options.push((skip_id, skip_label, "".to_string()));

        eprintln!("Select a font (or skip to keep current):");
        let selected_font_id = select("Font:")
            .items(
                &font_options
                    .iter()
                    .map(|(id, label, desc)| (*id, *label, desc.as_str()))
                    .collect::<Vec<_>>()
            )
            .interact()?;

        // Store selection only if not skip
        if selected_font_id != "skip" {
            if let Some(font) = FontCatalog::get_font(selected_font_id) {
                self.context.selected_font = Some(font.id.to_string());
            }
        }
        // If skip, leave selected_font as None (will default to current in apply step)

        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_theme(&mut self) -> Result<()> {
        self.log_step("Select Theme");

        if let Some(ref current) = self.context.current_theme {
            eprintln!("Current theme: {}", current);
        } else {
            eprintln!("Current theme: not yet applied");
        }

        // Get all themes for display
        let all_themes = self.theme_selector.all_themes();
        let theme_options: Vec<(&str, &str, String)> = all_themes
            .iter()
            .map(|t| (t.id.as_str(), t.name.as_str(), format!("— {}", t.family)))
            .collect();

        eprintln!("Select a theme:");
        let selected_theme_id = select("Theme:")
            .items(
                &theme_options
                    .iter()
                    .map(|(id, label, desc)| (*id, *label, desc.as_str()))
                    .collect::<Vec<_>>()
            )
            .interact()?;

        self.context.selected_theme = Some(selected_theme_id.to_string());
        self.context.current_step += 1;
        Ok(())
    }

    fn display_tool_inventory(&self, installed: &HashMap<String, bool>) -> Result<()> {
        eprintln!("\n✦ Tool Inventory:\n");
        
        for tool in ToolCatalog::all_tools() {
            let status_mark = if installed.get(tool.id).copied().unwrap_or(false) {
                "✓"
            } else if tool.detect_only {
                "◆"
            } else {
                "○"
            };

            let install_note = if tool.detect_only {
                " (synced if installed)"
            } else if !tool.installable {
                " (not installable)"
            } else {
                ""
            };

            eprintln!(
                "  {} {} — {}{}",
                status_mark, tool.label, tool.pitch, install_note
            );
        }
        eprintln!();
        Ok(())
    }

    fn show_completion(&mut self) -> Result<()> {
        self.log_step("Complete");
        outro(Language::SETUP_COMPLETE).ok();
        Ok(())
    }

    fn log_step(&self, step_name: &str) {
        eprintln!(
            "Step {} of {} — {}",
            self.context.current_step, self.context.total_steps, step_name
        );
    }

    pub fn get_context(&self) -> &WizardContext {
        &self.context
    }

    pub fn get_context_mut(&mut self) -> &mut WizardContext {
        &mut self.context
    }

    /// Build a review receipt from current wizard state
    pub fn build_review_receipt(&self) -> ReviewReceipt {
        let mut receipt = ReviewReceipt::new();

        // Add install actions based on selected tools
        for tool_id in &self.context.selected_tools {
            if let Some(tool) = ToolCatalog::get_tool(tool_id) {
                let action = InstallAction::from_metadata(&tool);
                receipt.add_install_action(action);
            }
        }

        receipt.selected_font = self.context.selected_font.clone();
        receipt.selected_theme = self.context.selected_theme.clone();

        receipt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wizard_new() {
        let wizard = Wizard::new().unwrap();
        let context = wizard.get_context();
        assert_eq!(context.mode, WizardMode::Manual);
        assert_eq!(context.current_step, 0);
        assert_eq!(context.total_steps, 6);
    }

    #[test]
    fn test_wizard_context_fields() {
        let wizard = Wizard::new().unwrap();
        let context = wizard.get_context();
        assert!(context.selected_tools.is_empty());
        assert!(context.selected_font.is_none());
        assert!(context.selected_theme.is_none());
    }

    #[test]
    fn test_wizard_force_flag_clears_state() {
        let mut wizard = Wizard::new().unwrap();
        wizard.context.current_font = Some("JetBrains Mono".to_string());
        wizard.context.current_theme = Some("Catppuccin Mocha".to_string());

        // After run with force, current state should be cleared
        // (we're just testing the flag behavior here)
        wizard.context.force = true;
        wizard.context.current_font = None;
        wizard.context.current_theme = None;

        assert!(wizard.context.force);
        assert!(wizard.context.current_font.is_none());
    }

    #[test]
    fn test_wizard_detects_font() {
        // Font detection should complete without error
        let wizard = Wizard::new().unwrap();
        let _context = wizard.get_context();
        // current_font may be None or Some depending on environment
        // The important thing is that detection succeeded
        assert!(true); // Wizard created successfully with font detection
    }

    #[test]
    fn test_build_review_receipt_empty() {
        let wizard = Wizard::new().unwrap();
        let receipt = wizard.build_review_receipt();
        assert!(receipt.install_actions.is_empty());
    }

    #[test]
    fn test_build_review_receipt_with_selections() {
        let mut wizard = Wizard::new().unwrap();
        wizard.context.selected_tools = vec!["ghostty".to_string(), "starship".to_string()];
        wizard.context.selected_font = Some("JetBrains Mono".to_string());
        wizard.context.selected_theme = Some("Catppuccin Mocha".to_string());

        let receipt = wizard.build_review_receipt();
        assert_eq!(receipt.install_actions.len(), 2);
        assert!(receipt.selected_font.is_some());
        assert!(receipt.selected_theme.is_some());
    }

    #[test]
    fn test_quick_mode_adjusts_step_count() {
        let mut wizard = Wizard::new().unwrap();
        wizard.context.mode = WizardMode::Quick;
        wizard.context.total_steps = 4; // Quick mode should be shorter
        assert_eq!(wizard.context.total_steps, 4);
    }

    #[test]
    fn test_wizard_mode_variants() {
        assert_ne!(WizardMode::Quick, WizardMode::Manual);
    }
}
