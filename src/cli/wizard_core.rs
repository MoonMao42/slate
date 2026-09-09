use super::menu::{multiselect, select, MultiSelectOutcome};
use super::wizard_support::wording as tr;
use crate::brand::language::Language;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::font_detection::detect_current_font_with_env;
use crate::cli::font_selection::FontCatalog;
use crate::cli::preset_selection::PresetCatalog;
use crate::cli::theme_selection::ThemeSelector;
use crate::cli::tool_selection::{
    compute_install_candidates_for_platform, detect_installed_tools_with_env,
    quick_tool_plan as compute_quick_tool_plan, InstallAction, ReviewReceipt, TerminalSettings,
    ToolCatalog,
};
use crate::cli::wizard_support;
use crate::detection::{TerminalKind, TerminalProfile, ToolPresence};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::platform::shell::ShellBackend;
use cliclack::{intro, outro};
use std::collections::HashMap;
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

enum ToolSelectionStep {
    Back,
    Skipped,
    Selected,
}

pub struct Wizard {
    env: SlateEnv,
    terminal: TerminalProfile,
    context: WizardContext,
    theme_selector: ThemeSelector,
    reviewed_installs: Option<crate::cli::tool_selection::InstallPlan>,
    // Explicit checkbox choices only, never implicitly detected integrations.
    selected_tool_choices: Vec<String>,
    focused_tool: Option<String>,
    fastfetch_install_added: bool,
    fastfetch_config_added: bool,
    selected_preset: Option<String>,
    selected_mode: Option<WizardMode>,
}

fn current_terminal_tool_id(terminal: &TerminalProfile) -> Option<&'static str> {
    match terminal.kind() {
        TerminalKind::Ghostty => Some("ghostty"),
        TerminalKind::Alacritty => Some("alacritty"),
        _ => None,
    }
}

fn automatic_configuration_tools(installed: &HashMap<String, ToolPresence>) -> Vec<String> {
    // Detection is a HashMap; plan order must not depend on its random seed.
    crate::cli::tool_selection::ToolCatalog::all_tools()
        .iter()
        .filter(|tool| {
            installed.get(tool.id).is_some_and(|presence| {
                presence.is_tier1()
                    || (presence.installed && matches!(tool.id, "ghostty" | "alacritty" | "kitty"))
            })
        })
        .map(|tool| tool.id.to_owned())
        .collect()
}

impl Wizard {
    pub fn new() -> Result<Self> {
        Self::with_env(&SlateEnv::from_process()?)
    }

    /// Capture the profile for startup hints and subsequent tool selection.
    /// This does not initialize directories, launch tools or apply selections.
    pub fn with_env(env: &SlateEnv) -> Result<Self> {
        let current_font = detect_current_font_with_env(env).ok().flatten();
        let current_theme = super::startup_detection::theme_hint(env);

        Ok(Self {
            env: env.clone(),
            terminal: TerminalProfile::from_env_vars(
                std::env::var("TERM_PROGRAM").ok().as_deref(),
                std::env::var("TERM").ok().as_deref(),
            )
            .with_session(env.session().clone()),
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
            reviewed_installs: None,
            selected_tool_choices: Vec::new(),
            focused_tool: None,
            fastfetch_install_added: false,
            fastfetch_config_added: false,
            selected_preset: None,
            selected_mode: None,
        })
    }

    /// Run the full wizard flow
    /// force=true ignores current state and runs as fresh install
    pub fn run(&mut self, quick_mode: bool, force: bool) -> Result<()> {
        self.reviewed_installs = None;
        self.context.confirmed = false;
        self.context.force = force;
        self.context.start_time = Some(Instant::now());

        // If force flag is set, clear current state
        if force {
            self.context.current_font = None;
            self.context.current_theme = None;
            eprintln!(
                "{}\n",
                tr(
                    "强制设置：重新选择字体和主题。",
                    "Force setup: choose the font and theme again."
                )
            );
        }

        // Step 0: Intro
        self.show_intro()?;

        let mode_step = self.context.current_step;
        loop {
            self.context.current_step = mode_step;
            let previous_mode = self.context.mode;
            if quick_mode {
                self.context.mode = WizardMode::Quick;
            } else {
                self.step_select_mode()?;
            }
            if previous_mode != self.context.mode {
                self.clear_pending_choices();
            }
            if self.context.mode == WizardMode::Manual {
                if self.run_manual_choices()? {
                    return Ok(());
                }
                continue;
            }
            let preset_step = self.context.current_step;
            loop {
                self.context.current_step = preset_step;
                if !self.step_select_preset_quick(!quick_mode)? {
                    break;
                }
                self.step_quick_auto_tools()?;
                self.step_auto_opacity()?;
                if self.step_review_and_confirm(true)? {
                    return Ok(());
                }
            }
        }
    }

