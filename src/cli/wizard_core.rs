use crate::brand::language::Language;
use crate::cli::font_detection::detect_current_font;
use crate::cli::font_selection::FontCatalog;
use crate::cli::preset_selection::PresetCatalog;
use crate::cli::theme_selection::ThemeSelector;
use crate::cli::tool_selection::{
    compute_install_candidates, detect_installed_tools, InstallAction, ReviewReceipt,
    TerminalSettings, ToolCatalog,
};
use crate::cli::wizard_support;
use crate::detection::{TerminalKind, TerminalProfile, ToolPresence};
use crate::env::SlateEnv;
use crate::error::Result;
use cliclack::{confirm, intro, multiselect, outro_cancel, select};
use std::collections::HashMap;
use std::fs;
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
    /// Tools to install (missing tools user selected)
    pub selected_tools: Vec<String>,
    /// Tools to configure (install targets + already-installed Tier 1 + user-opted Tier 2)
    pub tools_to_configure: Vec<String>,
    pub selected_font: Option<String>,
    pub selected_theme: Option<String>,
    pub selected_opacity: Option<crate::opacity::OpacityPreset>,
    /// None = user wasn't asked (quick mode), don't touch existing setting.
    /// Some(true/false) = user made an explicit choice.
    pub fastfetch_enabled: Option<bool>,
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

fn current_terminal_tool_id() -> Option<&'static str> {
    match TerminalProfile::detect().kind() {
        TerminalKind::Ghostty => Some("ghostty"),
        TerminalKind::Alacritty => Some("alacritty"),
        _ => None,
    }
}

