/// Tool selection for setup wizard.
/// Single source of truth for tool metadata, installability, and selection logic.

use crate::brand::language::Language;
use std::collections::HashMap;

/// Brew installation kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrewKind {
    Formula,
    Cask,
}

/// Tool metadata: single source of truth for wizard-managed tools.
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    /// Stable tool identifier (e.g., "ghostty", "starship", "bat")
    pub id: &'static str,
    /// Display label (may differ from id for branded names)
    pub label: &'static str,
    /// One-line pitch for user display
    pub pitch: &'static str,
    /// Whether this tool can be installed via setup wizard
    pub installable: bool,
    /// Homebrew package name
    pub brew_package: &'static str,
    /// Installation kind (formula vs cask)
    pub brew_kind: BrewKind,
    /// Whether tool is detect-only (e.g., tmux) — shown in inventory but not offered for install
    pub detect_only: bool,
}

impl ToolMetadata {
    /// Create metadata from parts
    fn new(
        id: &'static str,
        label: &'static str,
        pitch: &'static str,
        installable: bool,
        brew_package: &'static str,
        brew_kind: BrewKind,
        detect_only: bool,
    ) -> Self {
        Self {
            id,
            label,
            pitch,
            installable,
            brew_package,
            brew_kind,
            detect_only,
        }
    }

    /// Create formula install metadata
    fn formula(
        id: &'static str,
        label: &'static str,
        pitch: &'static str,
        brew_package: &'static str,
    ) -> Self {
        Self::new(id, label, pitch, true, brew_package, BrewKind::Formula, false)
    }

    /// Create cask install metadata
    fn cask(
        id: &'static str,
        label: &'static str,
        pitch: &'static str,
        brew_package: &'static str,
    ) -> Self {
        Self::new(id, label, pitch, true, brew_package, BrewKind::Cask, false)
    }

    /// Create detect-only metadata (no installation offered)
    fn detect_only(
        id: &'static str,
        label: &'static str,
        pitch: &'static str,
    ) -> Self {
        Self::new(id, label, pitch, false, "", BrewKind::Formula, true)
    }
}

/// Central registry of all tools managed by + setup.
/// This is the source of truth for tool selection, inventory, and installation.
pub struct ToolCatalog;

impl ToolCatalog {
    /// Get all tools managed by setup wizard
    pub fn all_tools() -> Vec<ToolMetadata> {
        vec![
            // Installable formula-based tools
            ToolMetadata::formula(
                "ghostty",
                "Ghostty",
                Language::PITCH_GHOSTTY,
                "ghostty",
            ),
            ToolMetadata::formula(
                "starship",
                "Starship",
                Language::PITCH_STARSHIP,
                "starship",
            ),
            ToolMetadata::formula(
                "bat",
                "bat",
                Language::PITCH_BAT,
                "bat",
            ),
            ToolMetadata::formula(
                "delta",
                "delta",
                Language::PITCH_DELTA,
                "delta",
            ),
            ToolMetadata::formula(
                "eza",
                "eza",
                Language::PITCH_EZA,
                "eza",
            ),
            ToolMetadata::formula(
                "lazygit",
                "lazygit",
                Language::PITCH_LAZYGIT,
                "lazygit",
            ),
            ToolMetadata::formula(
                "fastfetch",
                "fastfetch",
                Language::PITCH_FASTFETCH,
                "fastfetch",
            ),
            ToolMetadata::formula(
                "zsh-syntax-highlighting",
                "zsh-syntax-highlighting",
                Language::PITCH_ZSH_SYNTAX,
                "zsh-syntax-highlighting",
            ),
            // Cask-based installations
            ToolMetadata::cask(
                "alacritty",
                "Alacritty",
                Language::PITCH_ALACRITTY,
                "alacritty",
            ),
            // Detect-only: synced if installed, not offered for install
            ToolMetadata::detect_only(
                "tmux",
                "tmux",
                Language::PITCH_TMUX,
            ),
        ]
    }

