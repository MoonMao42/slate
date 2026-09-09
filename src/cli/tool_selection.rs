/// Tool selection for setup wizard.
/// Single source of truth for tool metadata, installability, and selection logic.
use crate::brand::language::Language;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::detection::{self, TerminalProfile, ToolPresence};
use crate::env::SlateEnv;
use std::collections::HashMap;

mod install_plan;
mod quick;
pub(crate) use install_plan::{InstallPlan, PlannedToolInstall};
pub(crate) use quick::{core_tools as quick_core_tools, plan as quick_tool_plan};

// Keep the existing catalog API while the installation type belongs to the
// shared platform executor, not the interactive wizard.
pub use crate::platform::packages::BrewKind;

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

const ALL_TOOLS: [ToolMetadata; 14] = [
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
        id: "btop",
        label: "btop",
        pitch: "System monitor in your theme · reopen btop after changes",
        installable: true,
        brew_package: "btop",
        brew_kind: BrewKind::Formula,
        detect_only: false,
    },
    ToolMetadata {
        id: "yazi",
        label: "Yazi",
        pitch: "File manager and code previews in your theme · reopen after changes",
        installable: true,
        brew_package: "yazi",
        brew_kind: BrewKind::Formula,
        detect_only: false,
    },
    ToolMetadata {
        id: "zellij",
        label: "Zellij",
        pitch: "Split panes, tabs and sessions in your theme",
        installable: true,
        brew_package: "zellij",
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
    /// Requested configuration targets, distinct from package installations.
    pub tools_to_configure: Vec<String>,
    /// Selected font name (if any)
    pub selected_font: Option<String>,
    /// Selected theme (if any)
    pub selected_theme: Option<String>,
    /// Terminal visual settings (if any)
    pub terminal_settings: Option<TerminalSettings>,
    /// Actual saved appearance request, including manual setup's inferred value.
    pub selected_opacity: Option<crate::opacity::OpacityPreset>,
    /// None means setup leaves this preference untouched (quick mode).
    pub fastfetch_enabled: Option<bool>,
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
            tools_to_configure: Vec::new(),
            selected_font: None,
            selected_theme: None,
            terminal_settings: None,
            selected_opacity: None,
            fastfetch_enabled: None,
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
        self.format_for_display_with_terminal(r, &TerminalProfile::detect())
    }

    /// Like `format_for_display_with_roles` but with an injected `TerminalProfile`,
    /// so snapshot tests aren't sensitive to the runner's `$TERM_PROGRAM`.
    pub(crate) fn format_for_display_with_terminal(
        &self,
        r: Option<&Roles<'_>>,
        terminal: &TerminalProfile,
    ) -> String {
        self.format_with_install_plan(r, terminal, None)
    }

    /// Runtime wizard rendering uses the captured installation plan. The legacy
    /// catalog-only renderer remains available for callers without a bound plan.
    pub(crate) fn format_with_install_plan(
        &self,
        r: Option<&Roles<'_>>,
        terminal: &TerminalProfile,
        installs: Option<&InstallPlan>,
    ) -> String {
        use super::wizard_support::wording;
        let language = super::ui_language::output_language();
        let mut output = String::new();

        // Heading: "◆ Review and confirm" via Roles::heading. Mirrors
        // sketch 003 tree narrative anchor.
        output.push_str(&render_heading(
            r,
            wording("检查设置", "Review and confirm"),
        ));
        output.push_str("\n\n");

        if !self.install_actions.is_empty() {
            output.push_str(&render_heading(r, wording("安装", "Install")));
            output.push('\n');
            for action in &self.install_actions {
                let kind_str = match action.brew_kind {
                    BrewKind::Formula => "formula",
                    BrewKind::Cask => "cask",
                };
                let description = installs.map_or_else(
                    || kind_str.to_owned(),
                    |plan| {
                        plan.description_in(&action.tool_id, language)
                            .unwrap_or_else(|| "not included in captured installation plan".into())
                    },
                );
                let line = format!("• {} — {}", action.tool_label, description);
                output.push_str(&match r {
                    Some(r) => r.tree_branch(&line),
                    None => format!("  {}", line),
                });
                output.push('\n');
            }
            if let Some(fallback) = installs.and_then(|plan| plan.fallback_description_in(language))
            {
                output.push_str(&format!("  {fallback}\n"));
            }
            if installs.is_some() {
                output.push_str(wording(
                    "  备份仅包含配置文件，不包含软件包或已安装的可执行文件。\n",
                    "  Backups cover configuration files, not packages or installed executables.\n",
                ));
            }
            output.push('\n');
        }

        if !self.tools_to_configure.is_empty() {
            let mut ids = self
                .tools_to_configure
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            ids.sort_unstable();
            ids.dedup();
            let labels = ids
                .into_iter()
                .map(|id| {
                    let label = ToolCatalog::get_tool(id).map_or(id, |tool| tool.label);
                    super::file_output::terminal_text(label)
                })
                .collect::<Vec<_>>();
            output.push_str(&Language::receipt_line(
                wording("计划配色", "Configure colors"),
                &labels.join(" · "),
            ));
            output.push('\n');
        }

        if let Some(font) = &self.selected_font {
            output.push_str(&Language::receipt_line(
                wording("字体", Language::RECEIPT_FONT_SECTION),
                &super::file_output::terminal_text(font),
            ));
            output.push('\n');
        }

        if let Some(theme) = &self.selected_theme {
            output.push_str(&Language::receipt_line(
                wording("主题", Language::RECEIPT_THEME_SECTION),
                &super::file_output::terminal_text(theme),
            ));
            output.push('\n');
        }

        let appearance = self
            .selected_opacity
            .map(|opacity| (opacity.to_f32(), opacity.blur_radius() > 0))
            .or_else(|| {
                self.terminal_settings
                    .as_ref()
                    .map(|settings| (settings.background_opacity, settings.blur_enabled))
            });
        let terminal_summary = appearance
            .map(|(opacity, blur)| terminal.setup_review_summary_in(Some(opacity), blur, language))
            .unwrap_or_else(|| {
                format!(
                    "{} · {}",
                    terminal.display_name(),
                    terminal.compatibility_label_in(language)
                )
            });
        output.push_str(&Language::receipt_line(
            wording("终端", Language::RECEIPT_TERMINAL_SECTION),
            &super::file_output::terminal_text(&terminal_summary),
        ));
        output.push('\n');

        if let Some(enabled) = self.fastfetch_enabled {
            output.push_str(&Language::receipt_line(
                wording("启动系统信息", "Startup system info"),
                if enabled {
                    wording("显示", "On")
                } else {
                    wording("不显示", "Off")
                },
            ));
            output.push('\n');
        }

        // Footer hint — path-role (dim italic, no container) per sketch 003.
        output.push('\n');
        let footer = wording("确认后先备份配置，再执行设置。", Language::RECEIPT_FOOTER);
        output.push_str(&match r {
            Some(r) => format!("  {}", r.path(footer)),
            None => format!("  {}", footer),
        });
        output.push('\n');

        output
    }
}

