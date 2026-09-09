//! Tool discovery, reviewed single-tool installation and selected theme sync.
//! Listing and review never initialize the profile, sound, or package manager.
use super::ui_language::tr;
use crate::{
    adapter::{ApplyStrategy, ToolRegistry},
    env::SlateEnv,
    error::{Result, SlateError},
};
use clap::{Args, Subcommand};
use std::io::IsTerminal;

mod action_feedback;
mod info;
mod install;
mod inventory;
mod sync;
mod workflows;
use action_feedback::ActionEffect;

#[derive(Args)]
pub struct ToolsOptions {
    #[command(subcommand)]
    command: Option<ToolsCommand>,
}

#[derive(Subcommand)]
enum ToolsCommand {
    /// Install one missing tool without applying themes or configuring shell hooks
    Install {
        /// Installable tool ID; already detected tools are not reinstalled
        tool: String,
        /// Review the installation route without launching an installer
        #[arg(long, conflicts_with = "yes")]
        dry_run: bool,
        /// Confirm installation without an interactive prompt
        #[arg(long)]
        yes: bool,
        /// Emit a read-only installation review as JSON
        #[arg(long, requires = "dry_run")]
        json: bool,
    },
    /// Explain one supported tool and its next steps without changing anything
    Info {
        /// Adapter ID shown by the tools list command
        tool: String,
        #[arg(long)]
        json: bool,
    },
    /// Inspect availability without writing settings or launching tools
    List {
        #[arg(long)]
        json: bool,
    },
    /// Sync the saved theme to selected tools; never install software
    Sync {
        /// Adapter IDs shown by the tools list command
        #[arg(required = true, num_args = 1..)]
        tools: Vec<String>,
        /// Review potential configuration paths without changing anything
        #[arg(long, conflicts_with = "yes")]
        dry_run: bool,
        /// Confirm synchronization without an interactive prompt
        #[arg(long)]
        yes: bool,
        /// Emit a read-only synchronization plan as JSON
        #[arg(long, requires = "dry_run")]
        json: bool,
    },
}

pub fn supported_tools() -> Vec<&'static str> {
    ToolRegistry::default()
        .adapters()
        .iter()
        .filter(|adapter| {
            adapter.apply_strategy() != ApplyStrategy::DetectAndInstall
                && adapter.tool_name() != "ls_colors"
        })
        .map(|adapter| adapter.tool_name())
        .collect()
}

pub fn installable_tools() -> Vec<&'static str> {
    let supported = supported_tools();
    super::tool_selection::ToolCatalog::installable_tools()
        .into_iter()
        .filter(|tool| supported.contains(&tool.id))
        .map(|tool| tool.id)
        .collect()
}

pub fn validate_options(options: &ToolsOptions) -> Result<()> {
    match &options.command {
        Some(ToolsCommand::Install { tool, .. }) => install::validate_name(tool)?,
        Some(ToolsCommand::Sync { tools, .. }) => sync::validate_names(tools)?,
        Some(ToolsCommand::Info { tool, .. }) => info::validate_id(tool)?,
        _ => {}
    }
    Ok(())
}

pub fn handle(env: &SlateEnv, options: &ToolsOptions) -> Result<()> {
    validate_options(options)?;
    match &options.command {
        Some(ToolsCommand::Install {
            tool,
            dry_run,
            yes,
            json,
        }) => install::handle(env, tool, *dry_run, *yes, *json),
        Some(ToolsCommand::Info { tool, json }) => info::print(env, tool, *json),
        Some(ToolsCommand::List { json }) => inventory::print(env, *json),
        Some(ToolsCommand::Sync {
            tools,
            dry_run,
            yes,
            json,
        }) => sync::handle(env, tools, *dry_run, *yes, *json),
        None => handle_menu(env),
    }
}

pub(super) fn handle_menu(env: &SlateEnv) -> Result<()> {
    menu(env, false)
}

pub(super) fn handle_hub_menu(env: &SlateEnv) -> Result<()> {
    menu(env, true)
}