    /// Get a tool by id
    pub fn get_tool(id: &str) -> Option<ToolMetadata> {
        Self::all_tools().into_iter().find(|t| t.id == id)
    }

    /// Get all installable tools (excludes detect-only)
    pub fn installable_tools() -> Vec<ToolMetadata> {
        Self::all_tools()
            .into_iter()
            .filter(|t| t.installable)
            .collect()
    }

    /// Get detect-only tools
    pub fn detect_only_tools() -> Vec<ToolMetadata> {
        Self::all_tools()
            .into_iter()
            .filter(|t| t.detect_only)
            .collect()
    }
}

/// Install action: what to install and how
#[derive(Debug, Clone)]
pub struct InstallAction {
    pub tool_id: String,
    pub tool_label: String,
    pub brew_package: String,
    pub brew_kind: BrewKind,
}

impl InstallAction {
    pub fn from_metadata(metadata: &ToolMetadata) -> Self {
        Self {
            tool_id: metadata.id.to_string(),
            tool_label: metadata.label.to_string(),
            brew_package: metadata.brew_package.to_string(),
            brew_kind: metadata.brew_kind,
        }
    }
}

/// Review receipt: structured action plan for user confirmation
#[derive(Debug, Clone)]
pub struct ReviewReceipt {
    /// Tools to install with their actions
    pub install_actions: Vec<InstallAction>,
    /// Selected font name (if any)
    pub selected_font: Option<String>,
    /// Selected theme (if any)
    pub selected_theme: Option<String>,
    /// Terminal visual settings (if any)
    pub terminal_settings: Option<TerminalSettings>,
}

/// Terminal visual settings applied via theme presets
#[derive(Debug, Clone)]
pub struct TerminalSettings {
    pub background_opacity: f32,
    pub blur_enabled: bool,
    pub padding_x: u32,
    pub padding_y: u32,
}

impl ReviewReceipt {
    pub fn new() -> Self {
        Self {
            install_actions: Vec::new(),
            selected_font: None,
            selected_theme: None,
            terminal_settings: None,
        }
    }

    /// Add an install action to the receipt
    pub fn add_install_action(&mut self, action: InstallAction) {
        self.install_actions.push(action);
    }

    /// Format receipt as human-readable string for display
    pub fn format_for_display(&self) -> String {
        let mut output = String::new();
        output.push_str("✦ Review and confirm:\n\n");

        if !self.install_actions.is_empty() {
            output.push_str("→ Install:\n");
            for action in &self.install_actions {
                let kind_str = match action.brew_kind {
                    BrewKind::Formula => "formula",
                    BrewKind::Cask => "cask",
                };
                output.push_str(&format!("  • {} ({})\n", action.tool_label, kind_str));
            }
            output.push('\n');
        }

        if let Some(font) = &self.selected_font {
            output.push_str(&format!("→ Font: {}\n\n", font));
        }

        if let Some(theme) = &self.selected_theme {
            output.push_str(&format!("→ Theme: {}\n\n", theme));
        }

        if let Some(settings) = &self.terminal_settings {
            output.push_str(&format!("→ Terminal: opacity {}, ", settings.background_opacity));
            if settings.blur_enabled {
                output.push_str("blur enabled, ");
            }
            output.push_str(&format!("padding {}x{}\n\n", settings.padding_x, settings.padding_y));
        }

        output.push_str("Backup current config first? (yes/no)\n");
        output
    }
}

/// Install candidates: missing tools that are installable (used for multiselect)
pub fn compute_install_candidates(
    installed: &HashMap<String, bool>,
) -> Vec<ToolMetadata> {
    ToolCatalog::installable_tools()
        .into_iter()
        .filter(|tool| {
            // Include tool if NOT installed
            !installed.get(tool.id).copied().unwrap_or(false)
        })
        .collect()
}

