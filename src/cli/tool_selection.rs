/// Tool selection for setup wizard.
/// Single source of truth for tool metadata, installability, and selection logic.
use crate::brand::language::Language;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::detection::{self, TerminalProfile, ToolPresence};
use crate::env::SlateEnv;
use std::collections::HashMap;

/// Brew installation kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrewKind {
    Formula,
    Cask,
}

/// Tool metadata: single source of truth for wizard-managed tools.
#[derive(Debug, Clone, Copy)]
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

const ALL_TOOLS: [ToolMetadata; 11] = [
    ToolMetadata {
        id: "ghostty",
        label: "Ghostty",
        pitch: Language::PITCH_GHOSTTY,
        installable: false,
        brew_package: "ghostty",
        brew_kind: BrewKind::Cask,
        detect_only: true,
    },
    ToolMetadata {
        id: "starship",
        label: "Starship",
        pitch: Language::PITCH_STARSHIP,
        installable: true,
        brew_package: "starship",
        brew_kind: BrewKind::Formula,
        detect_only: false,
    },
    ToolMetadata {
        id: "bat",
        label: "bat",
        pitch: Language::PITCH_BAT,
        installable: true,
        brew_package: "bat",
        brew_kind: BrewKind::Formula,
        detect_only: false,
    },
    ToolMetadata {
        id: "delta",
        label: "delta",
        pitch: Language::PITCH_DELTA,
        installable: true,
        brew_package: "delta",
        brew_kind: BrewKind::Formula,
        detect_only: false,
    },
    ToolMetadata {
        id: "eza",
        label: "eza",
        pitch: Language::PITCH_EZA,
        installable: true,
        brew_package: "eza",
        brew_kind: BrewKind::Formula,
        detect_only: false,
    },
    ToolMetadata {
        id: "lazygit",
        label: "lazygit",
        pitch: Language::PITCH_LAZYGIT,
        installable: true,
        brew_package: "lazygit",
        brew_kind: BrewKind::Formula,
        detect_only: false,
    },
    ToolMetadata {
        id: "fastfetch",
        label: "fastfetch",
        pitch: Language::PITCH_FASTFETCH,
        installable: true,
        brew_package: "fastfetch",
        brew_kind: BrewKind::Formula,
        detect_only: false,
    },
    ToolMetadata {
        id: "zsh-syntax-highlighting",
        label: "zsh-syntax-highlighting",
        pitch: Language::PITCH_ZSH_SYNTAX,
        installable: true,
        brew_package: "zsh-syntax-highlighting",
        brew_kind: BrewKind::Formula,
        detect_only: false,
    },
    ToolMetadata {
        id: "alacritty",
        label: "Alacritty",
        pitch: Language::PITCH_ALACRITTY,
        installable: false,
        brew_package: "alacritty",
        brew_kind: BrewKind::Cask,
        detect_only: true,
    },
    ToolMetadata {
        id: "kitty",
        label: "Kitty",
        pitch: Language::PITCH_KITTY,
        installable: false,
        brew_package: "kitty",
        brew_kind: BrewKind::Cask,
        detect_only: true,
    },
    ToolMetadata {
        id: "tmux",
        label: "tmux",
        pitch: Language::PITCH_TMUX,
        installable: false,
        brew_package: "",
        brew_kind: BrewKind::Formula,
        detect_only: true,
    },
];

/// Central registry of all tools managed by + setup.
/// This is the source of truth for tool selection, inventory, and installation.
pub struct ToolCatalog;

impl ToolCatalog {
    /// Get all tools managed by setup wizard
    pub fn all_tools() -> &'static [ToolMetadata] {
        &ALL_TOOLS
    }

    /// Get a tool by id
    pub fn get_tool(id: &str) -> Option<ToolMetadata> {
        Self::all_tools().iter().copied().find(|t| t.id == id)
    }

    /// Get all installable tools (excludes detect-only)
    pub fn installable_tools() -> Vec<ToolMetadata> {
        Self::all_tools()
            .iter()
            .copied()
            .filter(|t| t.installable)
            .collect()
    }

    /// Get detect-only tools
    pub fn detect_only_tools() -> Vec<ToolMetadata> {
        Self::all_tools()
            .iter()
            .copied()
            .filter(|t| t.detect_only)
            .collect()
    }
}

/// Detect installation state for all wizard-managed tools using the shared presence resolver.
pub fn detect_installed_tools() -> HashMap<String, ToolPresence> {
    SlateEnv::from_process()
        .map(|env| detect_installed_tools_with_env(&env))
        .unwrap_or_default()
}