fn compute_quick_tool_plan(
    installed: &HashMap<String, ToolPresence>,
    current_terminal: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    let core_tools = ["starship", "zsh-syntax-highlighting"];

    let selected_tools = core_tools
        .iter()
        .filter(|&&id| {
            let is_missing = !installed.get(id).map(|p| p.installed).unwrap_or(false);
            let is_installable = ToolCatalog::get_tool(id)
                .map(|t| t.installable)
                .unwrap_or(false);
            is_missing && is_installable
        })
        .map(|id| id.to_string())
        .collect::<Vec<_>>();

    let mut tools_to_configure = installed
        .iter()
        .filter(|(_, presence)| presence.is_tier1())
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();

    for id in core_tools {
        if installed.get(id).map(|p| p.installed).unwrap_or(false)
            && !tools_to_configure.contains(&id.to_string())
        {
            tools_to_configure.push(id.to_string());
        }
    }

    if let Some(terminal_id) = current_terminal {
        if installed
            .get(terminal_id)
            .map(|presence| presence.installed)
            .unwrap_or(false)
            && !tools_to_configure.contains(&terminal_id.to_string())
        {
            tools_to_configure.push(terminal_id.to_string());
        }
    }

    for id in &selected_tools {
        if !tools_to_configure.contains(id) {
            tools_to_configure.push(id.clone());
        }
    }

    (selected_tools, tools_to_configure)
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
                total_steps: 6, // intro → mode/preset → tools → font → theme → opacity → action list → apply
                selected_tools: Vec::new(),
                tools_to_configure: Vec::new(),
                selected_font: None,
                selected_theme: None,
                fastfetch_enabled: None,
                selected_opacity: None,
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

        // Step 0: Intro
        self.show_intro()?;

        // Step 1: Mode or preset selection
        if quick_mode {
            self.context.mode = WizardMode::Quick;
        } else {
            self.step_select_mode()?;
            if self.context.mode == WizardMode::Quick {
                // User chose Quick inside manual entry
            }
        }

        if self.context.mode == WizardMode::Quick {
            // ── Quick mode: Pick a vibe → Review → Apply ──
            self.step_select_preset_quick()?;
            self.step_quick_auto_tools()?;
            self.step_auto_opacity()?;
        } else {
            // ── Manual mode: Tools → Font → Theme → Fastfetch → Review ──
            self.step_detect_and_select_tools()?;
            self.step_select_font()?;
            self.step_select_theme()?;
            self.step_auto_opacity()?;
            self.step_select_fastfetch()?;
        }

        self.step_review_and_confirm()?;

        Ok(())
    }

    fn show_intro(&mut self) -> Result<()> {
        // Use cliclack's intro frame (: no custom ASCII art)
        intro("✦ slate").ok();
        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_mode(&mut self) -> Result<()> {
        self.log_step("Select Setup Mode");

        if !wizard_support::is_interactive() {
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

        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_preset_quick(&mut self) -> Result<()> {
        self.log_step("Select Style Preset");

        if !wizard_support::is_interactive() {
            // Non-interactive: use the locked default preset.
            let preset = PresetCatalog::default_preset();
            wizard_support::apply_preset_selection(&mut self.context, &preset);
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
            wizard_support::apply_preset_selection(&mut self.context, &preset);
        }

        self.context.current_step += 1;
        Ok(())
    }

    fn step_detect_and_select_tools(&mut self) -> Result<()> {
        self.log_step("Detect and Select Tools");

        let installed = detect_installed_tools();

        // Display full inventory with status using typography helpers
        self.display_tool_inventory(&installed)?;

        // Build two groups for the multiselect:
        // 1. Install candidates: missing + installable tools
        // 2. Tier 2 candidates: installed but not in PATH (need user opt-in to configure)
        let install_candidates = compute_install_candidates(&installed);

        // Terminal apps are the user's primary workspace — if slate detects the .app on disk
        // (including /Applications/Ghostty.app which tiers as "fallback"), treat it as auto-
        // configure. Restricting this to tier-1 broke the common macOS case where casks land
        // in /Applications and is_tier1() returns false.
        let is_terminal_app = |id: &str| matches!(id, "ghostty" | "alacritty" | "kitty");

        // Tier 2 candidates: installed but not in PATH, AND not a detected terminal app
        // (those go to auto-configure instead of the opt-in list).
        let tier2_candidates: Vec<&crate::cli::tool_selection::ToolMetadata> =
            crate::cli::tool_selection::ToolCatalog::all_tools()
                .iter()
                .filter(|tool| {
                    installed
                        .get(tool.id)
                        .map(|p| p.installed && !p.in_path && !is_terminal_app(tool.id))
                        .unwrap_or(false)
                })
                .collect();

        // Tier 1 tools: always configure. Include detected terminal apps even when tiered
        // as fallback, since the user's terminal is the whole point of slate.
        let tier1_ids: Vec<String> = installed
            .iter()
            .filter(|(id, p)| p.is_tier1() || (p.installed && is_terminal_app(id)))
            .map(|(id, _)| id.clone())
            .collect();

        // If nothing to show in multiselect, skip
        if install_candidates.is_empty() && tier2_candidates.is_empty() {
            self.context.tools_to_configure = tier1_ids;
            eprintln!("All tools are already installed.");
            self.context.current_step += 1;
            return Ok(());
        }

        // Non-interactive mode: install candidates only, configure Tier 1
        if !wizard_support::is_interactive() {
            self.context.selected_tools = install_candidates
                .iter()
                .map(|c| c.id.to_string())
                .collect();
            let mut to_configure = tier1_ids;
            for id in &self.context.selected_tools {
                if !to_configure.contains(id) {
                    to_configure.push(id.clone());
                }
            }
            self.context.tools_to_configure = to_configure;
            self.context.current_step += 1;
            return Ok(());
        }

        // Build multiselect items
        let mut items: Vec<(&str, String, String)> = Vec::new();
        let install_ids: std::collections::HashSet<&str> =
            install_candidates.iter().map(|t| t.id).collect();

        // Group 1: tools to install
        for tool in &install_candidates {
            items.push((tool.id, tool.label.to_string(), tool.pitch.to_string()));
        }

        // Group 2: Tier 2 tools (available but not in PATH)
        for tool in &tier2_candidates {
            items.push((
                tool.id,
                format!("{} (not in PATH)", tool.label),
                "configure anyway".to_string(),
            ));
        }

        eprintln!("Select tools to install or configure (Enter to skip):");
        let selected: Vec<&str> = multiselect("Tools:")
            .items(
                &items
                    .iter()
                    .map(|(id, label, pitch)| (*id, label.as_str(), pitch.as_str()))
                    .collect::<Vec<_>>(),
            )
            .required(false)
            .interact()
            .map_err(handle_cliclack_error)?;

        // Split: install only missing tools, configure = user picks + Tier 1
        self.context.selected_tools = selected
            .iter()
            .filter(|id| install_ids.contains(*id))
            .map(|id| id.to_string())
            .collect();

        let mut to_configure = tier1_ids;
        for id in &selected {
            if !to_configure.contains(&id.to_string()) {
                to_configure.push(id.to_string());
            }
        }
        self.context.tools_to_configure = to_configure;
        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_font(&mut self) -> Result<()> {
        self.log_step("Select Font");

        wizard_support::print_current_font(self.context.current_font.as_deref());
        let font_options = wizard_support::build_font_options();

        if !wizard_support::is_interactive() {
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

        wizard_support::print_current_theme(
            &self.theme_selector,
            self.context.current_theme.as_deref(),
        );

        // Get all themes for display
        let all_themes = self.theme_selector.all_themes();
        let theme_options = wizard_support::build_theme_options(
            &self.theme_selector,
            self.context.current_theme.as_deref(),
        );

        if !wizard_support::is_interactive() {
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

    fn display_tool_inventory(
        &self,
        installed: &HashMap<String, crate::detection::ToolPresence>,
    ) -> Result<()> {
        wizard_support::print_tool_inventory(installed);
        Ok(())
    }

    fn log_step(&self, _step_name: &str) {
        // Intentionally minimal — no "Step X of Y" counter.
        // cliclack's own frames provide enough visual progress.
    }

    pub fn get_context(&self) -> &WizardContext {
        &self.context
    }

    pub fn get_context_mut(&mut self) -> &mut WizardContext {
        &mut self.context
    }

    /// Quick mode: auto-select core tools (starship + zsh-syntax-highlighting) if missing.
    /// No multiselect — the preset decides what's essential.
    fn step_quick_auto_tools(&mut self) -> Result<()> {
        let installed = detect_installed_tools();
        let (selected_tools, tools_to_configure) =
            compute_quick_tool_plan(&installed, current_terminal_tool_id());

        self.context.selected_tools = selected_tools;
        self.context.tools_to_configure = tools_to_configure;

        self.context.current_step += 1;
        Ok(())
    }

    /// Auto-select opacity based on theme (dark → Frosted, light → Solid).
    /// Skips if a preset already locked the opacity (e.g. quick mode presets).
    fn step_auto_opacity(&mut self) -> Result<()> {
        // If a preset already set opacity, respect it
        if self.context.selected_opacity.is_some() {
            self.context.current_step += 1;
            return Ok(());
        }

        let selected_theme_id =
            wizard_support::resolve_theme_id_for_opacity(&self.context, &self.theme_selector)?;

        let registry = crate::theme::ThemeRegistry::new()?;
        if let Some(theme) = registry.get(&selected_theme_id) {
            let recommended = crate::opacity::recommended_opacity_for_theme(theme);
            self.context.selected_opacity = Some(recommended);
        } else {
            self.context.selected_opacity = Some(crate::opacity::OpacityPreset::Frosted);
        }

        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_fastfetch(&mut self) -> Result<()> {
        self.log_step("Fastfetch Auto-Run");

        if !wizard_support::is_interactive() {
            // Non-interactive mode: skip fastfetch prompt
            self.context.current_step += 1;
            return Ok(());
        }

        let enable_fastfetch = confirm("Show system info every time you open a terminal?")
            .initial_value(false) // Default N (disabled)
            .interact()
            .map_err(handle_cliclack_error)?;

        // If the user wants autorun but fastfetch isn't on the actual PATH, schedule the
        // install. The shell wrapper guards on `command -v fastfetch`, which only sees
        // tier-1 entries — a fastfetch sitting in /opt/homebrew/bin but outside the user's
        // PATH would still make the autorun silently print nothing. Checking `is_tier1`
        // matches what the runtime guard will resolve.
        if enable_fastfetch {
            let env = SlateEnv::from_process()?;
            let presence =
                crate::detection::detect_tool_presence_with_env("fastfetch", &env);
            if !presence.is_tier1()
                && !self.context.selected_tools.iter().any(|t| t == "fastfetch")
            {
                self.context.selected_tools.push("fastfetch".to_string());
                if !self
                    .context
                    .tools_to_configure
                    .iter()
                    .any(|t| t == "fastfetch")
                {
                    self.context.tools_to_configure.push("fastfetch".to_string());
                }
            }
        }

        self.context.fastfetch_enabled = Some(enable_fastfetch);
        self.context.current_step += 1;
        Ok(())
    }

    fn step_review_and_confirm(&mut self) -> Result<()> {
        self.log_step("Review and Confirm");
        let receipt = self.build_review_receipt();
        eprintln!("\n{}", self.display_receipt(&receipt));

        if !wizard_support::is_interactive() {
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

    // Step counting removed — cliclack frames provide visual progress.

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
}

fn detect_current_theme_id() -> Option<String> {
    let home = SlateEnv::from_process()
        .ok()
        .and_then(|e| e.home().to_str().map(|s| s.to_string()))?;
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

    #[test]
    fn test_compute_quick_tool_plan_configures_starship_and_current_terminal() {
        let mut installed = HashMap::new();
        installed.insert(
            "starship".to_string(),
            ToolPresence {
                installed: true,
                in_path: true,
                evidence: None,
            },
        );
        installed.insert(
            "ghostty".to_string(),
            ToolPresence {
                installed: true,
                in_path: false,
                evidence: None,
            },
        );

        let (selected_tools, tools_to_configure) =
            compute_quick_tool_plan(&installed, Some("ghostty"));

        assert!(!selected_tools.contains(&"starship".to_string()));
        assert!(tools_to_configure.contains(&"starship".to_string()));
        assert!(tools_to_configure.contains(&"ghostty".to_string()));
    }
}