/// Filter selected tools to ensure only installable tools are included
pub fn filter_valid_selections(selected_ids: Vec<String>) -> Vec<InstallAction> {
    selected_ids
        .into_iter()
        .filter_map(|id| {
            ToolCatalog::get_tool(&id).and_then(|metadata| {
                if metadata.installable {
                    Some(InstallAction::from_metadata(&metadata))
                } else {
                    None
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_catalog_has_tools() {
        let tools = ToolCatalog::all_tools();
        assert!(!tools.is_empty());
        // Expect at least: ghostty, starship, bat, delta, eza, lazygit, fastfetch, zsh-syntax-highlighting, alacritty, tmux
        assert!(tools.len() >= 10);
    }

    #[test]
    fn test_detect_only_tools_not_installable() {
        let detect_only = ToolCatalog::detect_only_tools();
        assert!(!detect_only.is_empty());
        for tool in detect_only {
            assert!(!tool.installable);
            assert!(tool.detect_only);
        }
    }

    #[test]
    fn test_tmux_is_detect_only() {
        let tmux = ToolCatalog::get_tool("tmux");
        assert!(tmux.is_some());
        let tmux = tmux.unwrap();
        assert!(!tmux.installable);
        assert!(tmux.detect_only);
    }

    #[test]
    fn test_install_candidates_excludes_installed() {
        let mut installed = HashMap::new();
        installed.insert("ghostty".to_string(), true);
        installed.insert("starship".to_string(), false);

        let candidates = compute_install_candidates(&installed);

        // ghostty is installed, so should NOT be in candidates
        assert!(!candidates.iter().any(|t| t.id == "ghostty"));

        // starship is not installed, so SHOULD be in candidates
        assert!(candidates.iter().any(|t| t.id == "starship"));
    }

    #[test]
    fn test_install_candidates_excludes_detect_only() {
        let mut installed = HashMap::new();
        installed.insert("tmux".to_string(), false); // tmux not installed

        let candidates = compute_install_candidates(&installed);

        // Even though tmux is not installed, it's detect-only so should NOT be a candidate
        assert!(!candidates.iter().any(|t| t.id == "tmux"));
    }

    #[test]
    fn test_filter_valid_selections() {
        let selected = vec!["ghostty".to_string(), "tmux".to_string(), "unknown".to_string()];
        let actions = filter_valid_selections(selected);

        // ghostty is installable → included
        assert!(actions.iter().any(|a| a.tool_id == "ghostty"));

        // tmux is detect-only → filtered out
        assert!(!actions.iter().any(|a| a.tool_id == "tmux"));

        // unknown doesn't exist → filtered out
        assert!(!actions.iter().any(|a| a.tool_id == "unknown"));
    }

    #[test]
    fn test_install_action_from_metadata() {
        let metadata = ToolMetadata::formula("ghostty", "Ghostty", "pitch", "ghostty");
        let action = InstallAction::from_metadata(&metadata);

        assert_eq!(action.tool_id, "ghostty");
        assert_eq!(action.tool_label, "Ghostty");
        assert_eq!(action.brew_package, "ghostty");
        assert_eq!(action.brew_kind, BrewKind::Formula);
    }

    #[test]
    fn test_review_receipt_format() {
        let mut receipt = ReviewReceipt::new();
        receipt.selected_font = Some("JetBrains Mono".to_string());
        receipt.selected_theme = Some("Catppuccin Mocha".to_string());

        let formatted = receipt.format_for_display();
        assert!(formatted.contains("JetBrains Mono"));
        assert!(formatted.contains("Catppuccin Mocha"));
        assert!(formatted.contains("Review and confirm"));
    }

    #[test]
    fn test_brew_kind_distinction() {
        let formula = BrewKind::Formula;
        let cask = BrewKind::Cask;

        assert_ne!(formula, cask);
    }

    #[test]
    fn test_all_tools_have_metadata() {
        for tool in ToolCatalog::all_tools() {
            assert!(!tool.id.is_empty());
            assert!(!tool.label.is_empty());
            assert!(!tool.pitch.is_empty());
            assert!(!tool.brew_package.is_empty() || tool.detect_only);
        }
    }

    #[test]
    fn test_installable_tools_are_not_detect_only() {
        for tool in ToolCatalog::installable_tools() {
            assert!(!tool.detect_only);
        }
    }
}
