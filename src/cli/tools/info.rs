//! Read-only tool discovery. Installation advice describes existing setup
//! policy; it does not probe native versions, permissions, network or activation.
use super::{inventory, supported_tools};
use crate::cli::ui_language::tr;
use crate::{
    cli::tool_selection::ToolCatalog,
    env::SlateEnv,
    error::{Result, SlateError},
    platform::packages::{self, InstallContext, ToolInstallRoute},
};
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct ToolInfo {
    schema_version: u8,
    pub tool: inventory::Tool,
    pub purpose: &'static str,
    pub theme: Option<String>,
    pub warning: Option<&'static str>,
    /// Only the prerequisites for reviewing a sync, not validated compatibility.
    pub sync_review_available: bool,
    pub installation: InstallationAdvice,
    pub next_steps: Vec<String>,
    pub recommended_action: RecommendedAction,
    pub theme_selection_available: bool,
    #[serde(skip)]
    menu_notice: Option<(bool, &'static str)>,
}

#[derive(Serialize)]
pub(super) struct RecommendedAction {
    pub action: &'static str,
    pub label: &'static str,
    pub reason: &'static str,
    pub command: Option<String>,
}

fn recommend(
    tool: &inventory::Tool,
    theme: Option<&str>,
    installation: &InstallationAdvice,
    theme_selection_available: bool,
) -> RecommendedAction {
    let (action, label, reason, command) = match tool.available {
        None => (
            "refresh",
            "Refresh Availability",
            "Availability is unknown; refresh before reviewing changes.",
            Some("slate tools list".into()),
        ),
        Some(false) if installation.guided_install => (
            "install",
            "Install This Tool",
            "Review the single-tool installation first. Installation will still require confirmation and does not configure colors.",
            Some(format!("slate tools install {} --dry-run", tool.id)),
        ),
        Some(false) => (
            "refresh",
            "Refresh Availability",
            "Install this tool separately, then refresh. Slate has no guided installation route for it.",
            Some("slate tools list".into()),
        ),
        Some(true) if theme.is_none() && theme_selection_available => (
            "theme",
            "Choose a Theme First",
            "Theme preview affects detected adapters, not only this tool. Opening it requires separate confirmation; it does not run the installation wizard.",
            Some("slate theme".into()),
        ),
        Some(true) if theme.is_none() => (
            "refresh",
            "Refresh Availability",
            "Saved theme state is unreadable. Inspect it locally before choosing a replacement; no fallback is assumed.",
            Some("slate status".into()),
        ),
        Some(true) if tool.id != "ghostty" && super::super::doctor::has_tool_file_check(tool.id) => (
            "check",
            "Check Theme Setup",
            "Check current wiring before resyncing. This launches no tools and changes no files.",
            Some(format!("slate doctor {}", tool.id)),
        ),
        Some(true) => (
            "preview",
            "Preview Sync",
            "Review the saved theme's proposed file changes before applying anything.",
            Some(format!("slate tools sync {} --dry-run", tool.id)),
        ),
    };
    RecommendedAction {
        action,
        label,
        reason,
        command,
    }
}

#[derive(Serialize)]
pub(super) struct InstallationAdvice {
    pub route: &'static str,
    pub guided_install: bool,
    pub description: String,
}

pub(super) fn validate_id(id: &str) -> Result<()> {
    if supported_tools().contains(&id) {
        Ok(())
    } else {
        Err(SlateError::InvalidConfig(
            "Choose a supported adapter ID from `slate tools list`; nothing was changed.".into(),
        ))
    }
}

/// Compact discovery copy, not an explanation of synchronization side effects.
pub(super) fn menu_purpose(id: &str) -> &'static str {
    if crate::cli::ui_language::current() == crate::config::ui_language::UiLanguage::English {
        return purpose(id);
    }
    match id {
        "ghostty" | "alacritty" | "kitty" => "终端窗口",
        "starship" => "命令提示符：目录与 Git 状态",
        "bat" => "查看文件与代码着色",
        "btop" => "查看 CPU、内存与进程",
        "yazi" => "浏览目录与预览文件",
        "delta" => "查看 Git 代码差异",
        "eza" => "列出文件与目录",
        "lazygit" => "交互管理 Git 修改与历史",
        "fastfetch" => "展示系统信息",
        "zsh-syntax-highlighting" => "输入命令时着色",
        "tmux" => "管理终端会话与分屏",
        "zellij" => "管理标签页与分屏会话",
        "nvim" => "编辑文本与代码",
        "opencode" => "终端 AI 编程助手",
        _ => "工具配色",
    }
}

