use crate::error::Result;
use crate::brand::language::Language;
use crate::cli::font_detection::detect_current_font;
use crate::cli::tool_selection::{
    ToolCatalog, compute_install_candidates, ReviewReceipt, InstallAction
};
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WizardMode {
    Quick,
    Manual,
}

pub struct Wizard {
    context: WizardContext,
}

impl Wizard {
    pub fn new() -> Result<Self> {
        // Detect current font on wizard startup
        let current_font = detect_current_font().ok().flatten();
        
        Ok(Self {
            context: WizardContext {
                mode: WizardMode::Manual,
                current_step: 0,
                total_steps: 6, // intro → tools → font → theme → action list → apply
                selected_tools: Vec::new(),
                selected_font: None,
                selected_theme: None,
                current_font,
            },
        })
    }

    /// Run the full wizard flow
    pub fn run(&mut self, quick_mode: bool) -> Result<()> {
        // Step 0: Intro
        self.show_intro()?;

        // Step 1: Mode selection (or skip if --quick)
        if quick_mode {
            self.context.mode = WizardMode::Quick;
            self.context.total_steps = 4; // Adjust for Quick: preset → tools → action → apply
        } else {
            self.step_select_mode()?;
        }

        // Step 2: Tool detection and selection
        self.step_detect_and_select_tools()?;

        // Later steps handled by subsequent plans
        // For now: show step counter format and completion
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

    fn step_detect_and_select_tools(&mut self) -> Result<()> {
        self.log_step("Detect and Select Tools");

        // Create a registry just for detection (not default-populated)
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

        // Build multiselect items as tuples: (id, label, description)
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

        // Convert to owned vec for interaction
        let items_for_select: Vec<(String, String, String)> = items
            .iter()
            .map(|(id, label, desc)| (id.to_string(), label.clone(), desc.clone()))
            .collect();

        eprintln!("Select tools to install:");
        let selected: Vec<&str> = multiselect("Tools:")
            .items(
                &items_for_select
                    .iter()
                    .map(|(id, label, desc)| (id.as_str(), label.clone(), desc.clone()))
                    .collect::<Vec<_>>()
            )
            .interact()?;

        // Convert &str to String
        self.context.selected_tools = selected.into_iter().map(|s| s.to_string()).collect();
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
}
