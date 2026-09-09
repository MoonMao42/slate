use super::*;
use crate::{config::ConfigManager, theme::ThemeRegistry};
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct Inventory {
    schema_version: u8,
    pub theme: Option<String>,
    pub warning: Option<&'static str>,
    #[serde(skip)]
    theme_notice: ThemeNotice,
    pub theme_selection_available: bool,
    pub tools: Vec<Tool>,
}

enum ThemeNotice {
    Ready,
    Missing,
    Unknown,
    Unreadable,
}

impl Inventory {
    pub(super) fn menu_text(&self) -> String {
        use std::fmt::Write;
        let mut output = format!(
            "\n  {} · {}\n\n",
            tr("工具总览", "Tool Inventory"),
            super::super::file_output::terminal_text(self.theme.as_deref().unwrap_or(
                match self.theme_notice {
                    ThemeNotice::Unknown | ThemeNotice::Unreadable =>
                        tr("主题需检查", "Check saved theme"),
                    _ => tr("未选择主题", "No theme selected"),
                },
            )),
        );
        for tool in &self.tools {
            let _ = writeln!(output, "  {} · {}", tool.label, tool.menu_status_label());
        }
        output.push_str(tr(
            "\n  仅检测结果，不代表配色已生效。\n",
            "\n  Detection only; active colors are not verified.\n",
        ));
        if let Some((_, notice)) = self.menu_notice() {
            let _ = writeln!(output, "  {notice}");
        }
        output
    }

    pub(super) fn menu_notice(&self) -> Option<(bool, &'static str)> {
        match self.theme_notice {
            ThemeNotice::Ready => None,
            ThemeNotice::Missing => Some((
                false,
                tr("尚未选择主题，仍可浏览和检查工具；同步前再选择主题即可。", "No theme selected. Browse or inspect tools now; choose a theme before syncing."),
            )),
            ThemeNotice::Unknown => Some((
                true,
                tr("已保存的主题无法识别；同步前请选择可用主题，不会自动替换为默认主题。", "Saved theme is unknown. Choose a supported theme before syncing; no default is substituted."),
            )),
            ThemeNotice::Unreadable => {
                Some((true, tr("已保存主题无法读取；请先检查配置，不会直接替换主题。", "Saved theme is unreadable. Check configuration before replacing it.")))
            }
        }
    }
}

#[derive(Serialize)]
pub(super) struct Tool {
    pub id: &'static str,
    pub label: &'static str,
    pub available: Option<bool>,
    pub detection: Option<Detection>,
    pub hint: &'static str,
}

impl Tool {
    /// Interactive wording only; CLI/JSON detection evidence stays unchanged.
    pub(super) fn menu_status_label(&self) -> &'static str {
        if crate::cli::ui_language::current() == crate::config::ui_language::UiLanguage::English {
            return self.status_label();
        }
        match self.status_label() {
            "unknown" => "检测状态未知",
            "not detected" => "未检测到",
            "in PATH" => "命令可找到",
            "outside PATH" => "找到程序，但不在 PATH 中",
            "app found" => "找到应用",
            "config found" => "仅找到配置",
            "plugin found" => "找到插件",
            _ => "已检测到",
        }
    }

    pub(super) fn status_label(&self) -> &'static str {
        match self.available {
            None => "unknown",
            Some(false) => "not detected",
            Some(true) => match self.detection.as_ref() {
                Some(found) => match (found.kind, found.executable_in_path) {
                    ("executable", Some(true)) => "in PATH",
                    ("executable", Some(false)) => "outside PATH",
                    ("app_bundle", _) => "app found",
                    ("configuration", _) => "config found",
                    ("plugin", _) => "plugin found",
                    _ => "detected",
                },
                None => "detected",
            },
        }
    }
}

#[derive(Serialize)]
pub(super) struct Detection {
    pub kind: &'static str,
    pub path: String,
    pub path_is_lossy: bool,
    /// Only executable evidence makes a statement about shell PATH.
    pub executable_in_path: Option<bool>,
}