pub(super) fn purpose(id: &str) -> &'static str {
    match id {
        "ghostty" | "alacritty" | "kitty" => "Terminal emulator · colors for your terminal windows",
        "starship" => "Shell prompt · directory, Git and command context",
        "bat" => "File viewer · syntax-highlighted code in the terminal",
        "btop" => "System monitor · CPU, memory, disks and processes",
        "yazi" => "File manager · navigate files and preview code",
        "delta" => "Git diff viewer · readable code changes",
        "eza" => "Directory listing · file and folder colors",
        "lazygit" => "Interactive Git client · repository changes and history",
        "fastfetch" => "System information · an optional shell startup banner",
        "zsh-syntax-highlighting" => "Zsh input highlighting · colors while typing commands",
        "tmux" => "Terminal multiplexer · sessions, windows and panes",
        "zellij" => "Terminal workspace · tabs, split panes and persistent sessions",
        "nvim" => "Neovim editor · synchronized editor colors",
        "opencode" => "OpenCode terminal interface · synchronized TUI colors",
        _ => "Theme adapter",
    }
}

fn installation(id: &str, context: InstallContext) -> InstallationAdvice {
    let Some(tool) = ToolCatalog::get_tool(id).filter(|tool| tool.installable) else {
        return InstallationAdvice {
            route: "manual",
            guided_install: false,
            description: "If installation is needed, install this tool separately; Slate's guided setup does not install it. Configuration support is separate from installation.".into(),
        };
    };
    let (route, description) = match context.route(id) {
        Ok(ToolInstallRoute::Homebrew) => (
            "homebrew",
            format!("Slate can offer Homebrew package {}. Network and installation permissions have not been checked.", tool.brew_package),
        ),
        Ok(ToolInstallRoute::Apt) => (
            "apt",
            format!("Slate can offer apt package {} with administrator access. Network and installation permissions have not been checked.", packages::apt::package_name(id).expect("apt route has a mapping")),
        ),
        Ok(ToolInstallRoute::UserLocalStarship) => (
            "user-local-starship",
            "Slate can offer a user-local Starship download. This requires curl and network access; neither has been checked here.".into(),
        ),
        Err(reason) => {
            return InstallationAdvice {
                route: "manual",
                guided_install: false,
                description: format!("Slate cannot install this tool automatically: {}. Install manually if needed.", reason.reason()),
            };
        },
    };
    InstallationAdvice {
        route,
        guided_install: true,
        description,
    }
}

pub(super) fn inspect(env: &SlateEnv, id: &str) -> Result<ToolInfo> {
    let inventory = inventory::inspect_one(env, id)?;
    let menu_notice = inventory.menu_notice();
    let tool = inventory
        .tools
        .into_iter()
        .find(|tool| tool.id == id)
        .expect("validated adapter appears in inventory");
    let mut info = describe(
        tool,
        inventory.theme,
        inventory.warning,
        InstallContext::detect(),
        inventory.theme_selection_available,
    );
    info.menu_notice = menu_notice;
    Ok(info)
}

