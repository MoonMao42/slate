use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::font_selection::FontCatalog;
use crate::cli::preset_selection::StylePreset;
use crate::cli::theme_selection::ThemeSelector;
use crate::cli::tool_selection::ToolCatalog;
use crate::cli::wizard_core::WizardContext;
use crate::error::Result;
use crate::opacity::OpacityPreset;
use std::collections::HashMap;
use std::io::IsTerminal;

pub(crate) fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
}

fn preset_opacity(preset: &StylePreset) -> OpacityPreset {
    if preset.visuals.blur_radius > 0 {
        OpacityPreset::Frosted
    } else if preset.visuals.background_opacity < 1.0 {
        OpacityPreset::Clear
    } else {
        OpacityPreset::Solid
    }
}

pub(crate) fn apply_preset_selection(context: &mut WizardContext, preset: &StylePreset) {
    let opacity = preset_opacity(preset);
    context.selected_font = Some(preset.font_id.to_string());
    context.selected_theme = Some(preset.theme_id.to_string());
    context.selected_opacity = Some(opacity);
    context.selected_terminal_settings = Some(crate::cli::tool_selection::TerminalSettings {
        background_opacity: opacity.to_f32(),
        blur_enabled: opacity.blur_radius() > 0,
        padding_x: preset.visuals.padding_x,
        padding_y: preset.visuals.padding_y,
    });
}

pub(crate) fn build_theme_options(
    theme_selector: &ThemeSelector,
    current_theme_id: Option<&str>,
) -> Vec<(String, String, String)> {
    let mut theme_options = Vec::new();

    if let Some(current_theme_id) = current_theme_id {
        let current_name = theme_selector
            .get_theme(current_theme_id)
            .map(|theme| theme.name.clone())
            .unwrap_or_else(|| current_theme_id.to_string());
        theme_options.push((
            "keep-current".to_string(),
            "Keep current theme".to_string(),
            format!("— {}", current_name),
        ));
    }

    theme_options.extend(theme_selector.all_themes().into_iter().map(|theme| {
        (
            theme.id.clone(),
            theme.name.clone(),
            format!("— {}", theme.family),
        )
    }));

    theme_options
}

pub(crate) fn resolve_theme_id_for_opacity(
    context: &WizardContext,
    theme_selector: &ThemeSelector,
) -> Result<String> {
    if let Some(theme_id) = context.selected_theme.as_ref() {
        return Ok(theme_id.clone());
    }
    if let Some(theme_id) = context.current_theme.as_ref() {
        return Ok(theme_id.clone());
    }
    theme_selector
        .all_themes()
        .first()
        .map(|theme| theme.id.clone())
        .ok_or_else(|| {
            crate::error::SlateError::InvalidThemeData("No themes available".to_string())
        })
}

/// Pure formatter for the `current font: …` secondary label — split
/// from the eprintln wrapper so snapshot tests can assert on the byte
/// output without touching `std::io::stderr`.
pub(crate) fn format_current_font_label(
    r: Option<&Roles<'_>>,
    current_font: Option<&str>,
) -> String {
    let value = current_font.unwrap_or("system default");
    let line = format!("current font: {}", value);
    match r {
        Some(r) => format!("  {}", r.path(&line)),
        None => format!("  {}", line),
    }
}

/// Pure formatter for the `current theme: …` secondary label.
pub(crate) fn format_current_theme_label(
    r: Option<&Roles<'_>>,
    theme_selector: &ThemeSelector,
    current_theme_id: Option<&str>,
) -> String {
    let label = current_theme_id
        .map(|id| {
            theme_selector
                .get_theme(id)
                .map(|theme| theme.name.clone())
                .unwrap_or_else(|| id.to_string())
        })
        .unwrap_or_else(|| "not yet applied".to_string());
    let line = format!("current theme: {}", label);
    match r {
        Some(r) => format!("  {}", r.path(&line)),
        None => format!("  {}", line),
    }
}

pub(crate) fn print_current_font(current_font: Option<&str>) {
    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);
    eprintln!("{}", format_current_font_label(r.as_ref(), current_font));
    eprintln!();
}

pub(crate) fn print_current_theme(theme_selector: &ThemeSelector, current_theme_id: Option<&str>) {
    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);
    eprintln!(
        "{}",
        format_current_theme_label(r.as_ref(), theme_selector, current_theme_id)
    );
    eprintln!();
}