impl Detection {
    pub(super) fn from_presence(presence: &crate::detection::ToolPresence) -> Option<Self> {
        use crate::detection::ToolEvidence;
        let (kind, path, executable) = match presence.evidence.as_ref()? {
            ToolEvidence::Executable(path) => ("executable", path, true),
            ToolEvidence::AppBundle(path) => ("app_bundle", path, false),
            ToolEvidence::Config(path) => ("configuration", path, false),
            ToolEvidence::Plugin(path) => ("plugin", path, false),
        };
        Some(Self {
            kind,
            path: path.to_string_lossy().into_owned(),
            path_is_lossy: path.to_str().is_none(),
            executable_in_path: executable.then_some(presence.in_path),
        })
    }
}

/// Concise interactive guidance; the CLI/JSON contract below stays unchanged.
pub(super) fn menu_sync_hint(id: &str) -> &'static str {
    if crate::cli::ui_language::current() == crate::config::ui_language::UiLanguage::English {
        return hint(id);
    }
    match id {
        "btop" => "保存配色后重新打开；旧会话退出时可能写回旧配色，届时需再次同步。",
        "zellij" => "同步窗格、标签栏、状态栏及明暗主题选择；布局或启动参数可能覆盖，未验证当前会话。",
        "yazi" => "同步界面与代码预览配色后重新打开；个人主题覆盖仍优先。",
        "bat" => "同步主题资源并重建缓存；需已启用 Shell 集成。",
        "lazygit" => "仅同步界面配色；需更新后的 Slate Shell 集成并从新 Shell 打开，保留自定义 LG_CONFIG_FILE。",
        "fastfetch" => "同步 Slate 预设布局与配色，不合并个人布局；需已启用 Slate Shell 包装命令。",
        "eza" | "zsh-syntax-highlighting" => "同步生成的配色；需已启用 Slate Shell 集成。",
        "starship" => "调整现有 starship.toml 的配色；配置缺失时先使用引导设置。",
        "nvim" => "通知已有的 Slate 编辑器集成；不安装加载器。",
        "opencode" => "更新已有 TUI 配置，不新建入口文件；保存后重新打开。",
        "ghostty" | "alacritty" | "kitty" => "同步终端配置；可能刷新正在运行的窗口，并重新应用已保存的外观。",
        "tmux" => "同步配置；支持时刷新当前 tmux 会话。",
        "delta" => "同步 Git 差异配色与引用；需现有 .gitconfig 和 Slate 的 Bat 主题缓存，不安装或启用 Delta，不改分页器选择。",
        _ => "同步已保存主题。",
    }
}

pub(super) fn hint(id: &str) -> &'static str {
    match id {
        "btop" => "sync palette and config · reopen btop afterward",
        "zellij" => {
            "sync pane/tab/status colors and static/dark/light choices · existing layouts or CLI options can override · live session unverified"
        }
        "yazi" => {
            "sync native flavor and code-preview colors · personal theme overrides win · reopen Yazi"
        }
        "bat" => "sync theme assets and rebuild cache · existing shell integration required",
        "lazygit" => {
            "sync GUI colors only · updated Slate shell integration required · custom LG_CONFIG_FILE is preserved · reopen from a new shell"
        }
        "fastfetch" => {
            "sync Slate's preset layout and colors · personal layouts are not merged · existing Slate shell wrapper required"
        }
        "eza" | "zsh-syntax-highlighting" => {
            "sync generated colors · existing Slate shell integration required"
        }
        "starship" => "recolor existing starship.toml · use guided setup if missing",
        "nvim" => "notify an existing Slate editor integration · does not install the loader",
        "opencode" => "update an existing TUI config · does not create a new one",
        "ghostty" | "alacritty" | "kitty" => {
            "sync terminal config · may reload running windows and reapply saved appearance"
        }
        "tmux" => "sync config · reload the current tmux session when supported",
        "delta" => {
            "sync Git diff colors and include · requires Slate's generated Bat theme cache · existing .gitconfig required · preserves pager selection · does not install or activate Delta"
        }
        _ => "sync the saved theme",
    }
}

