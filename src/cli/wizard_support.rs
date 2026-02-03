use crate::cli::font_selection::FontCatalog;
use crate::cli::preset_selection::StylePreset;
use crate::cli::theme_selection::ThemeSelector;
use crate::cli::tool_selection::ToolCatalog;
use crate::cli::wizard_core::WizardContext;
use crate::design::typography::Typography;
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

pub(crate) fn print_current_font(current_font: Option<&str>) {
    if let Some(current_font) = current_font {
        eprintln!(
            "{}",
            Typography::secondary_label("current font", current_font)
        );
    } else {
        eprintln!(
            "{}",
            Typography::secondary_label("current font", "system default")
        );
    }
    eprintln!();
}

pub(crate) fn print_current_theme(theme_selector: &ThemeSelector, current_theme_id: Option<&str>) {
    if let Some(current_theme_id) = current_theme_id {
        let current_label = theme_selector
            .get_theme(current_theme_id)
            .map(|theme| theme.name.clone())
            .unwrap_or_else(|| current_theme_id.to_string());
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
}

pub(crate) fn print_tool_inventory(installed: &HashMap<String, crate::detection::ToolPresence>) {
    eprintln!("\n{}\n", Typography::section_header("Tool Inventory"));

    for tool in ToolCatalog::all_tools() {
        let presence = installed.get(tool.id);
        let is_installed = presence.map(|p| p.installed).unwrap_or(false);
        let is_in_path = presence.map(|p| p.in_path).unwrap_or(false);
        let status_mark = if is_installed && is_in_path {
            "✓"
        } else if is_installed {
            "~" // Tier 2: available but not in PATH
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
            fastfetch_enabled: false,
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