/// Render `◆ title` via Roles::heading, or a plain fallback when ctx is
/// unavailable (registry init failure — graceful degrade).
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

/// Missing catalog tools that this platform has a concrete installation route
/// for. Existing installed/configuration-only tools are handled independently.
pub(crate) fn compute_install_candidates_for_platform(
    installed: &HashMap<String, ToolPresence>,
    context: crate::platform::packages::InstallContext,
) -> Vec<ToolMetadata> {
    compute_install_candidates(installed)
        .into_iter()
        .filter(|tool| context.route(tool.id).is_ok())
        .collect()
}

/// Recheck final installation intent before snapshot/preferences/installers.
/// Catalog validity is checked separately by setup preparation. Never silently
/// remove a requested tool just because its installation route is unavailable.
pub(crate) fn validate_install_routes(
    selected_ids: &[String],
    context: crate::platform::packages::InstallContext,
) -> crate::error::Result<()> {
    for id in selected_ids {
        context.route(id).map_err(|unavailable| crate::error::SlateError::InvalidConfig(format!(
            "Cannot automatically install '{}': {}. Use Manual setup to configure existing tools, or provide the missing installation route before retrying.",
            id.escape_default(), unavailable.reason(),
        )))?;
    }
    Ok(())
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
    fn bootstrap_candidates_only_offer_real_routes_without_changing_presence() {
        use crate::platform::packages::{InstallContext, PackageManagerBackend};
        let mut installed = HashMap::new();
        let context = InstallContext {
            package_manager: PackageManagerBackend::Unsupported,
            supported_os: true,
        };
        let candidates = compute_install_candidates_for_platform(&installed, context);
        assert_eq!(
            candidates.iter().map(|tool| tool.id).collect::<Vec<_>>(),
            ["starship"]
        );
        installed.insert(
            "bat".into(),
            ToolPresence {
                installed: true,
                in_path: false,
                evidence: None,
            },
        );
        assert_eq!(
            compute_install_candidates_for_platform(&installed, context).len(),
            1
        );
        assert!(installed["bat"].installed && !installed["bat"].in_path);
        installed.insert(
            "starship".into(),
            ToolPresence {
                installed: true,
                in_path: true,
                evidence: None,
            },
        );
        assert!(compute_install_candidates_for_platform(&installed, context).is_empty());
        assert!(compute_install_candidates_for_platform(
            &HashMap::new(),
            InstallContext {
                supported_os: false,
                ..context
            }
        )
        .is_empty());
        for backend in [PackageManagerBackend::Homebrew, PackageManagerBackend::Apt] {
            let candidates = compute_install_candidates_for_platform(
                &HashMap::new(),
                InstallContext {
                    package_manager: backend,
                    ..context
                },
            );
            let expected = ToolCatalog::installable_tools()
                .into_iter()
                .filter(|tool| {
                    InstallContext {
                        package_manager: backend,
                        ..context
                    }
                    .route(tool.id)
                    .is_ok()
                })
                .map(|tool| tool.id)
                .collect::<Vec<_>>();
            assert_eq!(
                candidates.iter().map(|tool| tool.id).collect::<Vec<_>>(),
                expected
            );
            assert!(candidates.iter().all(|tool| !tool.detect_only));
        }
    }

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
    fn review_receipt_escapes_font_and_theme_control_characters() {
        let mut receipt = ReviewReceipt::new();
        receipt.selected_font = Some("Personal\n\x1b[2JFont".into());
        // ANSI-FIXTURE: raw input for escaping or width checks.
        receipt.selected_theme = Some("Theme\r\x1b[31mName".into());
        let formatted = receipt.format_for_display();
        assert!(!formatted.contains('\x1b'));
        assert!(!formatted.contains("Personal\n"));
        assert!(!formatted.contains("Theme\r"));
        assert!(formatted.contains("Personal") && formatted.contains("Font"));
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

    /// snapshot — the `ReviewReceipt` format body routed through
    /// a MockTheme-backed `Roles`. Locks the `◆ heading + ┃ ├─` tree
    /// narrative for the review pane. Deterministic bytes per.
    #[test]
    fn tool_selection_review_receipt_basic_mode_snapshot() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        // Basic mode so the snapshot is stable even without a theme-derived
        // pill bg — and more importantly so the receipt's tree
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

        // Pin the terminal so the snapshot doesn't drift with the runner's
        // $TERM_PROGRAM (CI is `Other terminal`, dev is `Ghostty`).
        let terminal = detection::TerminalProfile::from_env_vars(Some("ghostty"), None);
        let out = receipt.format_for_display_with_terminal(Some(&r), &terminal);
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
    /// receipt still renders (no styling, plain text). graceful
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