pub(super) fn inspect(env: &SlateEnv) -> Result<Inventory> {
    inspect_selected(env, None, crate::detection::detect_tool_presence_with_env)
}

pub(super) fn inspect_one(env: &SlateEnv, id: &str) -> Result<Inventory> {
    inspect_group(env, &[id])
}

pub(super) fn inspect_group(env: &SlateEnv, ids: &[&str]) -> Result<Inventory> {
    inspect_selected(
        env,
        Some(ids),
        crate::detection::detect_tool_presence_with_env,
    )
}

fn inspect_selected(
    env: &SlateEnv,
    selected: Option<&[&str]>,
    mut detect: impl FnMut(&str, &SlateEnv) -> crate::detection::ToolPresence,
) -> Result<Inventory> {
    if let Some(ids) = selected {
        for id in ids {
            super::info::validate_id(id)?;
        }
    }
    let themes = ThemeRegistry::new()?;
    let (theme, warning, theme_selection_available, theme_notice) = match ConfigManager::from_env_paths(env)
        .get_current_theme()
    {
        Ok(Some(id)) if themes.get(&id).is_some() => (Some(id), None, true, ThemeNotice::Ready),
        Ok(None) => (
            None,
            Some("Choose and save a theme first with `slate theme`, or use guided setup."),
            true,
            ThemeNotice::Missing,
        ),
        Ok(Some(_)) => (
            None,
            Some(
                "Saved theme is unknown; choose a recognized theme before syncing. No fallback is assumed.",
            ),
            true,
            ThemeNotice::Unknown,
        ),
        Err(_) => (
            None,
            Some(
                "Saved theme is unreadable; inspect it before choosing a replacement. No fallback is assumed.",
            ),
            false,
            ThemeNotice::Unreadable,
        ),
    };
    let tools = super::supported_tools()
        .into_iter()
        .filter(|id| selected.is_none_or(|selected| selected.contains(id)))
        .map(|id| {
            let presence = detect(id, env);
            Tool {
                id,
                label: crate::cli::tool_selection::ToolCatalog::get_tool(id)
                    .map(|tool| tool.label)
                    .unwrap_or(id),
                // Adapter readiness may launch a native version check (Neovim). The
                // inventory promises file-only detection, not runtime compatibility.
                available: Some(presence.installed),
                detection: Detection::from_presence(&presence),
                hint: hint(id),
            }
        })
        .collect();
    Ok(Inventory {
        schema_version: 1,
        theme,
        warning,
        theme_notice,
        theme_selection_available,
        tools,
    })
}