fn menu(env: &SlateEnv, from_hub: bool) -> Result<()> {
    if !interactive() {
        return inventory::print(env, false);
    }
    let mut last_opened = None;
    loop {
        let report = inventory::inspect(env)?;
        cliclack::intro(tr("slate · 工具配色", "slate · Tool Themes"))?;
        super::file_output::write_output(tr(
            "浏览不修改设置；检测到工具不代表配色已生效。\n",
            "Browsing changes no settings; detection does not confirm active colors.\n",
        ))?;
        if let Some((warning, message)) = report.menu_notice() {
            if warning {
                cliclack::log::warning(message)?;
            } else {
                super::file_output::write_output(&format!("\n  {message}\n\n"))?;
            }
        }
        let mut menu = super::menu::select(tr("选择工具或查看全部支持", "Choose a Tool"))
            .escape_value("quit")
            .max_rows(8);
        if let Some(id) = last_opened {
            menu = menu.initial_value(id);
        }
        // Discovery is useful before theme selection too. The detail page owns
        // prerequisite checks and asks separately before opening global preview.
        for tool in &report.tools {
            if tool.available == Some(true) {
                let hint = if Some(tool.id) == last_opened {
                    format!(
                        "{}{} · {}",
                        tr("刚刚查看 · ", "Recent · "),
                        tool.menu_status_label(),
                        info::menu_purpose(tool.id)
                    )
                } else {
                    format!(
                        "{} · {}",
                        tool.menu_status_label(),
                        info::menu_purpose(tool.id)
                    )
                };
                menu = menu.item(tool.id, tool.label, hint);
            }
        }
        let action = menu
            .item(
                "browse",
                tr("浏览全部工具", "All Tools"),
                tr(
                    "查看全部适配工具、用途和后续操作 · 不安装",
                    "Browse supported integrations",
                ),
            )
            .item(
                "workflows",
                tr("按用途找工具", "Browse by Workflow"),
                tr(
                    "窗口、提示符、文件、开发或分屏 · 浏览不安装",
                    "Terminal, prompt, files, development and panes",
                ),
            )
            .item(
                "check",
                tr("检查配色配置", "Check Theme Configuration"),
                tr(
                    "只读检查配色配置 · 不启动工具",
                    "Inspect files without launching tools",
                ),
            )
            .item(
                "list",
                tr("查看工具总览", "Tool Inventory"),
                tr("查看检测结果和后续操作 · 只读", "Detection and next steps"),
            )
            .item(
                "setup",
                tr("引导设置", "Guided Setup"),
                tr(
                    "可安装工具并配置 Shell 接入 · 另行审阅确认",
                    "Installation and shell setup require separate confirmation",
                ),
            )
            .item(
                "quit",
                if from_hub {
                    tr("返回首页", "Back")
                } else {
                    tr("退出工具菜单", "Quit")
                },
                "",
            )
            .interact()
            .map_err(input_error)?;
        // Preserve position without moving items. Do not make installation the
        // default action merely because the user just returned from setup.
        if action != "setup" {
            last_opened = Some(action);
        }
        match action {
            "browse" => browse_menu(env)?,
            "workflows" => workflows::browse(env)?,
            "check" => check_menu(env)?,
            "list" => {
                let report = inventory::inspect(env)?;
                let mut page = super::menu::ReadOnlyPage::enter()?;
                let result = (|| -> Result<()> {
                    page.view(
                        &report.menu_text(),
                        tr("工具总览", "Tool Inventory"),
                        tr("返回工具菜单", "Back to Tools"),
                    )
                    .map_err(input_error)?;
                    Ok(())
                })();
                page.finish(result)?;
            }
            "setup" => super::setup::handle_with_env(false, false, None, env)?,
            "quit" => return Ok(()),
            id => {
                tool_menu(env, id, tr("返回工具菜单", "Back to Tools"))?;
            }
        }
    }
}

fn browse_menu(env: &SlateEnv) -> Result<()> {
    browse_group(env, None)
}

