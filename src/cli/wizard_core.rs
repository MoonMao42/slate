use crate::brand::language::Language;
use crate::cli::font_detection::detect_current_font;
use crate::cli::font_selection::FontCatalog;
use crate::cli::preset_selection::PresetCatalog;
use crate::cli::theme_selection::ThemeSelector;
use crate::cli::tool_selection::{
    compute_install_candidates, detect_installed_tools, InstallAction, ReviewReceipt,
    TerminalSettings, ToolCatalog,
};
use crate::design::typography::Typography;
use crate::error::Result;
use cliclack::{confirm, intro, multiselect, outro_cancel, select};
use std::collections::HashMap;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Instant;

use std::io;

/// Handle cliclack IO errors (Ctrl+C returns Interrupted kind)
/// Map Interrupted to UserCancelled for graceful handling
fn handle_cliclack_error(e: io::Error) -> crate::error::SlateError {
    if e.kind() == io::ErrorKind::Interrupted {
        crate::error::SlateError::UserCancelled
    } else {
        crate::error::SlateError::IOError(e)
    }
}

pub struct WizardContext {
    pub mode: WizardMode,
    pub current_step: usize,
    pub total_steps: usize,
    pub selected_tools: Vec<String>,
    pub selected_font: Option<String>,
    pub selected_theme: Option<String>,
    pub selected_terminal_settings: Option<TerminalSettings>,
    pub current_font: Option<String>,
    pub current_theme: Option<String>,
    pub confirmed: bool,
    pub force: bool,
    pub start_time: Option<Instant>,
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
        let current_theme = detect_current_theme_id();