/// Pure formatter for the tool-inventory block. Returns the already-
/// joined, newline-separated body; caller adds blank-line padding via
/// eprintln. Splitting the formatter out keeps snapshot tests pure.
pub(crate) fn format_tool_inventory(
    r: Option<&Roles<'_>>,
    installed: &HashMap<String, crate::detection::ToolPresence>,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(match r {
        Some(r) => r.heading("Tool Inventory"),
        None => "◆ Tool Inventory".to_string(),
    });

    for tool in ToolCatalog::all_tools() {
        let presence = installed.get(tool.id);
        let is_installed = presence.map(|p| p.installed).unwrap_or(false);
        let is_in_path = presence.map(|p| p.in_path).unwrap_or(false);
        let status_mark = if is_installed && is_in_path {
            '✓'
        } else if is_installed {
            '~' // Tier 2: available but not in PATH
        } else if tool.detect_only {
            '◆'
        } else {
            '○'
        };

        let install_note = if tool.detect_only {
            " (synced if installed)"
        } else if !tool.installable {
            " (not installable)"
        } else {
            ""
        };

        let row = format!(
            "{} {} — {}{}",
            status_mark, tool.label, tool.pitch, install_note
        );
        lines.push(match r {
            Some(r) => r.tree_branch(&row),
            None => format!("  {}", row),
        });
    }
    lines.join("\n")
}

pub(crate) fn print_tool_inventory(installed: &HashMap<String, crate::detection::ToolPresence>) {
    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);
    eprintln!("\n{}\n", format_tool_inventory(r.as_ref(), installed));
}

pub(crate) fn build_font_options() -> Vec<(&'static str, &'static str, String)> {
    let mut font_options: Vec<(&str, &str, String)> = FontCatalog::all_fonts()
        .iter()
        .map(|font| (font.id, font.name, format!("— {}", font.label)))
        .collect();
    let (skip_id, skip_label) = FontCatalog::skip_option();
    font_options.push((skip_id, skip_label, String::new()));
    font_options
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::preset_selection::PresetCatalog;
    use crate::cli::wizard_core::{WizardContext, WizardMode};

    fn test_context() -> WizardContext {
        WizardContext {
            mode: WizardMode::Quick,
            current_step: 0,
            total_steps: 0,
            selected_tools: Vec::new(),
            tools_to_configure: Vec::new(),
            selected_font: None,
            selected_theme: None,
            selected_opacity: None,
            fastfetch_enabled: None,
            selected_terminal_settings: None,
            current_font: None,
            current_theme: None,
            confirmed: false,
            force: false,
            start_time: None,
        }
    }

    #[test]
    fn test_apply_preset_selection_sets_effective_opacity_for_quick_mode() {
        let mut context = test_context();
        let preset = PresetCatalog::get_preset("modern-dark").unwrap();

        apply_preset_selection(&mut context, &preset);

        assert_eq!(context.selected_opacity, Some(OpacityPreset::Frosted));
        let settings = context
            .selected_terminal_settings
            .expect("terminal settings");
        assert_eq!(settings.background_opacity, OpacityPreset::Frosted.to_f32());
        assert!(settings.blur_enabled);
    }

    /// Wave 1 snapshot — tool inventory block rendered through the
    /// MockTheme-backed Basic mode Roles. Locks `◆ Tool Inventory`
    /// heading + `┃ ├─` tree rows.
    #[test]
    fn tool_inventory_basic_mode_snapshot() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};
        use crate::detection::{ToolEvidence, ToolPresence};
        use std::path::PathBuf;

        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);

        let mut installed: HashMap<String, ToolPresence> = HashMap::new();
        installed.insert(
            "ghostty".to_string(),
            ToolPresence::in_path_with(ToolEvidence::Executable(PathBuf::from("/usr/bin/ghostty"))),
        );
        installed.insert("starship".to_string(), ToolPresence::missing());

        let out = format_tool_inventory(Some(&r), &installed);
        insta::assert_snapshot!("wizard_support_tool_inventory_basic", out);
    }

    /// `◆` + `Tool Inventory` heading anchor must land across every
    /// mode. Truecolor wraps the diamond in ANSI so we assert on the
    /// anchors separately.
    #[test]
    fn tool_inventory_always_carries_diamond_heading() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        let installed: HashMap<String, crate::detection::ToolPresence> = HashMap::new();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let out = format_tool_inventory(Some(&r), &installed);
            assert!(
                out.contains('◆'),
                "missing diamond in mode {mode:?}: {out:?}"
            );
            assert!(
                out.contains("Tool Inventory"),
                "missing `Tool Inventory` in mode {mode:?}: {out:?}"
            );
        }
    }

    /// D-05 graceful degrade — zero ANSI when Roles is absent.
    #[test]
    fn tool_inventory_falls_back_to_plain_without_roles() {
        let installed: HashMap<String, crate::detection::ToolPresence> = HashMap::new();
        let out = format_tool_inventory(None, &installed);
        assert!(!out.contains('\x1b'));
        assert!(out.contains("◆ Tool Inventory"));
    }

    #[test]
    fn current_font_label_carries_value_via_path_role() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);

        let out = format_current_font_label(Some(&r), Some("JetBrains Mono"));
        assert!(out.contains("JetBrains Mono"));
        assert!(out.contains("current font:"));
    }

    #[test]
    fn current_font_label_falls_back_when_value_absent() {
        let out = format_current_font_label(None, None);
        assert_eq!(out, "  current font: system default");
    }
}