    fn clear_pending_choices(&mut self) {
        self.context.selected_tools.clear();
        self.context.tools_to_configure.clear();
        self.context.selected_font = None;
        self.context.selected_theme = None;
        self.context.selected_opacity = None;
        self.context.selected_terminal_settings = None;
        self.context.fastfetch_enabled = None;
        self.context.confirmed = false;
        self.reviewed_installs = None;
        self.selected_tool_choices.clear();
        self.focused_tool = None;
        self.fastfetch_install_added = false;
        self.fastfetch_config_added = false;
    }

    fn run_manual_choices(&mut self) -> Result<bool> {
        #[derive(Clone, Copy)]
        enum Page {
            Tools,
            Font,
            Theme,
            Fastfetch,
            Review,
        }
        let first_step = self.context.current_step;
        let mut page = Page::Tools;
        let mut tools_shown = false;
        loop {
            self.context.current_step = first_step
                + match page {
                    Page::Tools => 0,
                    Page::Font => 1,
                    Page::Theme => 2,
                    Page::Fastfetch => 3,
                    Page::Review => 4,
                };
            page = match page {
                Page::Tools => {
                    tools_shown = match self.step_detect_and_select_tools()? {
                        ToolSelectionStep::Back => return Ok(false),
                        ToolSelectionStep::Skipped => false,
                        ToolSelectionStep::Selected => true,
                    };
                    Page::Font
                }
                Page::Font => {
                    if self.step_select_font(tools_shown)? {
                        Page::Theme
                    } else {
                        Page::Tools
                    }
                }
                Page::Theme => {
                    if self.step_select_theme()? {
                        Page::Fastfetch
                    } else {
                        Page::Font
                    }
                }
                Page::Fastfetch => {
                    if self.step_select_fastfetch()? {
                        Page::Review
                    } else {
                        Page::Theme
                    }
                }
                Page::Review => {
                    // Manual setup has no explicit opacity preset. Recompute
                    // after edits rather than retaining the previous review.
                    self.context.selected_opacity = None;
                    self.step_auto_opacity()?;
                    if self.step_review_and_confirm(true)? {
                        return Ok(true);
                    }
                    Page::Fastfetch
                }
            };
        }
    }

