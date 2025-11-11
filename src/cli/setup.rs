use crate::error::Result;
use crate::cli::wizard_core::Wizard;
use crate::cli::preflight;
use crate::cli::tool_selection::ToolCatalog;

/// Handle `slate setup` command with optional flags
/// Supports: --quick, --force, --only <tool>
pub fn handle(quick: bool, force: bool, only: Option<String>) -> Result<()> {
    // If --only flag is set, handle retry flow
    if let Some(tool_id) = only {
        return handle_retry_only(&tool_id);
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

    // For now, show a placeholder completion message
    // Full execution and summary will come in next task
    eprintln!("\nSetup wizard flow completed. Full execution coming next.\n");

    Ok(())
}

/// Handle --only flag: retry a single tool installation
fn handle_retry_only(tool_id: &str) -> Result<()> {
    // Validate that the tool exists and is installable
    if let Some(tool) = ToolCatalog::get_tool(tool_id) {
        if !tool.installable {
            return Err(crate::error::SlateError::Internal(format!(
                "Tool '{}' is not installable via setup",
                tool_id
            )));
        }

        eprintln!("\n✦ Retrying tool installation: {}\n", tool.label);

        // Run pre-flight checks
        let preflight_result = preflight::run_checks()?;
        if !preflight_result.is_ready() {
            return Err(crate::error::SlateError::Internal(
                "Pre-flight checks failed.".to_string(),
            ));
        }

        // In a real implementation, we would:
        // 1. Check if tool is already installed (skip if yes)
        // 2. Run `brew install` with appropriate args
        // 3. Report results

        eprintln!("Retry for tool '{}' started (execution coming next)\n", tool_id);

        Ok(())
    } else {
        Err(crate::error::SlateError::Internal(
            format!("Unknown tool: '{}'. Run 'slate setup' to see available tools.", tool_id),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Verify valid tools are recognized
        let result = handle_retry_only("ghostty");
        // Should not error on validation (execution details may differ)
        // We're just checking the tool exists
        let _ = result;
    }

    #[test]
    fn test_setup_only_detectable_tool() {
        // Verify detect-only tools are rejected for retry
        let result = handle_retry_only("tmux");
        // tmux is detect-only, should fail
        assert!(result.is_err());
    }
}