pub(super) fn print(env: &SlateEnv, json: bool) -> Result<()> {
    use std::fmt::Write;
    let report = inspect(env)?;
    if json {
        return super::super::file_output::write_output(&format!(
            "{}\n",
            serde_json::to_string_pretty(&report)?
        ));
    }
    let mut output = format!(
        "Tools · saved theme: {}\nAvailability is not proof of configuration or live theme activation.\n\n",
        report.theme.as_deref().unwrap_or("not available")
    );
    for tool in &report.tools {
        let status = tool.status_label();
        let _ = writeln!(output, "  {:24} {:13} {}", tool.id, status, tool.hint);
    }
    if let Some(warning) = &report.warning {
        let _ = writeln!(output, "\n{warning}");
    }
    output.push_str("\nDiscover one tool and its next steps: slate tools info yazi\nCheck theme wiring (no tools launched): slate doctor btop\nReview one tool's theme writes: slate tools sync btop --dry-run\nReview one tool's installation: slate tools install btop --dry-run\nAdd shell/editor startup hooks or review a full setup: slate setup\nInstallation, theme sync and startup integration are separate steps; previews make no changes.\n");
    super::super::file_output::write_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::ToolPresence;

    #[test]
    fn inventory_menu_distinguishes_first_use_from_invalid_theme_without_changing_json() {
        for (state, warning) in [
            ("missing", Some(false)),
            ("nord", None),
            ("unknown", Some(true)),
            ("unreadable", Some(true)),
        ] {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            if state != "missing" {
                std::fs::create_dir_all(env.config_dir()).unwrap();
                if state == "unreadable" {
                    std::fs::create_dir(env.managed_file("current")).unwrap();
                } else {
                    std::fs::write(env.managed_file("current"), state).unwrap();
                }
            }
            let report =
                inspect_selected(&env, Some(&[]), |_, _| panic!("no tool probes needed")).unwrap();
            assert_eq!(report.menu_notice().map(|(warning, _)| warning), warning);
            assert_eq!(report.theme_selection_available, state != "unreadable");
            let menu = report.menu_text();
            assert!(menu.contains("仅检测结果，不代表配色已生效"));
            if matches!(state, "unknown" | "unreadable") {
                assert!(!menu.contains("未选择主题"), "{menu}");
                assert!(
                    menu.contains("无法识别") || menu.contains("无法读取"),
                    "{menu}"
                );
            }
            let json = serde_json::to_value(&report).unwrap();
            assert!(json.get("theme_notice").is_none());
            assert_eq!(json.as_object().unwrap().len(), 5);
            if state == "missing" {
                assert!(report.warning.unwrap().contains("Choose and save a theme"));
                assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
            }
        }
    }

    #[test]
    fn grouped_inventory_probes_each_selected_tool_once_and_validates_before_detection() {
        let root = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(root.path().to_owned());
        let mut calls = Vec::new();
        let report = inspect_selected(&env, Some(&["tmux", "zellij", "tmux"]), |id, _| {
            calls.push(id.to_owned());
            ToolPresence::missing()
        })
        .unwrap();
        let expected: Vec<_> = supported_tools()
            .into_iter()
            .filter(|id| ["tmux", "zellij"].contains(id))
            .collect();
        assert_eq!(calls, expected);
        assert_eq!(
            report.tools.iter().map(|tool| tool.id).collect::<Vec<_>>(),
            expected
        );
        assert!(report.theme.is_none());
        assert!(
            inspect_selected(&env, Some(&["tmux", "unknown"]), |_, _| panic!(
                "must validate all IDs before probing"
            ))
            .is_err()
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn selected_tool_inventory_probes_only_its_target_and_preserves_theme_state() {
        let root = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(root.path().to_owned());
        std::fs::create_dir_all(env.config_dir()).unwrap();
        std::fs::write(env.managed_file("current"), "nord\n").unwrap();
        let mut calls = Vec::new();
        for id in supported_tools() {
            calls.clear();
            let selected = inspect_selected(&env, Some(&[id]), |name, _| {
                calls.push(name.to_owned());
                ToolPresence::missing()
            })
            .unwrap();
            assert_eq!(calls, [id]);
            assert_eq!(selected.tools.len(), 1);
            assert_eq!(selected.tools[0].id, id);
            assert_eq!(selected.theme.as_deref(), Some("nord"));
            assert!(selected.theme_selection_available);
        }
        calls.clear();
        let all = inspect_selected(&env, None, |name, _| {
            calls.push(name.to_owned());
            ToolPresence::missing()
        })
        .unwrap();
        assert_eq!(calls, supported_tools());
        assert_eq!(all.tools.len(), supported_tools().len());
        assert!(
            inspect_selected(&env, Some(&["not-an-adapter"]), |_, _| panic!(
                "invalid target probed"
            ))
            .is_err()
        );
        assert_eq!(
            std::fs::read(env.managed_file("current")).unwrap(),
            b"nord\n"
        );
        assert_eq!(std::fs::read_dir(env.config_dir()).unwrap().count(), 1);
        assert!(!env.slate_cache_dir().exists());
    }
}
