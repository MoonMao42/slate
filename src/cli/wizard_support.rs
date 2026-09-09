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

pub(crate) fn wording(zh: &'static str, en: &'static str) -> &'static str {
    crate::config::ui_language::Text { zh, en }.get(super::ui_language::output_language())
}

pub(crate) fn tool_pitch(tool: &crate::cli::tool_selection::ToolMetadata) -> &'static str {
    let zh = match tool.id {
        "ghostty" => "终端窗口与配色",
        "starship" => "命令提示符",
        "bat" => "代码与文本预览着色",
        "delta" => "Git 差异着色",
        "eza" => "彩色文件列表",
        "lazygit" => "交互式 Git 客户端",
        "fastfetch" => "系统信息展示",
        "btop" => "系统监控配色 · 修改后需重新打开",
        "yazi" => "文件管理与代码预览配色 · 修改后需重新打开",
        "zellij" => "分屏、标签页与会话配色",
        "zsh-syntax-highlighting" => "Shell 命令语法高亮",
        "alacritty" => "Alacritty 终端配色",
        "kitty" => "Kitty 终端配色",
        "tmux" => "终端复用与会话配色",
        _ => tool.pitch,
    };
    wording(zh, tool.pitch)
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

    if let Some(current_theme) = current_theme_id.and_then(|id| theme_selector.get_theme(id)) {
        theme_options.push((
            "keep-current".to_string(),
            wording("保留当前主题", "Keep current theme").to_string(),
            format!("— {}", current_theme.name),
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

/// Pure formatter for the best-effort saved font hint — split
/// from the eprintln wrapper so snapshot tests can assert on the byte
/// output without touching `std::io::stderr`.
pub(crate) fn format_current_font_label(
    r: Option<&Roles<'_>>,
    current_font: Option<&str>,
) -> String {
    // Detection reads direct config files only, not imports or a live window.
    // A missing hint therefore does not establish that the system default is used.
    let line = match current_font {
        Some(value) => format!(
            "{}: {}",
            wording("配置中的字体（仅供参考）", "configured font (hint)"),
            super::file_output::terminal_text(value)
        ),
        None => wording(
            "未检测到配置字体；可选择保留现有设置",
            "configured font: not detected; skip to keep existing settings",
        )
        .to_string(),
    };
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
                .unwrap_or_else(|| {
                    wording(
                        "无法识别，请从列表选择",
                        "unrecognized — choose a listed theme",
                    )
                    .to_string()
                })
        })
        .unwrap_or_else(|| wording("尚未应用", "not yet applied").to_string());
    let line = format!("{}: {}", wording("当前主题", "current theme"), label);
    match r {
        Some(r) => format!("  {}", r.path(&line)),
        None => format!("  {}", line),
    }
}

pub(crate) fn print_current_font(r: Option<&Roles<'_>>, current_font: Option<&str>) {
    eprintln!("{}", format_current_font_label(r, current_font));
    eprintln!();
}

pub(crate) fn print_current_theme(
    r: Option<&Roles<'_>>,
    theme_selector: &ThemeSelector,
    current_theme_id: Option<&str>,
) {
    eprintln!(
        "{}",
        format_current_theme_label(r, theme_selector, current_theme_id)
    );
    eprintln!();
}

/// Pure formatter for the tool-inventory block. Returns the already-
/// joined, newline-separated body; caller adds blank-line padding via
/// eprintln. Splitting the formatter out keeps snapshot tests pure.
pub(crate) fn format_tool_inventory(
    r: Option<&Roles<'_>>,
    installed: &HashMap<String, crate::detection::ToolPresence>,
    install_context: crate::platform::packages::InstallContext,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(match r {
        Some(r) => r.heading(wording("工具列表", "Tool Inventory")),
        None => format!("◆ {}", wording("工具列表", "Tool Inventory")),
    });

    for tool in ToolCatalog::all_tools() {
        let presence = installed.get(tool.id);
        let is_installed = presence.map(|p| p.installed).unwrap_or(false);
        let has_command = presence.is_some_and(|p| p.has_path_executable());
        let status_mark = if has_command {
            '✓'
        } else if is_installed {
            '~' // Found, but no executable-in-PATH evidence.
        } else if tool.detect_only {
            '◆'
        } else {
            '○'
        };

        let install_note = if tool.detect_only {
            wording("（已安装时可同步配置）", " (synced if installed)").to_owned()
        } else if !tool.installable {
            wording("（不支持自动安装）", " (not installable)").to_owned()
        } else if !is_installed {
            match install_context.route(tool.id) {
                Ok(crate::platform::packages::ToolInstallRoute::UserLocalStarship) => wording(
                    "（安装到用户目录；需要 curl）",
                    " (user-local install; requires curl)",
                )
                .to_owned(),
                Ok(_) => String::new(),
                Err(_) => wording("（需手动安装）", " (manual installation required)").to_owned(),
            }
        } else {
            String::new()
        };

        let row = format!(
            "{} {} — {}{}",
            status_mark,
            tool.label,
            tool_pitch(tool),
            install_note
        );
        lines.push(match r {
            Some(r) => r.tree_branch(&row),
            None => format!("  {}", row),
        });
    }
    lines.join("\n")
}

pub(crate) fn print_tool_inventory(
    r: Option<&Roles<'_>>,
    installed: &HashMap<String, crate::detection::ToolPresence>,
    install_context: crate::platform::packages::InstallContext,
) {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        eprintln!(
            "\n{}\n",
            format_tool_inventory_summary(installed, super::ui_language::output_language())
        );
        return;
    }
    eprintln!(
        "\n{}\n",
        format_tool_inventory(r, installed, install_context)
    );
}