/// Detect installation state for all wizard-managed tools with injected SlateEnv.
pub fn detect_installed_tools_with_env(env: &SlateEnv) -> HashMap<String, ToolPresence> {
    ToolCatalog::all_tools()
        .iter()
        .copied()
        .map(|tool| {
            (
                tool.id.to_string(),
                detection::detect_tool_presence_with_env(tool.id, env),
            )
        })
        .collect()
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

impl Default for ReviewReceipt {
    fn default() -> Self {
        Self::new()
    }
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

    /// Format receipt as human-readable string for display using the Roles API.
    pub fn format_for_display(&self) -> String {
        // Build a RenderContext up-front so the whole receipt renders through
        // the same ctx. Registry init failure is a graceful-degrade path
        // (plain text, no ANSI) — the wizard must still print something.
        let ctx = RenderContext::from_active_theme().ok();
        let r = ctx.as_ref().map(Roles::new);
        self.format_for_display_with_roles(r.as_ref())
    }

    /// Shared body — factored so snapshot tests can inject a mock `Roles`
    /// without going through registry init.
    pub(crate) fn format_for_display_with_roles(&self, r: Option<&Roles<'_>>) -> String {
        let mut output = String::new();
        let terminal = TerminalProfile::detect();

        // Heading: "◆ Review and confirm" via Roles::heading. Mirrors
        // sketch 003 tree narrative anchor.
        output.push_str(&render_heading(r, "Review and confirm"));
        output.push_str("\n\n");

        if !self.install_actions.is_empty() {
            output.push_str(&render_heading(r, "Install"));
            output.push('\n');
            for action in &self.install_actions {
                let kind_str = match action.brew_kind {
                    BrewKind::Formula => "formula",
                    BrewKind::Cask => "cask",
                };
                let line = format!("• {} — {}", action.tool_label, kind_str);
                output.push_str(&match r {
                    Some(r) => r.tree_branch(&line),
                    None => format!("  {}", line),
                });
                output.push('\n');
            }
            output.push('\n');
        }

        if let Some(font) = &self.selected_font {
            output.push_str(&Language::receipt_line(
                Language::RECEIPT_FONT_SECTION,
                font,
            ));
            output.push('\n');
        }

        if let Some(theme) = &self.selected_theme {
            output.push_str(&Language::receipt_line(
                Language::RECEIPT_THEME_SECTION,
                theme,
            ));
            output.push('\n');
        }

        let terminal_summary = self
            .terminal_settings
            .as_ref()
            .map(|settings| {
                terminal
                    .setup_review_summary(Some(settings.background_opacity), settings.blur_enabled)
            })
            .unwrap_or_else(|| {
                format!(
                    "{} · {}",
                    terminal.display_name(),
                    terminal.compatibility_label()
                )
            });
        output.push_str(&Language::receipt_line(
            Language::RECEIPT_TERMINAL_SECTION,
            &terminal_summary,
        ));
        output.push('\n');

        // Footer hint — path-role (dim italic, no container) per sketch 003.
        output.push('\n');
        output.push_str(&match r {
            Some(r) => format!("  {}", r.path(Language::RECEIPT_FOOTER)),
            None => format!("  {}", Language::RECEIPT_FOOTER),
        });
        output.push('\n');

        output
    }
}

/// Render `◆ title` via Roles::heading, or a plain fallback when ctx is
/// unavailable (registry init failure — graceful degrade, D-05).
fn render_heading(r: Option<&Roles<'_>>, title: &str) -> String {
    match r {
        Some(r) => r.heading(title),
        None => format!("◆ {title}"),
    }
}

/// Install candidates: missing tools that are installable (used for multiselect)
pub fn compute_install_candidates(installed: &HashMap<String, ToolPresence>) -> Vec<ToolMetadata> {
    ToolCatalog::installable_tools()
        .into_iter()
        .filter(|tool| {
            // Include tool if NOT installed
            !installed.get(tool.id).map(|p| p.installed).unwrap_or(false)
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
        assert!(tools.len() >= 11);
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
        installed.insert(
            "ghostty".to_string(),
            ToolPresence::in_path_with(crate::detection::ToolEvidence::Executable(
                "/usr/bin/ghostty".into(),
            )),
        );
        installed.insert("starship".to_string(), ToolPresence::missing());

        let candidates = compute_install_candidates(&installed);

        assert!(!candidates.iter().any(|t| t.id == "ghostty"));
        assert!(candidates.iter().any(|t| t.id == "starship"));
    }

    #[test]
    fn test_install_candidates_excludes_detect_only() {
        let mut installed = HashMap::new();
        installed.insert("tmux".to_string(), ToolPresence::missing());

        let candidates = compute_install_candidates(&installed);
        assert!(!candidates.iter().any(|t| t.id == "tmux"));
    }

    #[test]
    fn test_filter_valid_selections() {
        let selected = vec![
            "starship".to_string(),
            "tmux".to_string(),
            "ghostty".to_string(), // detect-only, should be excluded
            "unknown".to_string(),
        ];
        let actions = filter_valid_selections(selected);

        assert!(actions.iter().any(|a| a.tool_id == "starship"));
        assert!(!actions.iter().any(|a| a.tool_id == "tmux"));
        assert!(!actions.iter().any(|a| a.tool_id == "ghostty")); // detect-only
        assert!(!actions.iter().any(|a| a.tool_id == "unknown"));
    }

    #[test]
    fn test_install_action_from_metadata() {
        let metadata = ToolMetadata {
            id: "starship",
            label: "Starship",
            pitch: "pitch",
            installable: true,
            brew_package: "starship",
            brew_kind: BrewKind::Formula,
            detect_only: false,
        };
        let action = InstallAction::from_metadata(&metadata);

        assert_eq!(action.tool_id, "starship");
        assert_eq!(action.tool_label, "Starship");
        assert_eq!(action.brew_package, "starship");
        assert_eq!(action.brew_kind, BrewKind::Formula);
    }

    #[test]
    fn test_ghostty_uses_cask_install() {
        let ghostty = ToolCatalog::get_tool("ghostty").expect("ghostty should exist");
        assert_eq!(ghostty.brew_kind, BrewKind::Cask);
    }

    #[test]
    fn test_review_receipt_format() {
        let mut receipt = ReviewReceipt::new();
        receipt.selected_font = Some("JetBrains Mono".to_string());
        receipt.selected_theme = Some("Catppuccin Mocha".to_string());

        let formatted = receipt.format_for_display();
        assert!(formatted.contains("JetBrains Mono"));
        assert!(formatted.contains("Catppuccin Mocha"));
        assert!(formatted.contains("Review"));
        assert!(formatted.contains("Terminal"));
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

    /// Wave 1 snapshot — the `ReviewReceipt` format body routed through
    /// a MockTheme-backed `Roles`. Locks the `◆ heading + ┃ ├─` tree
    /// narrative for the review pane. Deterministic bytes per D-06.
    #[test]
    fn tool_selection_review_receipt_basic_mode_snapshot() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        // Basic mode so the snapshot is stable even without a theme-derived
        // D-04 pill bg — and more importantly so the receipt's tree
        // narrative is byte-identical across CI truecolor / contributor
        // truecolor / non-truecolor runners.
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);

        let mut receipt = ReviewReceipt::new();
        receipt.selected_font = Some("JetBrains Mono".to_string());
        receipt.selected_theme = Some("Catppuccin Mocha".to_string());
        receipt.install_actions.push(InstallAction {
            tool_id: "starship".to_string(),
            tool_label: "Starship".to_string(),
            brew_package: "starship".to_string(),
            brew_kind: BrewKind::Formula,
        });
        receipt.install_actions.push(InstallAction {
            tool_id: "bat".to_string(),
            tool_label: "bat".to_string(),
            brew_package: "bat".to_string(),
            brew_kind: BrewKind::Formula,
        });

        let out = receipt.format_for_display_with_roles(Some(&r));
        insta::assert_snapshot!("tool_selection_review_receipt_basic", out);
    }

    /// The `◆` anchor + `Review and confirm` label must land in the
    /// output regardless of render mode. Truecolor wraps the diamond in
    /// ANSI so we assert the anchors separately.
    #[test]
    fn review_receipt_uses_diamond_heading_anchor() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let receipt = ReviewReceipt::new();
            let out = receipt.format_for_display_with_roles(Some(&r));
            assert!(
                out.contains('◆'),
                "missing diamond in mode {mode:?}: {out:?}"
            );
            assert!(
                out.contains("Review and confirm"),
                "missing `Review and confirm` prose in mode {mode:?}: {out:?}"
            );
        }
    }

    /// Registry-init failure fallback — when no `Roles` can be built, the
    /// receipt still renders (no styling, plain text). D-05 graceful
    /// degradation contract.
    #[test]
    fn review_receipt_falls_back_to_plain_when_roles_absent() {
        let mut receipt = ReviewReceipt::new();
        receipt.selected_font = Some("JetBrains Mono".to_string());
        receipt.selected_theme = Some("Catppuccin Mocha".to_string());

        let out = receipt.format_for_display_with_roles(None);
        // Fallback path produces plain `◆ Review and confirm` (no ANSI),
        // so the adjacency check is safe here.
        assert!(out.contains("◆ Review and confirm"));
        assert!(out.contains("JetBrains Mono"));
        assert!(out.contains("Catppuccin Mocha"));
        assert!(
            !out.contains('\x1b'),
            "fallback must emit zero ANSI bytes, got: {out:?}"
        );
    }
}