fn describe(
    tool: inventory::Tool,
    theme: Option<String>,
    warning: Option<&'static str>,
    context: InstallContext,
    theme_selection_available: bool,
) -> ToolInfo {
    let installation = installation(tool.id, context);
    let recommended_action = recommend(
        &tool,
        theme.as_deref(),
        &installation,
        theme_selection_available,
    );
    let sync_review_available = theme.is_some() && tool.available == Some(true);
    let mut next_steps = Vec::new();
    if theme.is_none() && !theme_selection_available {
        next_steps.push("Saved theme state could not be read. Inspect `slate status` before choosing a replacement; no fallback is assumed.".into());
    } else if theme.is_none() {
        next_steps.push("Save a recognized theme with `slate theme`, or review a full setup with `slate setup`. Theme selection affects detected adapters, not only this tool.".into());
    }
    if tool.available.is_none() {
        next_steps
            .push("Availability is unknown. Refresh the inventory before reviewing a sync.".into());
    } else if tool.available == Some(false) {
        next_steps.push(if installation.guided_install {
            format!("This tool is not detected. Review a single-tool installation: slate tools install {} --dry-run. Installation does not configure themes or shell hooks; sync never installs software.", tool.id)
        } else {
            "This tool is not detected. Install it separately, make its executable or supported configuration discoverable, then return here.".into()
        });
    }
    if sync_review_available {
        next_steps.push(format!(
            "Review potential writes: slate tools sync {} --dry-run",
            tool.id
        ));
    }
    next_steps.push(tool.hint.into());
    if tool.id == "delta" {
        next_steps.push("Delta uses the same slate-* syntax themes as Bat. Review `slate tools sync bat --dry-run` if those assets are missing; Delta-only sync does not build Bat's cache. Custom BAT_CACHE_PATH or different Bat/Delta versions can affect theme availability.".into());
    }
    match tool.id {
        "delta" => next_steps.push("Sync preserves your Git pager selection; installing Delta and writing its colors do not make it the active pager. Inspect one relevant setting without changing it: git config --show-origin --get core.pager. An unset value is not proof of the effective pager: environment variables, repository settings and command-line options can also select it.".into()),
        "starship" | "bat" | "eza" | "lazygit" | "fastfetch" | "zsh-syntax-highlighting" => {
            next_steps.push("If shell integration is missing, review `slate setup` and open a new shell afterward. Syncing colors alone does not activate shell hooks.".into());
        }
        "nvim" => next_steps.push("For initial editor integration, review `slate setup` after installing Neovim. Existing colors can then be synced independently.".into()),
        "opencode" => next_steps.push("Create an OpenCode TUI configuration through your existing setup first; Slate does not create that entry file.".into()),
        _ => {}
    }
    if super::super::doctor::has_tool_file_check(tool.id) {
        next_steps.push(format!(
            "Check saved theme wiring without launching tools: slate doctor {}",
            tool.id
        ));
    }
    ToolInfo {
        schema_version: 1,
        purpose: purpose(tool.id),
        tool,
        theme,
        warning,
        sync_review_available,
        installation,
        next_steps,
        recommended_action,
        theme_selection_available,
        menu_notice: None,
    }
}

/// The interactive page is an action picker, not the full diagnostic report.
pub(super) fn render_menu(info: &ToolInfo) -> String {
    use std::fmt::Write;
    let theme_label = info.theme.as_deref().map_or_else(
        || {
            if !info.theme_selection_available {
                tr("需检查", "Check settings")
            } else if info.menu_notice.is_some_and(|(warning, _)| warning) {
                tr("无法识别", "Unknown")
            } else {
                tr("未选择", "Not selected")
            }
            .to_owned()
        },
        |id| {
            crate::theme::ThemeRegistry::new()
                .ok()
                .and_then(|registry| registry.get(id).map(|theme| theme.name.clone()))
                .unwrap_or_else(|| crate::cli::file_output::terminal_text(id))
        },
    );
    let status = info.tool.menu_status_label();
    let mut output = format!(
        "\n{} · {} · {}\n{}{}\n",
        info.tool.label,
        status,
        tr("未验证配色生效", "Live colors unverified"),
        tr("Slate 已保存主题：", "Saved Slate theme: "),
        theme_label
    );
    if let Some(warning) = info
        .menu_notice
        .map(|(_, message)| message)
        .or(info.warning)
    {
        let _ = writeln!(output, "{warning}");
    }
    if info
        .tool
        .detection
        .as_ref()
        .is_some_and(|found| found.executable_in_path == Some(false))
    {
        output.push_str(tr(
            "命令不在 PATH 中；修改终端启动设置前，请先查看详情。\n",
            "Command is outside PATH; review details before changing shell startup.\n",
        ));
    }
    if info.sync_review_available && matches!(info.tool.id, "ghostty" | "kitty" | "alacritty") {
        output.push_str(tr(
            "同步可能重载终端窗口，请先预览改动。\n",
            "Sync may reload terminal windows; preview changes first.\n",
        ));
    }
    output
}

