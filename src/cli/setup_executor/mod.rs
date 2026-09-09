//! Setup execution: runs the planned installations and applies configurations.
//! Handles partial failures and tracks results.

mod font_install;
mod font_stage;
#[cfg(test)]
mod homebrew_tests;
mod integration;
mod plan;
mod shell_loader;
mod starship;
mod tool_install;

use crate::brand::events::{dispatch, BrandEvent};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::failure_handler::{ExecutionSummary, InstallStatus, ToolInstallResult};
use crate::cli::file_output::terminal_text;
use crate::cli::wizard_support::wording as tr;
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::platform::packages::homebrew;

pub(crate) use font_install::chain::install_catalog as install_catalog_font;
pub(crate) use font_install::resolve_font_family_with_env;
#[cfg(test)]
use integration::setup_shell_integration_with_env;
pub(crate) use integration::{ensure_tool_configs, theme_apply_issues};
pub(crate) use plan::prepare_setup_with_env;
pub(crate) use tool_install::install_tool;
pub(crate) use tool_install::{install_planned as install_planned_tool, ToolInstallMethod};

/// Typed uncertainty must be checked before any message-based fallback.
pub(crate) fn installation_fallback_allowed(error: &crate::error::SlateError) -> bool {
    !matches!(
        error,
        crate::error::SlateError::HomebrewInstallUncertain(_)
            | crate::error::SlateError::AptInstallUncertain(_)
            | crate::error::SlateError::StarshipInstallUncertain(_)
    )
}

/// Execute the setup based on wizard selections with injected SlateEnv (preferred)
pub fn execute_setup_with_env(
    tools_to_install: &[String],
    tools_to_configure: &[String],
    font: Option<&str>,
    theme: Option<&str>,
    env: &SlateEnv,
) -> Result<ExecutionSummary> {
    execute_prepared_setup(prepare_setup_with_env(
        tools_to_install,
        tools_to_configure,
        font,
        theme,
        env,
    )?)
}

pub(crate) fn execute_prepared_setup(plan: plan::PreparedSetup) -> Result<ExecutionSummary> {
    execute_bound_plan(
        plan,
        crate::platform::packages::InstallContext::detect,
        tool_install::install_planned,
    )
}

fn execute_bound_plan(
    mut plan: plan::PreparedSetup,
    mut current_context: impl FnMut() -> crate::platform::packages::InstallContext,
    mut install: impl FnMut(
        &crate::cli::tool_selection::PlannedToolInstall,
        &SlateEnv,
    ) -> Result<tool_install::ToolInstallMethod>,
) -> Result<ExecutionSummary> {
    use crate::cli::tool_selection::InstallPlan;
    // CLI plans carry the receipt that was actually confirmed. Library callers
    // without a wizard capture one execution plan before any installer runs.
    let installs = match plan.reviewed_installs.take() {
        Some(reviewed) => reviewed,
        None => InstallPlan::capture(
            &plan
                .tools_to_install
                .iter()
                .map(|tool| tool.id.to_owned())
                .collect::<Vec<_>>(),
            &plan.env,
            current_context(),
        )?,
    };
    installs.verify_selection(&plan.tools_to_install, &plan.env)?;
    installs.verify_context(current_context())?;
    execute_checked(
        plan,
        |id, _, _, env| install(installs.tool(id)?, env),
        |id| installs.verify_tool(id, current_context()),
    )
}

#[cfg(test)]
fn execute_with_installer(
    plan: plan::PreparedSetup,
    install: impl FnMut(
        &str,
        &str,
        crate::cli::tool_selection::BrewKind,
        &SlateEnv,
    ) -> Result<tool_install::ToolInstallMethod>,
) -> Result<ExecutionSummary> {
    execute_checked(plan, install, |_| Ok(()))
}