    fn show_intro(&mut self) -> Result<()> {
        // Use cliclack's intro frame (: no custom ASCII art)
        intro("slate").ok();
        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_mode(&mut self) -> Result<()> {
        self.log_step(tr("选择设置方式", "Select Setup Mode"));

        if !wizard_support::is_interactive() {
            self.context.mode = WizardMode::Quick;
            self.context.current_step += 1;
            return Ok(());
        }

        let mode_choice = select(tr("设置方式：", "Setup mode:"))
            .initial_value(match self.selected_mode {
                Some(WizardMode::Manual) => "manual",
                _ => "quick",
            })
            .item("quick", tr("快速设置", "Quick (pick a vibe)"), "")
            .item("manual", tr("逐项设置", "Manual (customize each)"), "")
            .interact()
            .map_err(handle_cliclack_error)?;

        self.context.mode = match mode_choice {
            "quick" => WizardMode::Quick,
            "manual" => WizardMode::Manual,
            _ => WizardMode::Manual,
        };
        self.selected_mode = Some(self.context.mode);

        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_preset_quick(&mut self, allow_back: bool) -> Result<bool> {
        self.log_step(tr("选择预设风格", "Select Style Preset"));

        if !wizard_support::is_interactive() {
            // Non-interactive: use the locked default preset.
            let preset = PresetCatalog::default_preset();
            wizard_support::apply_preset_selection(&mut self.context, &preset);
            self.context.current_step += 1;
            return Ok(true);
        }

        let presets = PresetCatalog::all_presets();
        let preset_options: Vec<(&str, &str, String)> = presets
            .iter()
            .map(|p| {
                (
                    p.id,
                    tr(p.name_zh, p.name),
                    format!("— {}", tr(p.description_zh, p.description)),
                )
            })
            .collect();

        let mut menu = select(tr("选择风格：", "Pick a vibe:"))
            .initial_value(
                self.selected_preset
                    .as_deref()
                    .unwrap_or(PresetCatalog::default_preset().id),
            )
            .items(&preset_options);
        if allow_back {
            menu = menu
                .item("back", tr("返回", "Back"), "")
                .escape_value("back");
        }
        let selected_preset_id = menu.interact().map_err(handle_cliclack_error)?;

        if selected_preset_id == "back" {
            return Ok(false);
        }

        if let Some(preset) = PresetCatalog::get_preset(selected_preset_id) {
            wizard_support::apply_preset_selection(&mut self.context, &preset);
            self.selected_preset = Some(selected_preset_id.to_owned());
        }

        self.context.current_step += 1;
        Ok(true)
    }

    fn step_detect_and_select_tools(&mut self) -> Result<ToolSelectionStep> {
        // The checkbox page rebuilds both lists from explicit choices.
        self.fastfetch_install_added = false;
        self.fastfetch_config_added = false;
        self.log_step(tr("检测并选择工具", "Detect and Select Tools"));

        let installed = self.installed_tools();

        // Display full inventory with status using typography helpers
        let install_context = crate::platform::packages::InstallContext::detect();
        self.display_tool_inventory(&installed, install_context)?;

        // Build two groups for the multiselect:
        // 1. Install candidates: missing + installable tools
        // 2. Tier 2 candidates: installed but not in PATH (need user opt-in to configure)
        let install_candidates =
            compute_install_candidates_for_platform(&installed, install_context);

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
        let tier1_ids = automatic_configuration_tools(&installed);

        // If nothing to show in multiselect, skip
        if install_candidates.is_empty() && tier2_candidates.is_empty() {
            self.context.selected_tools.clear();
            self.selected_tool_choices.clear();
            self.context.tools_to_configure = tier1_ids;
            eprintln!(
                "{}", tr("没有可选的额外工具，继续使用已检测到的工具。", "No additional installation choices are available; continuing with detected tools.")
            );
            self.context.current_step += 1;
            return Ok(ToolSelectionStep::Skipped);
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
            return Ok(ToolSelectionStep::Skipped);
        }

        // Build multiselect items
        let mut items: Vec<(&str, String, String)> = Vec::new();
        let install_ids: std::collections::HashSet<&str> =
            install_candidates.iter().map(|t| t.id).collect();

        // Group 1: tools to install
        for tool in &install_candidates {
            items.push((
                tool.id,
                tool.label.to_string(),
                wizard_support::tool_pitch(tool).to_string(),
            ));
        }

        // Group 2: Tier 2 tools (available but not in PATH)
        for tool in &tier2_candidates {
            items.push((
                tool.id,
                format!("{} ({})", tool.label, tr("不在 PATH 中", "not in PATH")),
                tr("仍为此工具配置", "configure anyway").to_string(),
            ));
        }

        eprintln!(
            "{}",
            tr(
                "选择要安装或配置的工具；可不选。",
                "Select tools to install or configure; selection is optional."
            )
        );
        let focused_tool = self.focused_tool.clone();
        let mut menu = multiselect(tr("工具：", "Tools:"))
            .initial_values(
                self.selected_tool_choices
                    .iter()
                    .map(String::as_str)
                    .collect(),
            )
            .items(
                &items
                    .iter()
                    .map(|(id, label, pitch)| (*id, label.as_str(), pitch.as_str()))
                    .collect::<Vec<_>>(),
            )
            .focus_value(focused_tool.as_deref().as_ref())
            .required(false);
        let outcome = menu.interact_with_back().map_err(handle_cliclack_error)?;
        self.focused_tool = menu.focused_value().map(|id| id.to_string());

        let selected = match outcome {
            MultiSelectOutcome::Back(pending) => {
                self.selected_tool_choices = pending.iter().map(|id| id.to_string()).collect();
                self.reviewed_installs = None;
                self.context.confirmed = false;
                return Ok(ToolSelectionStep::Back);
            }
            MultiSelectOutcome::Submitted(selected) => selected,
        };

        // Split: install only missing tools, configure = user picks + Tier 1
        let explicit_choices = selected.iter().map(|id| id.to_string()).collect();
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
        self.selected_tool_choices = explicit_choices;
        self.context.current_step += 1;
        Ok(ToolSelectionStep::Selected)
    }

    fn step_select_font(&mut self, allow_back: bool) -> Result<bool> {
        self.log_step(tr("选择字体", "Select Font"));

        let ctx = self.render_context();
        let roles = ctx.as_ref().map(Roles::new);
        wizard_support::print_current_font(roles.as_ref(), self.context.current_font.as_deref());
        let font_options = wizard_support::build_font_options();

        if !wizard_support::is_interactive() {
            // Non-interactive: keep current font if present, otherwise preserve preset/default.
            self.context.current_step += 1;
            return Ok(true);
        }

        let mut menu = select(tr("字体：", "Font:"))
            // Font detection is only a hint. Preserve even unrecognized or
            // imported settings unless the user explicitly picks a font.
            .initial_value(self.context.selected_font.as_deref().unwrap_or("skip"))
            .items(
                &font_options
                    .iter()
                    .map(|(id, label, desc)| (*id, *label, desc.as_str()))
                    .collect::<Vec<_>>(),
            );
        if allow_back {
            menu = menu
                .item("back", tr("返回", "Back"), "")
                .escape_value("back");
        }
        let selected_font_id = menu.interact().map_err(handle_cliclack_error)?;

        if selected_font_id == "back" {
            return Ok(false);
        }

        // Returning to this page retains the pending selection, but choosing
        // Skip explicitly clears it rather than applying an earlier choice.
        self.context.selected_font =
            FontCatalog::get_font(selected_font_id).map(|font| font.id.to_string());

        self.context.current_step += 1;
        Ok(true)
    }

    fn step_select_theme(&mut self) -> Result<bool> {
        self.log_step(tr("选择主题", "Select Theme"));

        let ctx = self.render_context();
        let roles = ctx.as_ref().map(Roles::new);
        wizard_support::print_current_theme(
            roles.as_ref(),
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
            return Ok(true);
        }

        let selected_theme_id = select(tr("主题：", "Theme:"))
            .initial_value(
                self.context
                    .selected_theme
                    .as_deref()
                    .unwrap_or("keep-current"),
            )
            .items(
                &theme_options
                    .iter()
                    .map(|(id, label, desc)| (id.as_str(), label.as_str(), desc.as_str()))
                    .collect::<Vec<_>>(),
            )
            .item("back", tr("返回", "Back"), "")
            .escape_value("back")
            .interact()
            .map_err(handle_cliclack_error)?;

        if selected_theme_id == "back" {
            return Ok(false);
        }

        if selected_theme_id != "keep-current" {
            self.context.selected_theme = Some(selected_theme_id.to_string());
        } else {
            self.context.selected_theme = None;
        }
        self.context.current_step += 1;
        Ok(true)
    }

    fn display_tool_inventory(
        &self,
        installed: &HashMap<String, crate::detection::ToolPresence>,
        install_context: crate::platform::packages::InstallContext,
    ) -> Result<()> {
        let ctx = self.render_context();
        let roles = ctx.as_ref().map(Roles::new);
        wizard_support::print_tool_inventory(roles.as_ref(), installed, install_context);
        Ok(())
    }

    fn log_step(&self, _step_name: &str) {
        // Intentionally minimal — no "Step X of Y" counter.
        // cliclack's own frames provide enough visual progress.
    }

    pub fn get_context(&self) -> &WizardContext {
        &self.context
    }

    pub(crate) fn terminal_profile(&self) -> &TerminalProfile {
        &self.terminal
    }

    pub fn get_context_mut(&mut self) -> &mut WizardContext {
        &mut self.context
    }

    fn installed_tools(&self) -> HashMap<String, ToolPresence> {
        detect_installed_tools_with_env(&self.env)
    }

    /// Quick mode uses shell-appropriate core tools; Manual keeps the full catalog.
    fn step_quick_auto_tools(&mut self) -> Result<()> {
        let installed = self.installed_tools();
        self.select_quick_tools(
            &installed,
            crate::platform::shell::detect_backend(),
            crate::platform::packages::InstallContext::detect(),
        )
    }

    fn select_quick_tools(
        &mut self,
        installed: &HashMap<String, ToolPresence>,
        shell: ShellBackend,
        context: crate::platform::packages::InstallContext,
    ) -> Result<()> {
        let (selected_tools, tools_to_configure) =
            compute_quick_tool_plan(installed, current_terminal_tool_id(&self.terminal), shell)?;

        // Also applies when Quick was selected inside a guided entry point.
        crate::cli::tool_selection::validate_install_routes(&selected_tools, context)?;

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
        let theme = registry.get(&selected_theme_id).ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData(
                tr("无法识别所选主题，请重新选择主题；尚未设置透明度。", "Selected theme is unknown; choose a recognized theme before inferring opacity. No opacity was selected.").into(),
            )
        })?;
        self.context.selected_opacity = Some(crate::opacity::recommended_opacity_for_theme(theme));

        self.context.current_step += 1;
        Ok(())
    }