fn browse_group(env: &SlateEnv, group: Option<&workflows::Group>) -> Result<()> {
    let mut last_opened = None;
    loop {
        let report = match group {
            Some(group) => inventory::inspect_group(env, group.tool_ids())?,
            None => inventory::inspect(env)?,
        };
        super::file_output::write_output(tr(
            "包含尚未安装的工具；打开页面不会安装或同步。\n",
            "Includes tools not yet installed; browsing does not install or sync.\n",
        ))?;
        let title = group.map_or_else(
            || tr("全部支持的工具", "All Supported Tools").to_owned(),
            |group| format!("{} · {}", group.menu_label(), tr("工具", "Tools")),
        );
        let mut menu = super::menu::select(title).escape_value("back").max_rows(8);
        if let Some(id) = last_opened {
            menu = menu.initial_value(id);
        }
        let tools = report.tools.iter();
        for tool in tools.filter(|tool| group.is_none_or(|group| group.contains(tool.id))) {
            let availability = tool.menu_status_label();
            let recent = if Some(tool.id) == last_opened {
                tr("刚刚查看 · ", "Recent · ")
            } else {
                ""
            };
            menu = menu.item(
                tool.id,
                tool.label,
                format!("{recent}{availability} · {}", info::menu_purpose(tool.id)),
            );
        }
        let id = menu
            .item(
                "back",
                if group.is_some() {
                    tr("返回用途分类", "Back to Workflows")
                } else {
                    tr("返回工具菜单", "Back to Tools")
                },
                "",
            )
            .interact()
            .map_err(input_error)?;
        if id == "back" {
            return Ok(());
        }
        last_opened = Some(id);
        let back_label = group.map_or_else(
            || tr("返回工具列表", "Back to Tool List").to_owned(),
            |group| format!("{}{}", tr("返回", "Back to "), group.menu_label()),
        );
        tool_menu(env, id, &back_label)?;
    }
}

fn tool_menu(env: &SlateEnv, id: &str, back_label: &str) -> Result<()> {
    let mut last_inspection = None;
    loop {
        let details = info::inspect(env, id)?;
        super::file_output::write_output(&info::render_menu(&details))?;
        // Keep the reader's place after inspection, but never preselect a
        // mutating action merely because it was just executed. A preview may
        // disappear when prerequisites change in another terminal.
        let selected = match last_inspection {
            Some("preview") if details.sync_review_available => "preview",
            Some("check") if super::doctor::has_tool_file_check(id) => "check",
            Some("details") => "details",
            Some("refresh") => "refresh",
            _ => details.recommended_action.action,
        };
        let mut menu = super::menu::select(format!(
            "{} · {}",
            details.tool.label,
            tr("选择操作", "Choose an Action")
        ))
        .escape_value("back")
        .max_rows(8)
        .initial_value(selected);
        if details.theme.is_none() && details.theme_selection_available {
            menu = menu.item(
                "theme",
                tr("先选择主题", "Choose a Theme First"),
                tr(
                    "单独确认 · 主题预览可能影响多个已检测到的工具",
                    "Separate confirmation; preview may affect several tools",
                ),
            );
        }
        if details.sync_review_available {
            menu = menu
                .item(
                    "preview",
                    tr("预览同步改动", "Preview Sync"),
                    tr(
                        "查看可能修改的文件 · 不写入、不运行工具",
                        "Review potential writes",
                    ),
                )
                .item(
                    "sync",
                    tr("同步已保存主题", "Sync Saved Theme"),
                    tr("先审阅再确认 · 默认不执行同步", "Review before confirming"),
                );
        }
        if super::doctor::has_tool_file_check(id) {
            menu = menu.item(
                "check",
                tr("检查配色配置", "Check Theme Configuration"),
                tr(
                    "只读检查配色文件 · 不启动工具",
                    "Inspect files without launching tools",
                ),
            );
        }
        if details.tool.available == Some(false) && details.installation.guided_install {
            menu = menu.item(
                "install",
                tr("安装此工具", "Install This Tool"),
                tr(
                    "先审阅安装方式和依赖 · 不设置配色或终端启动项",
                    "Review installation; theme and shell setup are separate",
                ),
            );
        }
        let action = menu
            .item(
                "refresh",
                tr("刷新检测结果", "Refresh Detection"),
                tr(
                    "重新检测 · 适合在别处安装工具后使用",
                    "Refresh after installing elsewhere",
                ),
            )
            .item(
                "details",
                tr("查看详细信息", "Details"),
                tr(
                    "查看检测路径、安装指引和完整说明 · 只读",
                    "Detection paths and full guidance",
                ),
            )
            .item("back", back_label, "")
            .interact()
            .map_err(input_error)?;
        last_inspection = match action {
            "preview" | "check" | "details" | "refresh" => Some(action),
            _ => None,
        };
        match action {
            "theme" => {
                show_action_result(
                    super::theme_handoff::open(env, super::theme_handoff::Origin::Tool),
                    ActionEffect::Configuration,
                )?;
                // Return to this tool, reread prerequisites, and never sync it
                // implicitly after the user saves or cancels the theme picker.
            }
            "preview" => show_tool_report(
                sync::handle_menu(env, id, true),
                details.tool.label,
                tr("同步预览，未执行修改", "Sync Preview · No Changes"),
            )?,
            "sync" => show_action_result(
                sync::handle_menu(env, id, false),
                ActionEffect::Configuration,
            )?,
            "check" => show_action_result(
                show_file_check(env, id, tr("返回工具页面", "Back to Tool")),
                ActionEffect::ReadOnly,
            )?,
            "install" => show_action_result(
                install::handle(env, id, false, false, false),
                ActionEffect::Installation,
            )?,
            "refresh" => {}
            "details" => {
                let mut page = super::menu::ReadOnlyPage::enter()?;
                let result = page
                    .view(
                        &info::render_menu_details(&details),
                        &format!("{} · {}", details.tool.label, tr("详情", "Details")),
                        tr("返回工具页面", "Back to Tool"),
                    )
                    .map_err(input_error);
                page.finish(result)?;
            }
            _ => return Ok(()),
        }
    }
}

