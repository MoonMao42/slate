use crate::error::Result;
use crate::brand::language::Language;
use cliclack::{intro, outro, select};

pub struct WizardContext {
    pub mode: WizardMode,
    pub current_step: usize,
    pub total_steps: usize,
    pub selected_tools: Vec<String>,
    pub selected_font: Option<String>,
    pub selected_theme: Option<String>,
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
        Ok(Self {
            context: WizardContext {
                mode: WizardMode::Manual,
                current_step: 0,
                total_steps: 6, // intro → tools → font → theme → action list → apply
                selected_tools: Vec::new(),
                selected_font: None,
                selected_theme: None,
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

        // Later steps handled by subsequent plans
        // For now: show step counter format and completion
        self.show_completion()?;

        Ok(())
    }

    fn show_intro(&mut self) -> Result<()> {
        // Use cliclack's intro frame (per no custom ASCII art)
        cliclack::intro("✦ slate").ok();
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

    fn show_completion(&mut self) -> Result<()> {
        self.log_step("Complete");
        cliclack::outro(Language::SETUP_COMPLETE).ok();
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
}
