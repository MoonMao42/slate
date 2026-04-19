//! Setup execution: actually runs the brew installations and applies configurations.
//! Handles partial failures and tracks results.

mod font_install;
mod integration;
mod tool_install;

use crate::brand::events::{dispatch, BrandEvent, FailureKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::failure_handler::{ExecutionSummary, InstallStatus, ToolInstallResult};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;

pub(crate) use font_install::{
    copy_font_from_caskroom, download_font_release, font_display_name, install_font,
    is_font_installed_with_env, planned_font_installs, resolve_font_family_with_env,
    strip_error_prefix,
};
pub(crate) use integration::{
    ensure_tool_configs, setup_shell_integration_with_env, theme_apply_issues,
};
pub(crate) use tool_install::install_tool;

/// Execute the setup based on wizard selections with injected SlateEnv (preferred)
pub fn execute_setup_with_env(
    tools_to_install: &[String],
    tools_to_configure: &[String],
    font: Option<&str>,
    theme: Option<&str>,
    env: &SlateEnv,
) -> Result<ExecutionSummary> {
    let mut summary = ExecutionSummary::new();

    // Build a RenderContext up-front so the tree-narrative anchor + the
    // per-tool status lines share the same byte contract (sketch 003
    // winner + D-01 daily chrome). Registry init failure is graceful —
    // the executor still prints plain-text status, per D-05.
    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    // D-10: the setup-applying header is a static tree-narrative anchor.
    // Emit via println! (stderr-adjacent `eprintln!` for diagnostics
    // parity with the existing flow) bypassing cliclack.
    eprintln!("\n{}\n", heading(roles.as_ref(), "Applying your setup"));

    let spinner = cliclack::spinner();

    for tool_id in tools_to_install {
        if let Some(tool) = crate::cli::tool_selection::ToolCatalog::get_tool(tool_id) {
            if !tool.installable {
                summary.add_tool_result(ToolInstallResult {
                    tool_id: tool_id.clone(),
                    tool_label: tool.label.to_string(),
                    status: InstallStatus::Skipped,
                    error_message: Some("Not installable via setup".to_string()),
                });
                continue;
            }

            spinner.start(format!("Installing {}...", tool.label));

            let install_start = std::time::Instant::now();
            match install_tool(tool_id, tool.brew_package, tool.brew_kind, env) {
                Ok(method) => {
                    let elapsed = install_start.elapsed();
                    if elapsed < std::time::Duration::from_millis(400) {
                        std::thread::sleep(std::time::Duration::from_millis(400) - elapsed);
                    }
                    summary.add_tool_result(ToolInstallResult {
                        tool_id: tool_id.clone(),
                        tool_label: tool.label.to_string(),
                        status: InstallStatus::Success,
                        error_message: None,
                    });
                    spinner.stop(method.success_message(tool.label));
                    // D-17: per-tool-apply success → BrandEvent::ApplyComplete.
                    // Phase 20's SoundSink consumes this for per-tool SFX.
                    dispatch(BrandEvent::ApplyComplete);
                }
                Err(err) => {
                    let elapsed = install_start.elapsed();
                    if elapsed < std::time::Duration::from_millis(400) {
                        std::thread::sleep(std::time::Duration::from_millis(400) - elapsed);
                    }
                    summary.add_tool_result(ToolInstallResult {
                        tool_id: tool_id.clone(),
                        tool_label: tool.label.to_string(),
                        status: InstallStatus::Failed,
                        error_message: Some(err.to_string()),
                    });
                    spinner.error(status_error(
                        roles.as_ref(),
                        &format!("{} failed: {}", tool.label, err),
                    ));
                }
            }
        }
    }

    let font_plan = planned_font_installs(font);
    let mut brew_font_broken = false;
    let homebrew_font_path = matches!(
        crate::platform::packages::detect_backend(),
        crate::platform::packages::PackageManagerBackend::Homebrew
    );
    for font_name in &font_plan {
        let required = font == Some(font_name.as_str());
        let display = font_display_name(font_name);

        spinner.start(format!("Checking font {}...", display));
        if is_font_installed_with_env(env, font_name) {
            spinner.stop(format!("✓ {} already installed", display));
            if required {
                summary.font_applied = true;
            }
            continue;
        }

        if homebrew_font_path && !brew_font_broken {
            spinner.start(format!("Installing {} via Homebrew...", display));
            match install_font(font_name) {
                Ok(_) => {
                    spinner.stop(format!("✓ {} installed", display));
                    if required {
                        summary.font_applied = true;
                    }
                    continue;
                }
                Err(err) => {
                    let message = err.to_string().to_lowercase();
                    if message.contains("permission denied") || message.contains("not writable") {
                        brew_font_broken = true;
                        spinner.stop("⚠ Homebrew: no write access — switching to direct download");
                    } else {
                        // Non-permission brew failures (network, renamed cask, deleted cask, etc.)
                        // still fall through to direct download, but surface what went wrong so
                        // users aren't left guessing when the final path reports "download failed".
                        let err_full = err.to_string();
                        let err_line = strip_error_prefix(&err_full);
                        spinner.stop(format!(
                            "⚠ Homebrew install failed — trying fallback ({})",
                            err_line
                        ));
                        summary.add_notice(format!("brew {}: {}", display, err_line));
                    }
                }
            }
        }

        if homebrew_font_path && copy_font_from_caskroom(font_name, env).is_ok() {
            spinner.stop(format!("✓ {} installed (shared cache)", display));
            if required {
                summary.font_applied = true;
            }
            continue;
        }

        spinner.start(format!("Downloading {}...", display));
        match download_font_release(font_name, env) {
            Ok(_) => {
                spinner.stop(format!("✓ {} downloaded", display));
                if required {
                    summary.font_applied = true;
                }
            }
            Err(err) => {
                let full = err.to_string();
                let err_msg = strip_error_prefix(&full);
                if required {
                    spinner.error(format!("✗ {}: {}", display, err_msg));
                    summary.add_issue(format!("{}: {}", display, err_msg));
                } else {
                    spinner.stop(format!("⚠ {} unavailable", display));
                    summary.add_notice(format!("{}: {}", display, err_msg));
                }
            }
        }
    }

    if let Some(font_name) = font.filter(|_| summary.font_applied) {
        let family = resolve_font_family_with_env(env, font_name);
        match ConfigManager::with_env(env).and_then(|manager| manager.set_current_font(&family)) {
            Ok(_) => {}
            Err(err) => {
                summary.add_issue(format!(
                    "Font '{}' was installed but could not be saved to config: {}",
                    family, err
                ));
            }
        }

        summary.add_notice(crate::platform::fonts::activation_hint());
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

    spinner.start("Setting up shell integration...");
    match setup_shell_integration_with_env(theme, env, tools_to_configure) {
        Ok((selected_theme, report)) => {
            summary.theme_applied = true;
            for issue in theme_apply_issues(&report.results) {
                summary.add_issue(issue);
            }
            summary.set_theme_results(report.results);
            spinner.stop(status_success(
                roles.as_ref(),
                &format!("Shell integration configured for {}", selected_theme.name),
            ));
        }
        Err(err) => {
            spinner.error(status_error(
                roles.as_ref(),
                &format!("Shell integration had issues: {}", err),
            ));
            summary.add_issue(format!("Shell integration setup failed: {}", err));
            // D-17 + 18-CONTEXT.md: setup-level failure → single Failure
            // dispatch so Phase 20's SoundSink maps it to the failure SFX.
            dispatch(BrandEvent::Failure(FailureKind::SetupFailed));
        }
    }

    if summary.theme_applied {
        std::thread::sleep(std::time::Duration::from_millis(500));

        let theme_name = theme.unwrap_or("catppuccin-mocha");
        let font_name = font.unwrap_or("(system default)");
        let tool_count = summary.configured_count();
        let shell = match crate::platform::shell::detect_backend() {
            crate::platform::shell::ShellBackend::Zsh => "zsh (.zshrc)".to_string(),
            crate::platform::shell::ShellBackend::Bash => "bash (.bashrc)".to_string(),
            crate::platform::shell::ShellBackend::Fish => {
                "fish (~/.config/fish/conf.d/slate.fish)".to_string()
            }
            crate::platform::shell::ShellBackend::Unsupported => std::env::var("SHELL")
                .ok()
                .and_then(|shell_path| shell_path.rsplit('/').next().map(String::from))
                .unwrap_or_else(|| "unsupported".to_string()),
        };
        let terminal = detection::TerminalProfile::detect();

        let receipt_body = format!(
            "Terminal    {} ({})\n\
             Theme       {theme_name}\n\
             Font        {font_name}\n\
             Shell       {shell}\n\
             Tools       {tool_count} configured",
            terminal.display_name(),
            terminal.compatibility_label()
        );
        let _ = cliclack::note("Your terminal is beautiful", receipt_body);

        if let Some(tip) = terminal.setup_tip() {
            let _ = cliclack::log::remark(tip);
        }
    }

    let font_ok = font.is_none() || summary.font_applied;
    summary.overall_success = summary.failure_count() == 0
        && font_ok
        && summary.theme_applied
        && summary.theme_failure_count() == 0
        && summary.missing_integration_skip_count() == 0
        && summary.issues.is_empty();

    Ok(summary)
}

/// Render `◆ title` via Roles::heading, falling back to a plain `◆ …`
/// when the registry failed to boot. D-05 graceful degrade.
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
    fn test_planned_font_installs_only_selected() {
        let plan = planned_font_installs(Some("jetbrains-mono"));
        assert_eq!(plan, vec!["jetbrains-mono"]);
    }

    #[test]
    fn test_planned_font_installs_none_selected() {
        let plan = planned_font_installs(None);
        assert!(plan.is_empty());
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

    /// Wave 1 snapshot — the `◆ Applying your setup` narrative anchor
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

    /// D-05 graceful degrade — heading/status helpers emit plain text
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