fn execute_checked(
    plan: plan::PreparedSetup,
    mut install: impl FnMut(
        &str,
        &str,
        crate::cli::tool_selection::BrewKind,
        &SlateEnv,
    ) -> Result<tool_install::ToolInstallMethod>,
    mut verify: impl FnMut(&str) -> Result<()>,
) -> Result<ExecutionSummary> {
    plan.loader.verify(&plan.env)?;
    let env = &plan.env;
    let font = plan.font.as_deref();
    let tools_to_configure = &plan.tools_to_configure;
    let mut summary = ExecutionSummary::new();
    summary.font_requested = font.is_some();
    let terminal = detection::TerminalProfile::from_env_vars(
        std::env::var("TERM_PROGRAM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
    )
    .with_session(env.session().clone());

    // The prepared theme also colors progress; do not reread an ambient profile
    // after the caller's choice has been resolved.
    let ctx = RenderContext::new(&plan.theme);
    let roles = Some(Roles::new(&ctx));

    // the setup-applying header is a static tree-narrative anchor.
    // Emit via println! (stderr-adjacent `eprintln!` for diagnostics
    // parity with the existing flow) bypassing cliclack.
    eprintln!(
        "\n{}\n",
        heading(roles.as_ref(), tr("正在应用设置", "Applying your setup"))
    );

    let spinner = cliclack::spinner();

    for tool in &plan.tools_to_install {
        let tool_id = tool.id;
        if let Err(error) = verify(tool_id) {
            spinner.error(format!(
                "{} {}",
                tool.label,
                tr(
                    "安装计划已变化，请重新确认；设置已停止",
                    "installation plan needs review; setup stopped"
                )
            ));
            return Err(error);
        }
        spinner.start(format!(
            "{} {}...",
            tr("正在安装", "Installing"),
            tool.label
        ));

        // Report the actual result immediately; spinner visibility must not
        // impose a minimum duration on an already-completed installation.
        match install(tool_id, tool.brew_package, tool.brew_kind, env) {
            Ok(method) => {
                summary.add_tool_result(ToolInstallResult {
                    tool_id: tool_id.to_owned(),
                    tool_label: tool.label.to_string(),
                    status: InstallStatus::Success,
                    error_message: None,
                });
                spinner.stop(method.success_message(tool.label));
                // per-tool-apply success → BrandEvent::ApplyComplete.
                // SoundSink consumes this for per-tool SFX.
                dispatch(BrandEvent::ApplyComplete);
            }
            Err(err) if !installation_fallback_allowed(&err) => {
                // The handler wraps this failure with its existing checkpoint.
                // Do not launch another installer or follow-up configuration
                // while installer completion is unconfirmed.
                spinner.error(format!(
                    "{} {}",
                    tool.label,
                    tr(
                        "安装结果尚未确认；设置已停止",
                        "installation result is uncertain; setup stopped"
                    )
                ));
                return Err(err);
            }
            Err(err) => {
                summary.add_tool_result(ToolInstallResult {
                    tool_id: tool_id.to_owned(),
                    tool_label: tool.label.to_string(),
                    status: InstallStatus::Failed,
                    error_message: Some(err.to_string()),
                });
                spinner.error(status_error(
                    roles.as_ref(),
                    &format!(
                        "{} {}: {}",
                        tool.label,
                        tr("失败", "failed"),
                        terminal_text(&err.to_string())
                    ),
                ));
            }
        }
    }

    let font_cache_refresh = font_stage::execute(font, env, &mut summary);

    if let Some(font_name) = font.filter(|_| summary.font_available) {
        let family = resolve_font_family_with_env(env, font_name);
        save_font_choice(env, &family, &mut summary);

        summary.add_notice(crate::platform::fonts::activation_hint_in(
            font_cache_refresh,
            crate::cli::ui_language::output_language(),
        ));
    }

    let just_installed: Vec<String> = summary
        .tool_results
        .iter()
        .filter(|result| result.status == InstallStatus::Success)
        .map(|result| result.tool_id.clone())
        .collect();
    for issue in ensure_tool_configs(env, tools_to_configure, &just_installed) {
        summary.add_issue(issue);
    }

    spinner.start(tr(
        "正在配置 Shell 集成…",
        "Setting up shell integration...",
    ));
    match integration::setup_with_loader(&plan.theme, env, tools_to_configure, &plan.loader) {
        Ok(report) => {
            summary.theme_applied = true;
            for issue in theme_apply_issues(&report.results) {
                summary.add_issue(issue);
            }
            summary.set_theme_results(report.results);
            spinner.stop(status_success(
                roles.as_ref(),
                &format!(
                    "{} {}",
                    tr(
                        "Shell 集成已配置，主题：",
                        "Shell integration configured for"
                    ),
                    terminal_text(&plan.theme.name)
                ),
            ));
        }
        Err(err) => {
            spinner.error(status_error(
                roles.as_ref(),
                &format!(
                    "{}: {}",
                    tr("Shell 集成出现问题", "Shell integration had issues"),
                    terminal_text(&err.to_string())
                ),
            ));
            summary.add_issue(format!(
                "{}: {}",
                tr("Shell 集成设置失败", "Shell integration setup failed"),
                err
            ));
            // The handler owns the whole-setup success/failure milestone after
            // all follow-up work. The executor returns detailed partial results.
        }
    }

    use std::io::IsTerminal;
    // The interactive handler owns the final receipt and activation guidance.
    // Retain the legacy detailed card for redirected command output only.
    if show_configuration_card(
        summary.theme_applied,
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    ) {
        let theme_name = &plan.theme.id;
        let font_name = match font {
            Some(font) if summary.font_applied => font,
            Some(_) => "(selected font choice not saved)",
            None => "(existing font kept)",
        };
        let tool_count = summary.configured_count();
        let shell = plan.shell.label();
        let receipt_body = format!(
            "Terminal    {} ({})\n\
             Theme       {theme_name}\n\
             Font        {font_name}\n\
             Shell       {shell}\n\
             Tools       {tool_count} configured",
            terminal.display_name(),
            terminal.compatibility_label()
        );
        let _ = cliclack::note("Configuration files updated", receipt_body);

        if terminal.session().can_reload_terminal() {
            if let Some(tip) = terminal.setup_tip() {
                let _ = cliclack::log::remark(tip);
            }
        }
    }

    summary.refresh_outcome();

    Ok(summary)
}

fn show_configuration_card(applied: bool, input_tty: bool, output_tty: bool) -> bool {
    applied && !(input_tty && output_tty)
}

fn save_font_choice(env: &SlateEnv, family: &str, summary: &mut ExecutionSummary) {
    summary.font_applied = false;
    match ConfigManager::with_env(env).and_then(|manager| manager.set_current_font(family)) {
        Ok(()) => summary.font_applied = true,
        Err(error) => summary.add_issue(format!(
            "Selected font is available but its choice could not be saved: {error}"
        )),
    }
}

/// Render `◆ title` via Roles::heading, falling back to a plain `◆ …`
/// when the registry failed to boot. graceful degrade.
fn heading(r: Option<&Roles<'_>>, title: &str) -> String {
    match r {
        Some(r) => r.heading(title),
        None => format!("◆ {title}"),
    }
}

/// Render `✗ message` via Roles::status_error (theme red — never
/// lavender per D-01a), falling back to plain text without color when
/// no Roles is available.
fn status_error(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_error(message),
        None => format!("✗ {message}"),
    }
}