/// Read-only reports have one acknowledgement, never an implicit apply action.
fn show_tool_report(result: Result<()>, label: &str, title: &str) -> Result<()> {
    let reviewed = result.is_ok();
    show_action_result(result, ActionEffect::ReadOnly)?;
    if reviewed {
        super::menu::select(format!("{label} · {title}"))
            .item((), tr("返回工具页面", "Back to Tool"), "")
            .escape_value(())
            .interact()?;
    }
    Ok(())
}

/// Interactive details retain recoverable action errors. Direct CLI commands
/// still fail normally; cancellation and IO failures must not loop the menu.
fn show_action_result(result: Result<()>, effect: ActionEffect) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error @ (SlateError::UserCancelled | SlateError::IOError(_))) => Err(error),
        Err(error) => {
            cliclack::log::warning(format!(
                "{}: {}",
                tr("工具操作已停止", "Tool action stopped"),
                super::file_output::terminal_text(&error.to_string())
            ))?;
            super::file_output::write_output(&format!("{}\n", effect.recovery_note()))?;
            Ok(())
        }
    }
}

fn check_menu(env: &SlateEnv) -> Result<()> {
    let mut last_checked = None;
    loop {
        let mut menu = super::menu::select(tr("选择要检查的配色配置", "Choose a Theme Check"))
            .escape_value("quit")
            .max_rows(8);
        if let Some(target) = last_checked {
            menu = menu.initial_value(target);
        }
        for (id, label, hint) in super::doctor::TOOL_FILE_CHECKS {
            menu = menu.item(id, label, super::doctor::tool_file_check_hint(id, hint));
        }
        let target = menu
            .item("quit", tr("返回工具菜单", "Back to Tools"), "")
            .interact()
            .map_err(input_error)?;
        if target == "quit" {
            return Ok(());
        }
        show_file_check(env, target, tr("返回检查列表", "Back to Checks"))?;
        last_checked = Some(target);
    }
}

fn show_file_check(env: &SlateEnv, target: &str, back: &str) -> Result<()> {
    super::doctor::show_tool_files(env, target, back)
}

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn input_error(error: std::io::Error) -> SlateError {
    if error.kind() == std::io::ErrorKind::Interrupted {
        SlateError::UserCancelled
    } else {
        SlateError::IOError(error)
    }
}
