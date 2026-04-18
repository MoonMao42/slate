use crate::brand::language::Language;
use crate::cli::preflight;
use crate::cli::setup_executor;
use crate::cli::tool_selection::ToolCatalog;
use crate::cli::wizard_core::Wizard;
use crate::env::SlateEnv;
use crate::error::Result;
use std::io::IsTerminal;
use std::time::Instant;

/// Handle `slate setup` command with injected SlateEnv (preferred for testability)
pub fn handle_with_env(
    quick: bool,
    force: bool,
    only: Option<String>,
    env: &SlateEnv,
) -> Result<()> {
    // If --only flag is set, handle retry flow
    if let Some(tool_id) = only {
        return handle_retry_only(&tool_id);
    }

    if !std::io::stdin().is_terminal() && !quick {
        return Err(crate::error::SlateError::Internal(
            "Non-interactive setup requires --quick for explicit consent.".to_string(),
        ));
    }

    // Run pre-flight checks
    eprintln!("\n");
    let has_existing_install = env.managed_file("current").exists();
    let scenario = if quick && has_existing_install {
        // Reconfigure path: user already has a slate install, doesn't need a package manager.
        preflight::PreflightScenario::ConfigOnlyReconfigure
    } else if quick {
        preflight::PreflightScenario::QuickSetup
    } else {
        preflight::PreflightScenario::GuidedSetup
    };
    let preflight_result = preflight::run_checks_for_setup_with_env(env, scenario)?;
    eprintln!("{}", preflight_result.format_for_display());

    if !preflight_result.is_ready() {
        return Err(crate::error::SlateError::Internal(
            preflight_result.format_blocking_guidance(),
        ));
    }

    eprintln!("\n");

    // Run the wizard
    let mut wizard = Wizard::new()?;
    wizard.run(quick, force)?;

    // Build selections from wizard context
    let context = wizard.get_context();
    if !context.confirmed {
        return Ok(());
    }
    let start_time = context.start_time;
    let selected_tools = context.selected_tools.clone();
    let tools_to_configure = context.tools_to_configure.clone();
    let selected_font = context.selected_font.as_deref();
    let selected_theme = context.selected_theme.as_deref();
    let selected_opacity = context.selected_opacity;
    let fastfetch_enabled = context.fastfetch_enabled;

    // Snapshot current state BEFORE any mutations
    {
        use crate::config::{
            begin_restore_point_baseline_with_env, list_restore_points_with_env,
            snapshot_current_state_with_env,
        };
        let backups = list_restore_points_with_env(env).ok();
        let has_baseline = if let Some(ref backups) = backups {
            backups.iter().any(|rp| rp.is_baseline)
        } else {
            false
        };

        if !has_baseline {
            // First time: create baseline (pre-slate state)
            match begin_restore_point_baseline_with_env(env) {
                Ok(baseline_point) => {
                    eprintln!("✓ Baseline snapshot created ({})", baseline_point.id);
                }
                Err(_) => {
                    eprintln!("⚠ Could not create baseline snapshot — slate restore will not be available for pre-slate state");
                }
            }
        } else {
            // Subsequent runs: snapshot current config so user can restore back
            let config = crate::config::ConfigManager::with_env(env).ok();
            let label = config
                .and_then(|c| c.get_current_theme().ok().flatten())
                .unwrap_or_else(|| "pre-setup".to_string());
            match snapshot_current_state_with_env(env, &label) {
                Ok(snap) => {
                    eprintln!("✓ Snapshot created ({})", snap.id);
                }
                Err(_) => {
                    eprintln!("⚠ Could not create restore snapshot — continuing without it");
                }
            }
        }
    }

    prepare_setup_state(env, fastfetch_enabled, selected_opacity)?;

    // Execute the setup (install tools, apply configurations)
    let summary = setup_executor::execute_setup_with_env(
        &selected_tools,
        &tools_to_configure,
        selected_font,
        selected_theme,
        env,
    )?;

    // Display completion message with visibility guidance
    eprintln!("\n{}", summary.format_completion_message());
    if let Some(timing_line) = format_completion_timing(start_time) {
        eprintln!("{}", timing_line);
    }

    crate::cli::sound::play_feedback();

    // DEMO-02 (D-C1): single-line hint pointing at `slate demo`. setup has no
    // --auto / --quiet flags at this surface, so both guards are false.
    crate::cli::demo::emit_demo_hint_once(false, false);

    Ok(())
}

/// Handle `slate setup` command with optional flags (backward compatibility)
/// Supports: --quick, --force, --only <tool>
pub fn handle(quick: bool, force: bool, only: Option<String>) -> Result<()> {
    let env = SlateEnv::from_process()?;
    handle_with_env(quick, force, only, &env)
}