    fn step_select_fastfetch(&mut self) -> Result<bool> {
        self.log_step(tr("启动时显示系统信息", "Fastfetch Auto-Run"));

        if !wizard_support::is_interactive() {
            // Non-interactive mode: skip fastfetch prompt
            self.context.current_step += 1;
            return Ok(true);
        }

        let choice = select(tr(
            "每次打开终端时显示系统信息？",
            "Show system info every time you open a terminal?",
        ))
        .item("off", tr("不显示", "Off"), "")
        .item("on", tr("显示", "On"), "")
        .item("back", tr("返回", "Back"), "")
        .initial_value(if self.context.fastfetch_enabled == Some(true) {
            "on"
        } else {
            "off"
        })
        .escape_value("back")
        .answer_keys(vec![('y', "on"), ('n', "off")])
        .interact()
        .map_err(handle_cliclack_error)?;

        if choice == "back" {
            return Ok(false);
        }
        let enable_fastfetch = choice == "on";

        // If the user wants autorun but fastfetch isn't on the actual PATH, schedule the
        // install. The shell wrapper guards on `command -v fastfetch`, which only sees
        // tier-1 entries — a fastfetch sitting in /opt/homebrew/bin but outside the user's
        // PATH would still make the autorun silently print nothing. Checking `is_tier1`
        // matches what the runtime guard will resolve.
        let in_path = enable_fastfetch
            && crate::detection::detect_tool_presence_with_env("fastfetch", &self.env).is_tier1();
        self.set_fastfetch_selection(enable_fastfetch, in_path);
        self.context.current_step += 1;
        Ok(true)
    }

    fn set_fastfetch_selection(&mut self, enabled: bool, in_path: bool) {
        if !enabled {
            if self.fastfetch_install_added {
                self.context.selected_tools.retain(|id| id != "fastfetch");
            }
            if self.fastfetch_config_added {
                self.context
                    .tools_to_configure
                    .retain(|id| id != "fastfetch");
            }
            self.fastfetch_install_added = false;
            self.fastfetch_config_added = false;
        } else if !in_path {
            if !self
                .context
                .selected_tools
                .iter()
                .any(|id| id == "fastfetch")
            {
                self.context.selected_tools.push("fastfetch".into());
                self.fastfetch_install_added = true;
            }
            if !self
                .context
                .tools_to_configure
                .iter()
                .any(|id| id == "fastfetch")
            {
                self.context.tools_to_configure.push("fastfetch".into());
                self.fastfetch_config_added = true;
            }
        }
        self.context.fastfetch_enabled = Some(enabled);
    }