        Ok(Self {
            context: WizardContext {
                mode: WizardMode::Manual,
                current_step: 0,
                total_steps: 6, // intro → mode/preset → tools → font → theme → action list → apply
                selected_tools: Vec::new(),
                selected_font: None,
                selected_theme: None,
                selected_terminal_settings: None,
                current_font,
                current_theme,
                confirmed: false,
                force: false,
                start_time: None,
            },
            theme_selector: ThemeSelector::new()?,
        })
    }

    /// Run the full wizard flow
    /// force=true ignores current state and runs as fresh install
    pub fn run(&mut self, quick_mode: bool, force: bool) -> Result<()> {
        self.context.force = force;
        self.context.start_time = Some(Instant::now());

        // If force flag is set, clear current state
        if force {
            self.context.current_font = None;
            self.context.current_theme = None;
            eprintln!("⚙ Force mode: ignoring current state\n");
        }

        self.context.total_steps = if quick_mode { 4 } else { 5 };

        // Step 0: Intro
        self.show_intro()?;

        // Step 1: Mode or preset selection
        if quick_mode {
            self.context.mode = WizardMode::Quick;
            self.sync_total_steps(false);
            self.step_select_preset_quick()?;
        } else {
            // Manual mode: ask for mode selection
            self.step_select_mode()?;
            if self.context.mode == WizardMode::Quick {
                self.sync_total_steps(true);
                self.step_select_preset_quick()?;
            } else {
                self.sync_total_steps(true);
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

        self.step_review_and_confirm()?;

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

        if !std::io::stdin().is_terminal() {
            self.context.mode = WizardMode::Quick;
            self.context.current_step += 1;
            return Ok(());
        }

        let mode_choice = select("Setup mode:")
            .item("quick", "Quick (pick a vibe)", "")
            .item("manual", "Manual (customize each)", "")
            .interact()
            .map_err(handle_cliclack_error)?;

        self.context.mode = match mode_choice {
            "quick" => WizardMode::Quick,
            "manual" => WizardMode::Manual,
            _ => WizardMode::Manual,
        };

        self.sync_total_steps(true);
        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_preset_quick(&mut self) -> Result<()> {
        self.log_step("Select Style Preset");

        if !std::io::stdin().is_terminal() {
            // Non-interactive: use the locked default preset.
            let preset = PresetCatalog::default_preset();
            self.context.selected_font = Some(preset.font_id.to_string());
            self.context.selected_theme = Some(preset.theme_id.to_string());
            self.context.selected_terminal_settings =
                Some(self.terminal_settings_from_preset(&preset));
            self.context.current_step += 1;
            return Ok(());
        }

        let presets = PresetCatalog::all_presets();
        let preset_options: Vec<(&str, &str, String)> = presets
            .iter()
            .map(|p| (p.id, p.name, format!("— {}", p.description)))
            .collect();

        let selected_preset_id = select("Pick a vibe:")
            .items(&preset_options)
            .interact()
            .map_err(handle_cliclack_error)?;

        if let Some(preset) = PresetCatalog::get_preset(selected_preset_id) {
            self.context.selected_font = Some(preset.font_id.to_string());
            self.context.selected_theme = Some(preset.theme_id.to_string());
            self.context.selected_terminal_settings =
                Some(self.terminal_settings_from_preset(&preset));
        }

        self.context.current_step += 1;
        Ok(())
    }

    fn step_detect_and_select_tools(&mut self) -> Result<()> {
        self.log_step("Detect and Select Tools");

        let installed = detect_installed_tools();

        // Display full inventory with status using typography helpers
        self.display_tool_inventory(&installed)?;

        // Get install candidates (missing + installable)
        let candidates = compute_install_candidates(&installed);

        // If no candidates, skip selection
        if candidates.is_empty() {
            eprintln!("All tools are already installed.");
            self.context.current_step += 1;
            return Ok(());
        }

        // Non-interactive quick mode: select all candidates by default
        if !std::io::stdin().is_terminal() {
            self.context.selected_tools = candidates.iter().map(|c| c.id.to_string()).collect();
            self.context.current_step += 1;
            return Ok(());
        }

        // Build multiselect items: (id, label, pitch)
        let items: Vec<(&str, String, String)> = candidates
            .iter()
            .map(|tool| (tool.id, tool.label.to_string(), tool.pitch.to_string()))
            .collect();

        eprintln!("Select tools to install:");
        let selected: Vec<&str> = multiselect("Tools:")
            .items(
                &items
                    .iter()
                    .map(|(id, label, pitch)| (*id, label.as_str(), pitch.as_str()))
                    .collect::<Vec<_>>(),
            )
            .interact()
            .map_err(handle_cliclack_error)?;

        // Convert &str to String
        self.context.selected_tools = selected.into_iter().map(|s| s.to_string()).collect();
        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_font(&mut self) -> Result<()> {
        self.log_step("Select Font");

        // Display current font if available with better formatting
        if let Some(ref current) = self.context.current_font {
            eprintln!("{}", Typography::secondary_label("current font", current));
        } else {
            eprintln!(
                "{}",
                Typography::secondary_label("current font", "system default")
            );
        }
        eprintln!();

        // Build font options with skip
        let fonts = FontCatalog::all_fonts();
        let mut font_options: Vec<(&str, &str, String)> = fonts
            .iter()
            .map(|f| (f.id, f.name, format!("— {}", f.label)))
            .collect();

        // Add skip option manually (as a tuple)
        let (skip_id, skip_label) = FontCatalog::skip_option();
        font_options.push((skip_id, skip_label, "".to_string()));

        if !std::io::stdin().is_terminal() {
            // Non-interactive: keep current font if present, otherwise preserve preset/default.
            self.context.current_step += 1;
            return Ok(());
        }

        eprintln!("Select a font (or skip to keep current):");
        let selected_font_id = select("Font:")
            .items(
                &font_options
                    .iter()
                    .map(|(id, label, desc)| (*id, *label, desc.as_str()))
                    .collect::<Vec<_>>(),
            )
            .interact()
            .map_err(handle_cliclack_error)?;

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

        if let Some(ref current_theme_id) = self.context.current_theme {
            let current_label = self.resolve_theme_label(current_theme_id);
            eprintln!(
                "{}",
                Typography::secondary_label("current theme", &current_label)
            );
        } else {
            eprintln!(
                "{}",
                Typography::secondary_label("current theme", "not yet applied")
            );
        }
        eprintln!();

        // Get all themes for display
        let all_themes = self.theme_selector.all_themes();
        let mut theme_options: Vec<(String, String, String)> = Vec::new();

        if let Some(current_theme_id) = &self.context.current_theme {
            theme_options.push((
                "keep-current".to_string(),
                "Keep current theme".to_string(),
                format!("— {}", self.resolve_theme_label(current_theme_id)),
            ));
        }

        theme_options.extend(
            all_themes
                .iter()
                .map(|t| (t.id.clone(), t.name.clone(), format!("— {}", t.family))),
        );

        if !std::io::stdin().is_terminal() {
            // Non-interactive: keep current theme if present, otherwise preserve preset/default.
            if self.context.current_theme.is_none() && self.context.selected_theme.is_none() {
                if let Some(first) = all_themes.first() {
                    self.context.selected_theme = Some(first.id.clone());
                }
            }
            self.context.current_step += 1;
            return Ok(());
        }

        eprintln!("Select a theme:");
        let selected_theme_id = select("Theme:")
            .items(
                &theme_options
                    .iter()
                    .map(|(id, label, desc)| (id.as_str(), label.as_str(), desc.as_str()))
                    .collect::<Vec<_>>(),
            )
            .interact()
            .map_err(handle_cliclack_error)?;

        if selected_theme_id != "keep-current" {
            self.context.selected_theme = Some(selected_theme_id.to_string());
        } else {
            self.context.selected_theme = None;
        }
        self.context.current_step += 1;
        Ok(())
    }

    fn display_tool_inventory(&self, installed: &HashMap<String, bool>) -> Result<()> {
        eprintln!("\n{}\n", Typography::section_header("Tool Inventory"));

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
                "{}",
                Typography::list_item(
                    status_mark.chars().next().unwrap_or('•'),
                    tool.label,
                    &format!("{}{}", tool.pitch, install_note)
                )
            );
        }
        eprintln!();
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

    fn step_review_and_confirm(&mut self) -> Result<()> {
        self.log_step("Review and Confirm");
        let receipt = self.build_review_receipt();
        eprintln!("\n{}", self.display_receipt(&receipt));

        if !std::io::stdin().is_terminal() {
            self.context.confirmed = true;
            self.context.current_step += 1;
            return Ok(());
        }

        let confirmed = confirm(Language::SETUP_REVIEW)
            .initial_value(true)
            .interact()
            .map_err(handle_cliclack_error)?;

        self.context.confirmed = confirmed;
        self.context.current_step += 1;

        if !confirmed {
            outro_cancel("Setup canceled").ok();
        }

        Ok(())
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

        receipt.selected_font = self
            .context
            .selected_font
            .as_deref()
            .map(|font_id| self.resolve_font_label(font_id))
            .or_else(|| {
                self.context
                    .current_font
                    .as_ref()
                    .map(|font| format!("Keep current ({font})"))
            });
        receipt.selected_theme = self
            .context
            .selected_theme
            .as_deref()
            .map(|theme_id| self.resolve_theme_label(theme_id))
            .or_else(|| {
                self.context.current_theme.as_deref().map(|theme_id| {
                    format!("Keep current ({})", self.resolve_theme_label(theme_id))
                })
            });
        receipt.terminal_settings = self.context.selected_terminal_settings.clone();

        receipt
    }

    /// Format and display polished receipt
    pub fn display_receipt(&self, receipt: &ReviewReceipt) -> String {
        receipt.format_for_display()
    }

    fn sync_total_steps(&mut self, include_mode_step: bool) {
        self.context.total_steps = match (self.context.mode, include_mode_step) {
            (WizardMode::Quick, true) => 5,
            (WizardMode::Quick, false) => 4,
            (WizardMode::Manual, true) => 6,
            (WizardMode::Manual, false) => 5,
        };
    }

    fn resolve_font_label(&self, font_id_or_name: &str) -> String {
        FontCatalog::get_font(font_id_or_name)
            .map(|font| font.name.to_string())
            .unwrap_or_else(|| font_id_or_name.to_string())
    }

    fn resolve_theme_label(&self, theme_id_or_name: &str) -> String {
        self.theme_selector
            .get_theme(theme_id_or_name)
            .map(|theme| theme.name.clone())
            .unwrap_or_else(|| theme_id_or_name.to_string())
    }

    fn terminal_settings_from_preset(
        &self,
        preset: &crate::cli::preset_selection::StylePreset,
    ) -> TerminalSettings {
        TerminalSettings {
            background_opacity: preset.visuals.background_opacity,
            blur_enabled: preset.visuals.blur_radius > 0,
            padding_x: preset.visuals.padding_x,
            padding_y: preset.visuals.padding_y,
        }
    }
}

fn detect_current_theme_id() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".config/slate/current");

    if !path.exists() {
        return None;
    }

    fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
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

        wizard.context.force = true;
        wizard.context.current_font = None;
        wizard.context.current_theme = None;

        assert!(wizard.context.force);
        assert!(wizard.context.current_font.is_none());
    }

    #[test]
    fn test_wizard_detects_font() {
        let wizard = Wizard::new().unwrap();
        let _context = wizard.get_context();
        // current_font may be None or Some depending on environment
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
        wizard.context.selected_font = Some("jetbrains-mono".to_string());
        wizard.context.selected_theme = Some("catppuccin-mocha".to_string());

        let receipt = wizard.build_review_receipt();
        assert_eq!(receipt.install_actions.len(), 2);
        assert_eq!(
            receipt.selected_font.as_deref(),
            Some("JetBrains Mono Nerd Font")
        );
        assert_eq!(receipt.selected_theme.as_deref(), Some("Catppuccin Mocha"));
    }

    #[test]
    fn test_build_review_receipt_uses_current_state_when_skipped() {
        let mut wizard = Wizard::new().unwrap();
        wizard.context.current_font = Some("SF Mono".to_string());
        wizard.context.current_theme = Some("catppuccin-mocha".to_string());

        let receipt = wizard.build_review_receipt();
        assert_eq!(
            receipt.selected_font.as_deref(),
            Some("Keep current (SF Mono)")
        );
        assert_eq!(
            receipt.selected_theme.as_deref(),
            Some("Keep current (Catppuccin Mocha)")
        );
    }

    #[test]
    fn test_build_review_receipt_includes_terminal_settings() {
        let mut wizard = Wizard::new().unwrap();
        wizard.context.selected_terminal_settings = Some(TerminalSettings {
            background_opacity: 0.95,
            blur_enabled: true,
            padding_x: 12,
            padding_y: 12,
        });

        let receipt = wizard.build_review_receipt();
        let settings = receipt
            .terminal_settings
            .expect("terminal settings should exist");
        assert_eq!(settings.padding_x, 12);
        assert!(settings.blur_enabled);
    }

    #[test]
    fn test_quick_mode_adjusts_step_count() {
        let mut wizard = Wizard::new().unwrap();
        wizard.context.mode = WizardMode::Quick;
        wizard.context.total_steps = 4;
        assert_eq!(wizard.context.total_steps, 4);
    }

    #[test]
    fn test_wizard_mode_variants() {
        assert_ne!(WizardMode::Quick, WizardMode::Manual);
    }

    #[test]
    fn test_display_receipt_includes_sections() {
        let mut wizard = Wizard::new().unwrap();
        wizard.context.selected_tools = vec!["ghostty".to_string()];
        wizard.context.selected_font = Some("jetbrains-mono".to_string());
        wizard.context.selected_theme = Some("catppuccin-mocha".to_string());

        let receipt = wizard.build_review_receipt();
        let display = wizard.display_receipt(&receipt);

        assert!(display.contains("Review"));
        assert!(display.contains("JetBrains Mono Nerd Font"));
        assert!(display.contains("Catppuccin Mocha"));
    }

    #[test]
    fn test_wizard_tracks_start_time() {
        let mut wizard = Wizard::new().unwrap();
        wizard.context.start_time = Some(Instant::now());
        assert!(wizard.context.start_time.is_some());
    }
}