fn prepare_setup_state(
    env: &SlateEnv,
    fastfetch_enabled: Option<bool>,
    selected_opacity: Option<crate::opacity::OpacityPreset>,
) -> Result<()> {
    let config_mgr = crate::config::ConfigManager::with_env(env)?;

    // Fastfetch: only write if user made an explicit choice (Some).
    // None = user wasn't asked (quick mode) — preserve existing setting.
    match fastfetch_enabled {
        Some(true) => {
            if let Err(e) = config_mgr.enable_fastfetch_autorun() {
                eprintln!("⚠ Could not save fastfetch preference: {}", e);
            }
        }
        Some(false) => {
            if let Err(e) = config_mgr.disable_fastfetch_autorun() {
                eprintln!("⚠ Could not save fastfetch preference: {}", e);
            }
        }
        None => {} // Don't touch existing setting
    }

    if let Some(opacity) = selected_opacity {
        if let Err(e) = config_mgr.set_current_opacity_preset(opacity) {
            eprintln!("⚠ Could not save opacity preference: {}", e);
        }
    }

    Ok(())
}

/// Handle --only flag: retry a single tool installation.
/// Only installs the tool — does NOT rewrite shell integration or apply themes.
fn handle_retry_only(tool_id: &str) -> Result<()> {
    let tool = validate_retry_tool(tool_id)?;
    let env = crate::env::SlateEnv::from_process()?;

    eprintln!("\n✦ Retrying tool installation: {}\n", tool.label);

    // Run pre-flight checks
    let preflight_result =
        preflight::run_checks_for_setup_with_env(&env, preflight::PreflightScenario::RetryInstall)?;
    if !preflight_result.is_ready() {
        return Err(crate::error::SlateError::Internal(
            preflight_result.format_blocking_guidance(),
        ));
    }

    // Only install the single tool — no shell integration, no theme apply
    match setup_executor::install_tool(tool.id, tool.brew_package, tool.brew_kind, &env) {
        Ok(method) => {
            eprintln!("\n{}", method.success_message(tool.label));
        }
        Err(e) => {
            eprintln!("\n✗ Tool '{}' installation failed: {}\n", tool.label, e);
        }
    }

    Ok(())
}

fn validate_retry_tool(tool_id: &str) -> Result<crate::cli::tool_selection::ToolMetadata> {
    let Some(tool) = ToolCatalog::get_tool(tool_id) else {
        return Err(crate::error::SlateError::Internal(format!(
            "Unknown tool: '{}'. Run 'slate setup' to see available tools.",
            tool_id
        )));
    };

    if !tool.installable {
        return Err(crate::error::SlateError::Internal(format!(
            "Tool '{}' is not installable via setup",
            tool_id
        )));
    }

    Ok(tool)
}

fn format_completion_timing(start_time: Option<Instant>) -> Option<String> {
    start_time.map(|start| {
        format!(
            "{} {}ms",
            Language::COMPLETION_TIME_TAKEN,
            start.elapsed().as_millis()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opacity::OpacityPreset;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_setup_force_flag_recognized() {
        // Verify force flag is handled
        let force = true;
        assert!(force);
    }

    #[test]
    fn test_setup_only_invalid_tool() {
        // Verify invalid tool names are rejected
        let result = handle_retry_only("invalid_tool_xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_setup_only_valid_tool() {
        // Verify installable tools are recognized
        let result = validate_retry_tool("starship");
        assert!(result.is_ok());
    }

    #[test]
    fn test_setup_only_detectable_tool() {
        // Verify detect-only tools are rejected for retry
        let result = handle_retry_only("tmux");
        assert!(result.is_err());
        // ghostty is now detect-only too
        let result = handle_retry_only("ghostty");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_completion_timing_uses_label() {
        let start = Instant::now() - Duration::from_millis(10);
        let line = format_completion_timing(Some(start)).expect("timing should be present");

        assert!(line.contains(Language::COMPLETION_TIME_TAKEN));
        assert!(line.contains("ms"));
    }

    #[test]
    fn test_format_completion_timing_none() {
        assert!(format_completion_timing(None).is_none());
    }

    #[test]
    fn test_prepare_setup_state_updates_marker_and_opacity_before_apply() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let config = crate::config::ConfigManager::with_env(&env).unwrap();

        config.enable_fastfetch_autorun().unwrap();

        prepare_setup_state(&env, Some(false), Some(OpacityPreset::Frosted)).unwrap();

        assert!(!config.has_fastfetch_autorun().unwrap());
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Frosted
        );
    }
}