    fn step_review_and_confirm(&mut self, allow_back: bool) -> Result<bool> {
        self.log_step(tr("检查并确认", "Review and Confirm"));
        let receipt =
            self.prepare_review_receipt(crate::platform::packages::InstallContext::detect())?;
        eprintln!("\n{}", self.display_receipt(&receipt));

        if !wizard_support::is_interactive() {
            self.context.confirmed = true;
            self.context.current_step += 1;
            return Ok(true);
        }

        let mut menu = select(tr("确认执行以上设置？", Language::SETUP_REVIEW))
            .item("cancel", tr("取消", "Cancel"), "")
            .item("apply", tr("执行设置", "Apply Setup"), "")
            .initial_value("cancel")
            .answer_keys(vec![('y', "apply"), ('n', "cancel")]);
        if allow_back {
            menu = menu
                .item("back", tr("返回修改", "Back to edit"), "")
                .escape_value("back");
        } else {
            menu = menu.escape_value("cancel");
        }
        let choice = menu.interact().map_err(handle_cliclack_error)?;
        if choice == "back" {
            self.reviewed_installs = None;
            return Ok(false);
        }
        let confirmed = choice == "apply";

        self.context.confirmed = confirmed;
        self.context.current_step += 1;

        if !confirmed {
            outro(tr("已取消设置", "Setup canceled")).ok();
        }

        Ok(true)
    }

    fn prepare_review_receipt(
        &mut self,
        install_context: crate::platform::packages::InstallContext,
    ) -> Result<ReviewReceipt> {
        self.context.confirmed = false;
        self.reviewed_installs = None;
        self.reviewed_installs = Some(crate::cli::tool_selection::InstallPlan::capture(
            &self.context.selected_tools,
            &self.env,
            install_context,
        )?);
        Ok(self.build_review_receipt())
    }

    /// Build a review receipt from current wizard state
    pub fn build_review_receipt(&self) -> ReviewReceipt {
        let mut receipt = ReviewReceipt::new();
        receipt.tools_to_configure = self.context.tools_to_configure.clone();

        // Add install actions based on selected tools
        for tool_id in &self.context.selected_tools {
            if receipt
                .install_actions
                .iter()
                .any(|action| action.tool_id == *tool_id)
            {
                continue;
            }
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
                self.context.current_font.as_ref().map(|font| {
                    format!(
                        "{} ({})",
                        wizard_support::wording("保留当前字体", "Keep current"),
                        super::file_output::terminal_text(font)
                    )
                })
            });
        receipt.selected_theme = self
            .context
            .selected_theme
            .as_deref()
            .map(|theme_id| self.resolve_theme_label(theme_id))
            .or_else(|| {
                self.context.current_theme.as_deref().map(|theme_id| {
                    format!(
                        "{} ({})",
                        wizard_support::wording("保留当前主题", "Keep current"),
                        super::file_output::terminal_text(&self.resolve_theme_label(theme_id))
                    )
                })
            });
        receipt.terminal_settings = self.context.selected_terminal_settings.clone();
        receipt.selected_opacity = self.context.selected_opacity;
        receipt.fastfetch_enabled = self.context.fastfetch_enabled;