fn format_tool_inventory_summary(
    installed: &HashMap<String, crate::detection::ToolPresence>,
    language: crate::config::ui_language::UiLanguage,
) -> String {
    let mut in_path = 0;
    let mut detected = 0;
    let mut missing = 0;
    for tool in ToolCatalog::all_tools() {
        match installed.get(tool.id) {
            Some(presence) if presence.has_path_executable() => in_path += 1,
            Some(presence) if presence.installed => detected += 1,
            _ => missing += 1,
        }
    }
    match language {
        crate::config::ui_language::UiLanguage::Chinese => format!(
            "工具检测：PATH 命令 {in_path} · 其他检测记录 {detected} · 未检测到 {missing}"
        ),
        crate::config::ui_language::UiLanguage::English => format!(
            "Tool detection: {in_path} PATH commands · {detected} other detection records · {missing} not detected"
        ),
    }
}

pub(crate) fn build_font_options() -> Vec<(&'static str, &'static str, String)> {
    let mut font_options: Vec<(&str, &str, String)> = FontCatalog::all_fonts()
        .iter()
        .map(|font| {
            let zh = match font.id {
                "jetbrains-mono" => "适合终端阅读",
                "fira-code" => "适合喜欢连字的用户",
                "iosevka-term" => "紧凑窄体",
                "hack" => "清晰经典",
                _ => font.label,
            };
            (font.id, font.name, format!("— {}", wording(zh, font.label)))
        })
        .collect();
    let (skip_id, skip_label) = FontCatalog::skip_option();
    font_options.push((skip_id, wording("保留当前字体", skip_label), String::new()));
    font_options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_command_marks_require_executable_evidence_not_configuration_tier() {
        use crate::detection::{ToolEvidence, ToolPresence};
        for evidence in [
            ToolEvidence::AppBundle("/fixture/app".into()),
            ToolEvidence::Config("/fixture/config".into()),
            ToolEvidence::Plugin("/fixture/plugin".into()),
        ] {
            let presence = ToolPresence::installed_with(evidence);
            assert!(presence.is_tier1()); // Automatic configuration policy is unchanged.
            assert!(!presence.has_path_executable());
            let installed = HashMap::from([("ghostty".into(), presence)]);
            let full = format_tool_inventory(None, &installed, homebrew_context());
            assert!(full
                .lines()
                .any(|line| line.trim_start().starts_with("~ Ghostty")));
            assert!(!full
                .lines()
                .any(|line| line.trim_start().starts_with("✓ Ghostty")));
        }
        let evidence = ToolEvidence::Executable("/fixture/bin/bat".into());
        assert!(ToolPresence::in_path_with(evidence.clone()).has_path_executable());
        assert!(!ToolPresence::fallback_with(evidence).has_path_executable());
        assert!(!ToolPresence::missing().has_path_executable());
    }

    #[test]
    fn compact_inventory_counts_catalog_evidence_without_activation_claims() {
        use crate::{config::ui_language::UiLanguage, detection::ToolPresence};
        let mut installed = HashMap::new();
        installed.insert(
            "bat".into(),
            ToolPresence {
                installed: true,
                in_path: true,
                evidence: Some(crate::detection::ToolEvidence::Executable(
                    "/fixture/bin/bat".into(),
                )),
            },
        );
        installed.insert(
            "ghostty".into(),
            ToolPresence {
                installed: true,
                in_path: true,
                evidence: Some(crate::detection::ToolEvidence::AppBundle(
                    "/fixture/Ghostty.app".into(),
                )),
            },
        );
        installed.insert(
            "not-a-catalog-tool".into(),
            ToolPresence {
                installed: true,
                in_path: true,
                evidence: None,
            },
        );
        // Inconsistent detection must not turn a missing tool into a found one.
        installed.insert(
            "starship".into(),
            ToolPresence {
                installed: false,
                in_path: true,
                evidence: None,
            },
        );
        let missing = ToolCatalog::all_tools().len() - 2;
        let en = format_tool_inventory_summary(&installed, UiLanguage::English);
        assert_eq!(
            en,
            format!("Tool detection: 1 PATH commands · 1 other detection records · {missing} not detected")
        );
        let zh = format_tool_inventory_summary(&installed, UiLanguage::Chinese);
        assert_eq!(
            zh,
            format!("工具检测：PATH 命令 1 · 其他检测记录 1 · 未检测到 {missing}")
        );
        assert_eq!(zh.lines().count(), 1);
        assert_eq!(en.lines().count(), 1);
        let empty = format_tool_inventory_summary(&HashMap::new(), UiLanguage::English);
        assert!(empty.starts_with("Tool detection: 0 PATH commands · 0 other detection records"));
        for evidence in [
            None,
            Some(crate::detection::ToolEvidence::Config(
                "/fixture/config".into(),
            )),
            Some(crate::detection::ToolEvidence::Plugin(
                "/fixture/plugin".into(),
            )),
        ] {
            installed.get_mut("bat").unwrap().evidence = evidence;
            assert!(
                format_tool_inventory_summary(&installed, UiLanguage::English)
                    .starts_with("Tool detection: 0 PATH commands · 2 other detection records")
            );
        }
    }
    use crate::cli::preset_selection::PresetCatalog;
    use crate::cli::wizard_core::{WizardContext, WizardMode};

    fn homebrew_context() -> crate::platform::packages::InstallContext {
        crate::platform::packages::InstallContext {
            package_manager: crate::platform::packages::PackageManagerBackend::Homebrew,
            supported_os: true,
        }
    }

    #[test]
    fn bootstrap_inventory_distinguishes_local_manual_and_already_installed_tools() {
        use crate::platform::packages::{InstallContext, PackageManagerBackend};
        let context = InstallContext {
            package_manager: PackageManagerBackend::Unsupported,
            supported_os: true,
        };
        let mut installed = HashMap::new();
        installed.insert(
            "bat".into(),
            crate::detection::ToolPresence {
                installed: true,
                in_path: false,
                evidence: None,
            },
        );
        let text = format_tool_inventory(None, &installed, context);
        assert!(text
            .lines()
            .any(|line| line.contains("Starship")
                && line.contains("user-local install; requires curl")));
        assert!(text
            .lines()
            .any(|line| line.contains("eza") && line.contains("manual installation required")));
        assert!(text
            .lines()
            .any(|line| line.contains("~ bat") && !line.contains("manual installation required")));
        assert!(!text.contains('\u{1b}'));
        let unsupported = format_tool_inventory(
            None,
            &installed,
            InstallContext {
                supported_os: false,
                ..context
            },
        );
        assert!(!unsupported.contains("user-local install"));
    }

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
    fn wizard_theme_choices_only_offer_recognized_current_themes() {
        let selector = ThemeSelector::new().unwrap();
        let catalog = build_theme_options(&selector, None);
        assert!(!catalog.is_empty());
        for unknown in ["removed-theme", "PRIVATE\x1b[2J\nTHEME"] {
            assert_eq!(build_theme_options(&selector, Some(unknown)), catalog);
            let label = format_current_theme_label(None, &selector, Some(unknown));
            assert!(label.contains("unrecognized — choose a listed theme"));
            assert!(!label.contains(unknown));
            assert!(!label.contains('\x1b'));
        }
        for theme in selector.all_themes() {
            let options = build_theme_options(&selector, Some(&theme.id));
            assert_eq!(options[0].0, "keep-current");
            assert_eq!(options[0].2, format!("— {}", theme.name));
            assert_eq!(&options[1..], catalog.as_slice());
            assert!(
                format_current_theme_label(None, &selector, Some(&theme.id)).contains(&theme.name)
            );
        }
        assert!(format_current_theme_label(None, &selector, None).contains("not yet applied"));
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

    /// snapshot — tool inventory block rendered through the
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

        let out = format_tool_inventory(Some(&r), &installed, homebrew_context());
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
            let out = format_tool_inventory(Some(&r), &installed, homebrew_context());
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

    /// graceful degrade — zero ANSI when Roles is absent.
    #[test]
    fn tool_inventory_falls_back_to_plain_without_roles() {
        let installed: HashMap<String, crate::detection::ToolPresence> = HashMap::new();
        let out = format_tool_inventory(None, &installed, homebrew_context());
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
        assert!(out.contains("configured font (hint):"));
    }

    #[test]
    fn current_font_label_escapes_configured_terminal_controls() {
        let out = format_current_font_label(None, Some("Personal\n\x1b[2JFont"));
        assert!(out.contains("Personal") && out.contains("Font"));
        assert!(!out.contains('\n') && !out.contains('\x1b'));
    }

    #[test]
    fn current_font_label_falls_back_when_value_absent() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let roles = Roles::new(&ctx);
            for r in [None, Some(&roles)] {
                let out = format_current_font_label(r, None);
                assert!(out.contains("not detected; skip to keep existing settings"));
                assert!(!out.contains("system default"));
            }
        }
    }
}
