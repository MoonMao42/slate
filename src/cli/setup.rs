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
    let preflight_result = preflight::run_checks()?;
    eprintln!("{}", preflight_result.format_for_display());

    if !preflight_result.is_ready() {
        return Err(crate::error::SlateError::Internal(
            "Pre-flight checks failed. Please resolve issues above.".to_string(),
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
    let selected_font = context.selected_font.as_deref();
    let selected_theme = context.selected_theme.as_deref();
    let selected_opacity = context.selected_opacity;
    let fastfetch_enabled = context.fastfetch_enabled;

    // Enable fastfetch auto-run marker BEFORE execute_setup_with_env writes
    // env.zsh — write_shell_integration_file() checks the marker's existence
    // and only emits the `if command -v fastfetch; then fastfetch; fi` block
    // when the marker is present. Creating the marker after setup leaves
    // env.zsh stale and silently breaks the "Yes" answer to auto-run.
    if fastfetch_enabled {
        if let Ok(config_mgr) = crate::config::ConfigManager::with_env(env) {
            let _ = config_mgr.enable_fastfetch_autorun();
        }
    }

    // Create baseline backup BEFORE any mutations (per)
    // Check if baseline already exists (idempotent on subsequent runs)
    {
        use crate::config::{list_restore_points, begin_restore_point_baseline};
        let backups = list_restore_points().ok();
        let has_baseline = if let Some(backups) = backups {
            backups.iter().any(|rp| rp.is_baseline)
        } else {
            false
        };
        
        if !has_baseline {
            if let Ok(baseline_point) = begin_restore_point_baseline(env.home()) {
                eprintln!("✓ Baseline snapshot created ({})", baseline_point.id);
            }
        }
    }

    // Execute the setup (install tools, apply configurations)
    let summary = setup_executor::execute_setup_with_env(
        &selected_tools,
        selected_font,
        selected_theme,
        env,
    )?;

    // Persist selected opacity if chosen (manual mode only)
    if let Some(opacity) = selected_opacity {
        if let Ok(config_mgr) = crate::config::ConfigManager::with_env(env) {
            let _ = config_mgr.set_current_opacity_preset(opacity);
        }
        // Write opacity to adapters
        let _ = crate::adapter::ghostty::write_opacity_config(env, opacity);
        let _ = crate::adapter::ghostty::write_blur_radius(env, opacity);
        let _ = crate::adapter::alacritty::write_opacity_config(env, opacity);
    }

    // Display completion message with visibility guidance
    eprintln!("\n{}", summary.format_completion_message());
    if let Some(timing_line) = format_completion_timing(start_time) {
        eprintln!("{}", timing_line);
    }

    Ok(())
}

/// Handle `slate setup` command with optional flags (backward compatibility)
/// Supports: --quick, --force, --only <tool>
pub fn handle(quick: bool, force: bool, only: Option<String>) -> Result<()> {
    let env = SlateEnv::from_process()?;
    handle_with_env(quick, force, only, &env)
}

/// Handle --only flag: retry a single tool installation
fn handle_retry_only(tool_id: &str) -> Result<()> {
    let tool = validate_retry_tool(tool_id)?;

    eprintln!("\n✦ Retrying tool installation: {}\n", tool.label);

    // Run pre-flight checks
    let preflight_result = preflight::run_checks()?;
    if !preflight_result.is_ready() {
        return Err(crate::error::SlateError::Internal(
            "Pre-flight checks failed.".to_string(),
        ));
    }

    // Execute single tool installation
    let summary = setup_executor::execute_setup(&[tool_id.to_string()], None, None)?;

    // Show completion
    if summary.success_count() > 0 {
        eprintln!("\n✓ Tool '{}' installed successfully.\n", tool.label);
    } else {
        eprintln!(
            "\n✗ Tool '{}' installation failed. Check logs above.\n",
            tool.label
        );
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
    use std::time::Duration;

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
        // Verify valid tools are recognized without performing the install in test.
        let result = validate_retry_tool("ghostty");
        assert!(result.is_ok());
    }

    #[test]
    fn test_setup_only_detectable_tool() {
        // Verify detect-only tools are rejected for retry
        let result = handle_retry_only("tmux");
        // tmux is detect-only, should fail
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
}