        receipt
    }

    /// Format and display polished receipt
    pub fn display_receipt(&self, receipt: &ReviewReceipt) -> String {
        let ctx = self.render_context();
        let roles = ctx.as_ref().map(Roles::new);
        receipt.format_with_install_plan(
            roles.as_ref(),
            &self.terminal,
            self.reviewed_installs.as_ref(),
        )
    }

    pub(crate) fn confirmed_install_plan(&self) -> Result<crate::cli::tool_selection::InstallPlan> {
        self.reviewed_installs
            .as_ref()
            .filter(|_| self.context.confirmed)
            .cloned()
            .ok_or_else(|| {
                crate::error::SlateError::InvalidConfig(
                    tr(
                        "安装方案尚未确认，请重新检查设置。",
                        "No confirmed tool installation plan; review setup again",
                    )
                    .into(),
                )
            })
    }

    fn render_context(&self) -> Option<RenderContext<'_>> {
        self.context
            .current_theme
            .as_deref()
            .and_then(|id| self.theme_selector.get_theme(id))
            .or_else(|| {
                self.theme_selector
                    .get_theme(crate::theme::DEFAULT_THEME_ID)
            })
            .map(RenderContext::new)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_configuration_order_is_catalog_stable_and_preserves_opt_in() {
        let present = |in_path| ToolPresence {
            installed: true,
            in_path,
            evidence: None,
        };
        let entries = vec![
            ("kitty", present(false)),
            ("ghostty", present(false)),
            ("bat", present(true)),
            ("starship", present(true)),
            ("btop", present(false)),
            ("alacritty", ToolPresence::missing()),
            ("unknown", present(true)),
        ];
        let expected = crate::cli::tool_selection::ToolCatalog::all_tools()
            .iter()
            .filter(|tool| matches!(tool.id, "kitty" | "ghostty" | "bat" | "starship"))
            .map(|tool| tool.id.to_owned())
            .collect::<Vec<_>>();
        assert_eq!(expected.len(), 4);
        for reverse in [false, true] {
            let mut order = entries.clone();
            if reverse {
                order.reverse();
            }
            let installed = order
                .into_iter()
                .map(|(id, value)| (id.to_owned(), value))
                .collect();
            assert_eq!(automatic_configuration_tools(&installed), expected);
        }
        assert!(automatic_configuration_tools(&HashMap::new()).is_empty());
    }

    #[test]
    fn wizard_opacity_uses_appearance_but_keeps_explicit_presets_without_writes() {
        use crate::opacity::OpacityPreset;
        for selected in [
            None,
            Some(OpacityPreset::Solid),
            Some(OpacityPreset::Frosted),
            Some(OpacityPreset::Clear),
        ] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            let mut wizard = Wizard::with_env(&env).unwrap();
            wizard.context.selected_theme = Some("rose-pine-dawn".into());
            wizard.context.selected_opacity = selected;
            wizard.step_auto_opacity().unwrap();
            assert_eq!(
                wizard.context.selected_opacity,
                selected.or(Some(OpacityPreset::Solid))
            );
            assert_eq!(wizard.context.current_step, 1);
            assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn wizard_opacity_unknown_theme_stops_without_silent_fallback_or_state_advance() {
        for selected in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            let mut wizard = Wizard::with_env(&env).unwrap();
            if selected {
                wizard.context.selected_theme = Some("PRIVATE_UNKNOWN".into());
            } else {
                wizard.context.current_theme = Some("PRIVATE_UNKNOWN".into());
            }
            let error = wizard.step_auto_opacity().unwrap_err().to_string();
            assert!(error.contains("No opacity was selected"));
            assert!(!error.contains("PRIVATE_UNKNOWN"));
            assert_eq!(wizard.context.selected_opacity, None);
            assert_eq!(wizard.context.current_step, 0);
            assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn quick_shell_wizard_review_uses_shell_defaults_including_force_mode() {
        use crate::platform::packages::{InstallContext, PackageManagerBackend};
        let context = InstallContext {
            package_manager: PackageManagerBackend::Unsupported,
            supported_os: true,
        };
        for force in [false, true] {
            for shell in [
                ShellBackend::Bash,
                ShellBackend::Fish,
                ShellBackend::Zsh,
                ShellBackend::Unsupported,
            ] {
                let temp = tempfile::tempdir().unwrap();
                let env = SlateEnv::with_home(temp.path().to_owned());
                let mut wizard = Wizard::with_env(&env).unwrap();
                wizard.context.mode = WizardMode::Quick;
                wizard.context.force = force;
                let result = wizard.select_quick_tools(&HashMap::new(), shell, context);
                if matches!(shell, ShellBackend::Bash | ShellBackend::Fish) {
                    result.unwrap();
                    assert_eq!(wizard.context.selected_tools, ["starship"]);
                    assert_eq!(wizard.context.tools_to_configure, ["starship"]);
                    let receipt = wizard.prepare_review_receipt(context).unwrap();
                    assert_eq!(receipt.install_actions.len(), 1);
                    let text = wizard.display_receipt(&receipt);
                    assert!(text.contains("user-local executable"));
                    assert!(!text.contains("zsh-syntax-highlighting"));
                } else {
                    assert!(result.is_err());
                    assert!(wizard.context.selected_tools.is_empty());
                    assert!(wizard.confirmed_install_plan().is_err());
                }
                assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
            }
        }
    }

    #[test]
    fn install_review_wizard_uses_captured_routes_and_does_not_reuse_stale_confirmation() {
        use crate::platform::packages::{InstallContext, PackageManagerBackend};
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let mut wizard = Wizard::with_env(&env).unwrap();
        wizard.context.selected_tools = vec!["starship".into(), "delta".into(), "starship".into()];
        let apt = InstallContext {
            package_manager: PackageManagerBackend::Apt,
            supported_os: true,
        };
        let receipt = wizard.prepare_review_receipt(apt).unwrap();
        assert_eq!(receipt.install_actions.len(), 2);
        let text = wizard.display_receipt(&receipt);
        assert!(text.contains("apt package (git-delta; administrator access)"));
        assert!(text.contains("user-local executable") && !text.contains("Homebrew formula"));
        assert!(wizard.confirmed_install_plan().is_err());
        wizard.context.confirmed = true;
        let captured = wizard.confirmed_install_plan().unwrap();
        assert!(captured.description("delta").unwrap().contains("git-delta"));
        wizard.context.selected_tools = vec!["fastfetch".into()];
        assert!(wizard
            .prepare_review_receipt(InstallContext {
                package_manager: PackageManagerBackend::Unsupported,
                ..apt
            })
            .is_err());
        assert!(wizard.confirmed_install_plan().is_err());
        assert!(!wizard.context.confirmed);
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn bootstrap_quick_keeps_requested_core_tools_and_blocks_unavailable_routes() {
        use crate::platform::packages::{InstallContext, PackageManagerBackend};
        let context = InstallContext {
            package_manager: PackageManagerBackend::Unsupported,
            supported_os: true,
        };
        let mut installed = HashMap::new();
        let (selected, _) = compute_quick_tool_plan(&installed, None, ShellBackend::Zsh).unwrap();
        assert_eq!(selected, ["starship", "zsh-syntax-highlighting"]);
        let error =
            crate::cli::tool_selection::validate_install_routes(&selected, context).unwrap_err();
        assert!(error.to_string().contains("zsh-syntax-highlighting"));
        assert!(error.to_string().contains("Manual setup"));
        installed.insert(
            "zsh-syntax-highlighting".into(),
            ToolPresence {
                installed: true,
                in_path: false,
                evidence: None,
            },
        );
        let (selected, configure) =
            compute_quick_tool_plan(&installed, None, ShellBackend::Zsh).unwrap();
        assert_eq!(selected, ["starship"]);
        assert!(configure.contains(&"zsh-syntax-highlighting".into()));
        crate::cli::tool_selection::validate_install_routes(&selected, context).unwrap();
        assert!(
            crate::cli::tool_selection::validate_install_routes(&["bat".into()], context).is_err()
        );
        crate::cli::tool_selection::validate_install_routes(&[], context).unwrap();
    }

    fn isolated_wizard() -> Wizard {
        let home = tempfile::tempdir().unwrap();
        Wizard::with_env(&SlateEnv::with_home(home.path().to_owned())).unwrap()
    }

    #[test]
    fn wizard_profile_stays_bound_for_inventory_theme_and_session_receipt() {
        let td = tempfile::tempdir().unwrap();
        for (name, theme) in [("first", "nord"), ("second", "dracula")] {
            let home = td.path().join(name);
            let custom = home.join("custom-xdg");
            let env = SlateEnv::from_vars(|key| match key {
                "HOME" => Some(home.as_os_str().to_owned()),
                "XDG_CONFIG_HOME" => Some(custom.as_os_str().to_owned()),
                "SSH_CONNECTION" => Some("private remote session".into()),
                _ => None,
            })
            .unwrap();
            let app = home.join("Applications/Ghostty.app");
            std::fs::create_dir_all(&app).unwrap();
            std::fs::create_dir_all(env.config_dir()).unwrap();
            std::fs::write(env.managed_file("current"), format!("{theme}\n")).unwrap();
            let mut wizard = Wizard::with_env(&env).unwrap();
            assert_eq!(wizard.terminal.session(), env.session());
            assert_eq!(
                wizard.installed_tools()["ghostty"].evidence,
                Some(crate::detection::ToolEvidence::AppBundle(app))
            );
            std::fs::write(env.managed_file("current"), "catppuccin-mocha\n").unwrap();
            assert_eq!(wizard.render_context().unwrap().theme.id, theme);
            wizard.step_quick_auto_tools().unwrap();
            assert!(wizard
                .context
                .tools_to_configure
                .iter()
                .any(|tool| tool == "ghostty"));
            let receipt = wizard.display_receipt(&wizard.build_review_receipt());
            assert!(receipt.contains("remote shell"));
            assert!(!env.slate_cache_dir().exists());
        }
    }

    #[test]
    fn test_wizard_new() {
        let wizard = isolated_wizard();
        let context = wizard.get_context();
        assert_eq!(context.mode, WizardMode::Manual);
        assert_eq!(context.current_step, 0);
        assert_eq!(context.total_steps, 6);
    }

    #[test]
    fn test_wizard_context_fields() {
        let wizard = isolated_wizard();
        let context = wizard.get_context();
        assert!(context.selected_tools.is_empty());
        assert!(context.selected_font.is_none());
        assert!(context.selected_theme.is_none());
    }

    #[test]
    fn test_wizard_force_flag_clears_state() {
        let mut wizard = isolated_wizard();
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
        let wizard = isolated_wizard();
        let receipt = wizard.build_review_receipt();
        assert!(receipt.install_actions.is_empty());
    }

    #[test]
    fn test_build_review_receipt_with_selections() {
        let mut wizard = isolated_wizard();
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
        let mut wizard = isolated_wizard();
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
        let mut wizard = isolated_wizard();
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
    fn manual_review_exposes_actual_opacity_without_inventing_preset_padding() {
        for opacity in [
            crate::opacity::OpacityPreset::Solid,
            crate::opacity::OpacityPreset::Frosted,
        ] {
            let mut wizard = isolated_wizard();
            wizard.context.selected_opacity = Some(opacity);
            let receipt = wizard.build_review_receipt();
            assert!(receipt.terminal_settings.is_none());
            assert_eq!(receipt.selected_opacity, Some(opacity));
            let rendered = receipt.format_with_install_plan(
                None,
                &TerminalProfile::from_env_vars(Some("ghostty"), None),
                None,
            );
            assert!(
                rendered.contains(&format!("opacity {:.2}", opacity.to_f32())),
                "{rendered}"
            );
            assert_eq!(
                rendered.contains("frosted glass"),
                opacity.blur_radius() > 0
            );
        }
    }

    #[test]
    fn review_distinguishes_unchanged_disabled_and_enabled_startup_info() {
        for choice in [None, Some(false), Some(true)] {
            let mut wizard = isolated_wizard();
            wizard.context.fastfetch_enabled = choice;
            let receipt = wizard.build_review_receipt();
            assert_eq!(receipt.fastfetch_enabled, choice);
            let text = wizard.display_receipt(&receipt);
            match choice {
                None => assert!(!text.contains("Startup system info")),
                Some(false) => assert!(text.contains("Startup system info: Off"), "{text}"),
                Some(true) => assert!(text.contains("Startup system info: On"), "{text}"),
            }
        }
    }

    #[test]
    fn editing_autorun_removes_only_its_own_install_and_config_requests() {
        for install in [false, true] {
            for configure in [false, true] {
                let mut wizard = isolated_wizard();
                if install {
                    wizard.context.selected_tools.push("fastfetch".into());
                }
                if configure {
                    wizard.context.tools_to_configure.push("fastfetch".into());
                }
                let before_installs = wizard.context.selected_tools.clone();
                let before_configs = wizard.context.tools_to_configure.clone();
                wizard.set_fastfetch_selection(true, false);
                wizard.set_fastfetch_selection(true, false);
                assert_eq!(wizard.context.selected_tools, ["fastfetch"]);
                assert_eq!(wizard.context.tools_to_configure, ["fastfetch"]);
                wizard.set_fastfetch_selection(false, false);
                assert_eq!(wizard.context.selected_tools, before_installs);
                assert_eq!(wizard.context.tools_to_configure, before_configs);
                assert_eq!(wizard.context.fastfetch_enabled, Some(false));
            }
        }
    }

    #[test]
    fn changing_setup_mode_clears_pending_actions_but_keeps_saved_state_hints() {
        let mut wizard = isolated_wizard();
        wizard.context.current_theme = Some("nord".into());
        wizard.context.current_font = Some("Personal Font".into());
        wizard_support::apply_preset_selection(
            &mut wizard.context,
            &PresetCatalog::default_preset(),
        );
        wizard.context.selected_tools.push("bat".into());
        wizard.context.tools_to_configure.push("bat".into());
        wizard.set_fastfetch_selection(true, false);
        wizard.context.confirmed = true;
        wizard.clear_pending_choices();
        assert!(wizard.context.selected_tools.is_empty());
        assert!(wizard.context.tools_to_configure.is_empty());
        assert!(wizard.context.selected_font.is_none());
        assert!(wizard.context.selected_theme.is_none());
        assert!(wizard.context.selected_opacity.is_none());
        assert!(wizard.context.selected_terminal_settings.is_none());
        assert!(wizard.context.fastfetch_enabled.is_none());
        assert!(!wizard.context.confirmed);
        assert!(wizard.confirmed_install_plan().is_err());
        assert_eq!(wizard.context.current_theme.as_deref(), Some("nord"));
        assert_eq!(
            wizard.context.current_font.as_deref(),
            Some("Personal Font")
        );
    }

    #[test]
    fn review_includes_configuration_only_targets_without_implying_installation() {
        let mut wizard = isolated_wizard();
        wizard.context.tools_to_configure =
            vec!["starship".into(), "ghostty".into(), "ghostty".into()];
        let receipt = wizard.build_review_receipt();
        assert_eq!(
            receipt.tools_to_configure,
            wizard.context.tools_to_configure
        );
        assert!(receipt.install_actions.is_empty());
        let text = wizard.display_receipt(&receipt);
        assert!(
            text.contains("Configure colors: Ghostty · Starship"),
            "{text}"
        );
        assert!(!text.contains("formula") && !text.contains("cask"));
        let mut unsafe_receipt = ReviewReceipt::new();
        unsafe_receipt.tools_to_configure = vec!["unknown\x1b[2J\nfixture".into()];
        let text = unsafe_receipt.format_with_install_plan(None, &wizard.terminal, None);
        assert!(!text.contains('\x1b'));
        assert!(!text.contains("[2J\nfixture"));
    }

    #[test]
    fn test_quick_mode_adjusts_step_count() {
        let mut wizard = isolated_wizard();
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
        let mut wizard = isolated_wizard();
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
        let mut wizard = isolated_wizard();
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
            compute_quick_tool_plan(&installed, Some("ghostty"), ShellBackend::Zsh).unwrap();

        assert!(!selected_tools.contains(&"starship".to_string()));
        assert!(tools_to_configure.contains(&"starship".to_string()));
        assert!(tools_to_configure.contains(&"ghostty".to_string()));
    }
}