pub(super) fn render_menu_details(info: &ToolInfo) -> String {
    use std::fmt::Write;
    let mut output = render_menu(info);
    let _ = writeln!(output, "\n{}\n", menu_purpose(info.tool.id));
    if let Some(found) = &info.tool.detection {
        let kind = match found.kind {
            "executable" => tr("程序", "Executable"),
            "app_bundle" => tr("应用", "Application"),
            "configuration" => tr("配置文件", "Configuration"),
            "plugin" => tr("插件", "Plugin"),
            _ => tr("检测记录", "Detection record"),
        };
        let _ = writeln!(
            output,
            "{kind} · {}",
            crate::cli::file_output::terminal_text(&found.path)
        );
        if found.path_is_lossy {
            output.push_str(tr(
                "路径显示有损，不可直接复制。\n",
                "Lossy path display; do not copy.\n",
            ));
        }
        output.push_str(match found.executable_in_path {
            Some(true) => tr(
                "仅确认 PATH 中存在命令，未运行或检查版本。\n",
                "Found in PATH; not run or version-checked.\n",
            ),
            Some(false) => tr(
                "仅在备用位置找到程序，当前 Shell 未必能直接运行。\n",
                "Found outside PATH; its bare command may not work in this shell.\n",
            ),
            None => tr(
                "此路径不能证明命令可运行或配色已启用。\n",
                "This path does not prove a runnable command or active colors.\n",
            ),
        });
    }
    output.push_str(if info.installation.guided_install {
        tr("\n支持引导安装，仍需单独确认；安装与配色分开。\n", "\nGuided installation requires separate confirmation; colors are configured separately.\n")
    } else {
        tr("\n不提供引导安装；如需安装，请查看完整说明。\n", "\nNo guided installation; see the full report for installation guidance.\n")
    });
    if let Some(command) = &info.recommended_action.command {
        let _ = writeln!(
            output,
            "\n{}{}",
            tr("建议下一步：", "Suggested next step: "),
            crate::cli::file_output::terminal_text(command)
        );
    }
    let _ = writeln!(
        output,
        "{}slate tools info {}\n",
        tr("完整说明：", "Full report: "),
        info.tool.id
    );
    output.push_str(tr(
        "只读查看，未修改文件或启动工具。\n",
        "Read-only; no files changed or tools launched.\n",
    ));
    output
}

pub(super) fn render(info: &ToolInfo) -> String {
    use std::fmt::Write;
    let mut output = format!(
        "{} · {}\n{}\nAvailability: {} · not proof of configuration or live theme activation\nSaved theme: {}\nInstallation: {}\n",
        info.tool.label,
        info.tool.id,
        info.purpose,
        info.tool.status_label(),
        info.theme.as_deref().unwrap_or("not available"),
        info.installation.description,
    );
    if let Some(warning) = info.warning {
        let _ = writeln!(output, "{warning}");
    }
    if let Some(found) = &info.tool.detection {
        let _ = writeln!(
            output,
            "Detected via {}: {}{}",
            found.kind,
            super::super::file_output::terminal_text(&found.path),
            if found.path_is_lossy {
                " (lossy display; not an exact path)"
            } else {
                ""
            }
        );
        output.push_str(match found.executable_in_path {
            Some(true) => "Executable found in the captured process PATH; it was not launched or version-checked.\n",
            Some(false) => "Executable found only in a fallback location; its bare command may not work in this shell. Review PATH before expecting shell integration to work.\n",
            None => "This is not executable-in-PATH evidence. An app, configuration or plugin path does not prove a runnable command or active integration.\n",
        });
    }
    let _ = writeln!(
        output,
        "Recommended: {}\n{}",
        info.recommended_action.label, info.recommended_action.reason
    );
    if let Some(command) = &info.recommended_action.command {
        let _ = writeln!(output, "  {command}");
    }
    output.push_str("Next steps:\n");
    for step in &info.next_steps {
        let _ = writeln!(output, "  • {step}");
    }
    output.push_str("This inspection changed no files and launched no tools or installers.\n");
    output
}

pub(super) fn print(env: &SlateEnv, id: &str, json: bool) -> Result<()> {
    let info = inspect(env, id)?;
    super::super::file_output::write_output(&if json {
        format!("{}\n", serde_json::to_string_pretty(&info)?)
    } else {
        render(&info)
    })
}

#[cfg(test)]
mod tests;