/// Render `✓ message` via Roles::status_success (theme green), with a
/// plain fallback.
fn status_success(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_success(message),
        None => format!("✓ {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::font_selection::FontCatalog;
    use crate::cli::setup_executor::tool_install::{
        should_try_local_starship_fallback, ToolInstallMethod,
    };
    use crate::env::SlateEnv;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn shell_loader_drift_stops_real_executor_before_installers_and_theme_writes() {
        use crate::platform::shell::ShellBackend;
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let prepared = plan::prepare_with_shell(
            &["starship".into()],
            &[],
            None,
            Some("nord"),
            &env,
            ShellBackend::Bash,
        )
        .unwrap();
        std::fs::write(env.bash_integration_path(), "# later user edit\n").unwrap();
        let error = integration::setup_with_loader(&prepared.theme, &env, &[], &prepared.loader)
            .unwrap_err();
        assert!(error.to_string().contains("changed after preparation"));
        let error = execute_with_installer(prepared, |_, _, _, _| {
            panic!("changed loader must stop before an installer")
        })
        .unwrap_err();
        assert!(error.to_string().contains("changed after preparation"));
        assert_eq!(
            std::fs::read(env.bash_integration_path()).unwrap(),
            b"# later user edit\n"
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bash_startup_selection_drift_stops_real_executor_before_installers() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().into());
        std::fs::write(env.shell_profile_path(), "# existing user profile\n").unwrap();
        let prepared = plan::prepare_with_shell(
            &["starship".into()],
            &[],
            None,
            Some("nord"),
            &env,
            crate::platform::shell::ShellBackend::Bash,
        )
        .unwrap();
        std::fs::write(env.bash_login_path(), "# later higher-priority entry\n").unwrap();
        let error = execute_with_installer(prepared, |_, _, _, _| {
            panic!("selection drift must stop before installation")
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("selected startup or environment path changed"));
        assert_eq!(
            std::fs::read(env.shell_profile_path()).unwrap(),
            b"# existing user profile\n"
        );
        assert_eq!(
            std::fs::read(env.bash_login_path()).unwrap(),
            b"# later higher-priority entry\n"
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 2);
    }

    #[test]
    fn setup_outcome_font_availability_is_not_persistence() {
        for blocked in [false, true] {
            let td = TempDir::new().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let config = ConfigManager::with_env(&env).unwrap();
            let path = env.managed_file("current-font");
            if blocked {
                std::fs::create_dir(&path).unwrap();
            }
            let mut summary = ExecutionSummary::new();
            summary.font_requested = true;
            summary.font_available = true;
            summary.theme_applied = true;
            save_font_choice(&env, "Private Fixture Mono", &mut summary);
            assert!(summary.font_available);
            assert_eq!(summary.font_applied, !blocked);
            assert_eq!(summary.is_successful(), !blocked);
            if blocked {
                assert!(path.is_dir());
                assert_eq!(summary.issues.len(), 1);
            } else {
                assert_eq!(
                    config.get_current_font().unwrap().as_deref(),
                    Some("Private Fixture Mono")
                );
            }
        }
    }

    #[test]
    fn theme_safety_failed_setup_keeps_existing_shell_loader_and_environment() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        config.set_current_theme("nord").unwrap();
        let shell_env = env.config_dir().join("managed/shell/env.zsh");
        std::fs::create_dir_all(shell_env.parent().unwrap()).unwrap();
        std::fs::write(&shell_env, "# original shell env\n").unwrap();
        std::fs::write(env.zshrc_path(), "# original shell loader\n").unwrap();
        let broken = env.xdg_config_home().join("alacritty/alacritty.toml");
        std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
        std::fs::write(broken, "[broken TOML\n").unwrap();
        let result =
            setup_shell_integration_with_env(Some("catppuccin-mocha"), &env, &["alacritty".into()]);
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(shell_env).unwrap(),
            "# original shell env\n"
        );
        assert_eq!(
            std::fs::read_to_string(env.zshrc_path()).unwrap(),
            "# original shell loader\n"
        );
        assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
    }

    #[test]
    fn test_execute_setup_empty() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let result = execute_setup_with_env(&[], &[], None, None, &env);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert!(summary.overall_success);
        assert_eq!(summary.success_count(), 0);
        assert_eq!(summary.configured_count(), 0);
        assert!(summary.theme_results.is_empty());
    }

    #[test]
    fn configuration_card_is_not_duplicated_in_interactive_setup() {
        for input in [false, true] {
            for output in [false, true] {
                assert!(!show_configuration_card(false, input, output));
                assert_eq!(
                    show_configuration_card(true, input, output),
                    !(input && output)
                );
            }
        }
    }

    #[test]
    #[ignore = "private PTY executor fixture with simulated installers only"]
    fn executor_terminal_fixture() {
        let env = SlateEnv::from_process().unwrap();
        assert!(env.session().is_isolated());
        assert_eq!(std::fs::read_dir(env.home()).unwrap().count(), 0);
        crate::cli::ui_language::load_saved_ui_language(&env).unwrap();
        let failed = match std::env::var("SLATE_RECEIPT_CASE").unwrap().as_str() {
            "success" => false,
            "failure" => true,
            _ => panic!("unknown fixture"),
        };
        let plan = prepare_setup_with_env(&["bat".into()], &[], None, Some("nord"), &env).unwrap();
        let mut calls = 0;
        let summary = execute_with_installer(plan, |id, _, _, actual| {
            assert_eq!(id, "bat");
            assert_eq!(actual.home(), env.home());
            calls += 1;
            if failed {
                Err(crate::error::SlateError::Internal(
                    "fixture download failed".into(),
                ))
            } else {
                Ok(ToolInstallMethod::Homebrew)
            }
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(summary.is_successful(), !failed);
        assert!(summary.theme_applied);
        assert_eq!(
            ConfigManager::from_env_paths(&env)
                .get_current_theme()
                .unwrap()
                .as_deref(),
            Some("nord")
        );
        assert!(!env.user_local_bin().join("bat").exists());
        let terminal = detection::TerminalProfile::from_env_vars(Some("ghostty"), None)
            .with_session(env.session().clone());
        eprintln!(
            "RECEIPT-BEGIN\n{}RECEIPT-END",
            summary.format_completion_message_for_terminal(&terminal)
        );
    }

    #[test]
    fn completed_installations_report_success_and_known_failure_in_order() {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let ids = ["bat".into(), "delta".into()];
        let plan = prepare_setup_with_env(&ids, &[], None, Some("nord"), &env).unwrap();
        let mut calls = Vec::new();
        let summary = execute_with_installer(plan, |id, _, _, selected_env| {
            assert!(selected_env.session().is_isolated());
            calls.push(id.to_owned());
            if id == "bat" {
                Ok(ToolInstallMethod::Homebrew)
            } else {
                Err(crate::error::SlateError::Internal(
                    "fixture completed failure".into(),
                ))
            }
        })
        .unwrap();
        assert_eq!(calls, ids);
        assert_eq!(summary.tool_results.len(), 2);
        assert_eq!(summary.tool_results[0].status, InstallStatus::Success);
        assert_eq!(summary.tool_results[1].status, InstallStatus::Failed);
        assert!(summary.tool_results[1]
            .error_message
            .as_deref()
            .unwrap()
            .contains("fixture completed failure"));
        assert_eq!(summary.success_count(), 1);
        assert_eq!(summary.failure_count(), 1);
        assert!(!summary.is_successful());
    }

    #[test]
    fn homebrew_tool_uncertain_setup_stops_remaining_installers_and_configuration() {
        uncertain_setup_stops_remaining_installers_and_configuration("homebrew");
    }

    #[test]
    fn install_review_executor_checks_before_first_and_each_later_installer() {
        use crate::{
            cli::tool_selection::InstallPlan,
            platform::packages::{InstallContext, PackageManagerBackend, ToolInstallRoute},
        };
        let brew = InstallContext {
            package_manager: PackageManagerBackend::Homebrew,
            supported_os: true,
        };
        let changed = InstallContext {
            package_manager: PackageManagerBackend::Unsupported,
            ..brew
        };
        for change_before_first in [true, false] {
            let temp = TempDir::new().unwrap();
            let env = SlateEnv::with_home(temp.path().to_owned());
            let ids = ["bat".into(), "delta".into()];
            let reviewed = InstallPlan::capture(&ids, &env, brew).unwrap();
            let plan =
                prepare_setup_with_env(&ids, &["alacritty".into()], None, Some("nord"), &env)
                    .unwrap()
                    .with_reviewed_installs(reviewed, brew)
                    .unwrap();
            let current = std::cell::Cell::new(if change_before_first { changed } else { brew });
            let calls = std::cell::Cell::new(0);
            let error = execute_bound_plan(
                plan,
                || current.get(),
                |tool, selected_env| {
                    assert!(
                        !change_before_first,
                        "no installer may start after review drift"
                    );
                    assert_eq!(tool.metadata.id, "bat", "delta must never start");
                    assert_eq!(tool.route, ToolInstallRoute::Homebrew);
                    assert_eq!(selected_env.home(), env.home());
                    calls.set(calls.get() + 1);
                    std::fs::write(env.home().join("private-partial-package"), "retained").unwrap();
                    current.set(changed);
                    // Known failed exits normally continue; route drift must stop
                    // before the next installer even on that path. No success SFX.
                    Err(crate::error::SlateError::Internal(
                        "private completed failure".into(),
                    ))
                },
            )
            .unwrap_err();
            assert!(error.to_string().contains("plan changed"));
            assert_eq!(calls.get(), usize::from(!change_before_first));
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), calls.get());
            assert!(!env.managed_file("current").exists());
            assert!(!env.zshrc_path().exists());
        }
    }

    #[test]
    fn starship_local_uncertain_setup_stops_remaining_installers_and_configuration() {
        uncertain_setup_stops_remaining_installers_and_configuration("starship");
    }

    #[test]
    fn apt_install_uncertain_setup_stops_remaining_installers_and_configuration() {
        uncertain_setup_stops_remaining_installers_and_configuration("apt");
    }

    fn uncertain_setup_stops_remaining_installers_and_configuration(source: &str) {
        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let tools = if source == "apt" {
            ["bat", "delta"]
        } else {
            ["starship", "bat"]
        };
        let plan = prepare_setup_with_env(
            &tools.map(str::to_owned),
            &["alacritty".into()],
            Some("NeverInstallFixture Nerd Font"),
            Some("nord"),
            &env,
        )
        .unwrap();
        assert_eq!(plan.tools_to_install.len(), 2);
        let calls = std::cell::Cell::new(0);
        let error = execute_with_installer(plan, |id, _, _, selected_env| {
            assert_eq!(
                id, tools[0],
                "no later installer may start after uncertainty"
            );
            assert_eq!(selected_env.home(), env.home());
            calls.set(calls.get() + 1);
            std::fs::write(env.home().join("partial-package-record"), "left in place").unwrap();
            Err(match source {
                "starship" => crate::error::SlateError::StarshipInstallUncertain(
                    "fixture interruption".into(),
                ),
                "homebrew" => crate::error::SlateError::HomebrewInstallUncertain(
                    "fixture interruption".into(),
                ),
                "apt" => {
                    crate::error::SlateError::AptInstallUncertain("fixture interruption".into())
                }
                _ => unreachable!(),
            })
        })
        .unwrap_err();
        assert!(!installation_fallback_allowed(&error));
        assert_eq!(calls.get(), 1);
        assert_eq!(
            std::fs::read(env.home().join("partial-package-record")).unwrap(),
            b"left in place"
        );
        assert_eq!(std::fs::read_dir(env.home()).unwrap().count(), 1);
        assert!(!env.managed_file("current-font").exists());
        assert!(!env.managed_file("current").exists());
    }

    #[test]
    fn test_theme_selection_marks_summary_as_applied() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let summary =
            execute_setup_with_env(&[], &[], None, Some("catppuccin-mocha"), &env).unwrap();
        assert!(summary.theme_applied);
        assert_eq!(summary.configured_count(), 0);
        assert!(summary.theme_results.is_empty());
    }

    #[test]
    fn test_local_starship_fallback_triggering() {
        let permission = crate::error::SlateError::Internal(
            "starship — permission denied. shared Homebrew.".to_string(),
        );
        let missing_homebrew = crate::error::SlateError::Internal(
            "Homebrew was not found. Install it first or add it to PATH.".to_string(),
        );
        let network = crate::error::SlateError::Internal(
            "starship — network unreachable. Check your connection.".to_string(),
        );

        assert!(should_try_local_starship_fallback(&permission));
        assert!(should_try_local_starship_fallback(&missing_homebrew));
        assert!(!should_try_local_starship_fallback(&network));
    }

    #[test]
    fn test_user_local_install_message_mentions_directory() {
        let method = ToolInstallMethod::UserLocal(PathBuf::from("/tmp/.local/bin"));
        assert_eq!(
            method.success_message("Starship"),
            "✓ Starship installed locally at /tmp/.local/bin"
        );
    }

    #[test]
    fn test_setup_upgrades_legacy_starship_seed() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let config_path = env.xdg_config_home().join("starship.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        std::fs::write(
            &config_path,
            r#"format = "$username$directory$git_branch$git_status$cmd_duration$line_break$character"

[username]
show_always = true
format = "[$user]($style) "
style_user = "bold green"

[directory]
format = "[$path]($style) "
style = "bold cyan"
truncation_length = 3

[git_branch]
format = "[$symbol$branch]($style) "
symbol = ""
style = "bold purple"

[git_status]
format = "([$all_status$ahead_behind]($style) )"
style = "bold red"

[cmd_duration]
format = "[$duration]($style) "
style = "bold yellow"

[character]
success_symbol = "[>](bold green)"
error_symbol = "[>](bold red)"
"#,
        )
        .unwrap();

        let issues =
            ensure_tool_configs(&env, &["starship".to_string()], &["starship".to_string()]);
        assert!(issues.is_empty());

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("\"$schema\" = 'https://starship.rs/config-schema.json'"));
        assert!(content.contains("[](red)$os$username"));
    }

    #[test]
    fn test_setup_initializes_delta_gitconfig_with_context_comment() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let issues = ensure_tool_configs(&env, &["delta".to_string()], &["delta".to_string()]);

        assert!(issues.is_empty());
        let content = std::fs::read_to_string(env.home().join(".gitconfig")).unwrap();
        assert!(content.contains("git configuration"));
        assert!(content.contains("managed imports"));
    }

    #[test]
    fn setup_reuses_alacritty_alternates_without_creating_a_shadow_config() {
        for relative in [".config/alacritty.toml", ".alacritty.toml"] {
            let td = TempDir::new().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let path = env.home().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "# PRIVATE_CONTENT user\n").unwrap();
            let issues = ensure_tool_configs(&env, &["alacritty".into()], &["alacritty".into()]);
            assert!(issues.is_empty(), "{issues:?}");
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "# PRIVATE_CONTENT user\n"
            );
            assert!(!env
                .xdg_config_home()
                .join("alacritty/alacritty.toml")
                .exists());
        }
    }

    #[test]
    fn setup_does_not_initialize_through_a_dangling_alacritty_link() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let high = env.xdg_config_home().join("alacritty/alacritty.toml");
        let missing_target = env.home().join("must-not-be-created");
        std::fs::create_dir_all(high.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&missing_target, &high).unwrap();
        std::fs::write(env.home().join(".alacritty.toml"), "# lower\n").unwrap();
        ensure_tool_configs(&env, &["alacritty".into()], &["alacritty".into()]);
        assert!(!missing_target.exists());
        assert!(std::fs::symlink_metadata(&high).unwrap().is_symlink());
        assert_eq!(
            std::fs::read_to_string(env.home().join(".alacritty.toml")).unwrap(),
            "# lower\n"
        );
    }

    #[test]
    fn test_font_release_urls_match_official_asset_names() {
        let jetbrains = FontCatalog::get_font("jetbrains-mono").unwrap();
        let hack = FontCatalog::get_font("hack").unwrap();
        let iosevka = FontCatalog::get_font("iosevka-term").unwrap();
        let fira = FontCatalog::get_font("fira-code").unwrap();

        assert_eq!(jetbrains.release_asset, "JetBrainsMono");
        assert_eq!(hack.release_asset, "Hack");
        assert_eq!(iosevka.release_asset, "IosevkaTerm");
        assert_eq!(fira.release_asset, "FiraCode");
    }

    /// snapshot — the `◆ Applying your setup` narrative anchor
    /// (sketch 003 canon) rendered through Basic-mode Roles.
    #[test]
    fn setup_executor_heading_anchor_basic_snapshot() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let out = heading(Some(&r), "Applying your setup");
        insta::assert_snapshot!("setup_executor_heading_basic", out);
    }

    /// graceful degrade — heading/status helpers emit plain text
    /// when Roles is absent. Zero ANSI bytes.
    #[test]
    fn setup_executor_helpers_fall_back_to_plain_when_roles_absent() {
        assert_eq!(
            heading(None, "Applying your setup"),
            "◆ Applying your setup"
        );
        assert_eq!(status_error(None, "boom"), "✗ boom");
        assert_eq!(status_success(None, "ok"), "✓ ok");
    }

    /// D-01a invariant — status_error, across all modes, must never
    /// leak brand-anchor lavender bytes (error severity stays warning-
    /// colored).
    #[test]
    fn setup_executor_status_error_never_emits_lavender() {
        use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let out = status_error(Some(&r), "something failed");
            assert!(
                !out.contains("38;2;114;135;253"),
                "D-01a violation in mode {mode:?}: {out:?}"
            );
        }
    }
}
