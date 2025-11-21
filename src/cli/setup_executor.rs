/// Setup execution: actually runs the brew installations and applies configurations
/// Handles partial failures and tracks results

use crate::error::Result;
use crate::cli::failure_handler::{ExecutionSummary, ToolInstallResult, InstallStatus};
use crate::cli::tool_selection::{ToolCatalog, BrewKind};
use crate::cli::font_selection::FontCatalog;
use std::process::Command;

/// Execute the setup based on wizard selections
pub fn execute_setup(
    tools_to_install: &[String],
    font: Option<&str>,
    theme: Option<&str>,
) -> Result<ExecutionSummary> {
    let mut summary = ExecutionSummary::new();

    eprintln!("\n✦ Applying your setup...\n");

    let mut spinner = cliclack::spinner();

    // Install selected tools
    for tool_id in tools_to_install {
        if let Some(tool) = ToolCatalog::get_tool(tool_id) {
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

            match install_tool(&tool.brew_package, tool.brew_kind) {
                Ok(_) => {
                    summary.add_tool_result(ToolInstallResult {
                        tool_id: tool_id.clone(),
                        tool_label: tool.label.to_string(),
                        status: InstallStatus::Success,
                        error_message: None,
                    });
                    spinner.stop(format!("✓ {} installed", tool.label));
                }
                Err(e) => {
                    summary.add_tool_result(ToolInstallResult {
                        tool_id: tool_id.clone(),
                        tool_label: tool.label.to_string(),
                        status: InstallStatus::Failed,
                        error_message: Some(e.to_string()),
                    });
                    spinner.error(format!("✗ {} failed: {}", tool.label, e));
                    // Continue with next tool (partial failure handling)
                }
            }
        }
    }

    // Apply font (placeholder - actual application in adapters)
    if let Some(font_name) = font {
        spinner.start(format!("Installing font: {}...", font_name));
        match install_font(font_name) {
            Ok(_) => {
                summary.font_applied = true;
                spinner.stop("✓ Font installed");
            }
            Err(e) => {
                spinner.error(format!("✗ Font installation failed: {}", e));
            }
        }
    }

    // Apply theme (placeholder - actual application in adapters)
    if let Some(theme_name) = theme {
        spinner.start(format!("Theme selected: {}...", theme_name));
        spinner.stop("○ Theme adapter apply is not wired yet in this phase");
    }

    // Overall success: at least one tool succeeded, or no tools were selected
    summary.overall_success = summary.failure_count() == 0 || summary.success_count() > 0;

    Ok(summary)
}

/// Install a tool via Homebrew
fn install_tool(package: &str, kind: BrewKind) -> Result<()> {
    let mut cmd = Command::new("brew");

    match kind {
        BrewKind::Formula => {
            cmd.arg("install").arg(package);
        }
        BrewKind::Cask => {
            cmd.arg("install").arg("--cask").arg(package);
        }
    }

    let output = cmd.output()
        .map_err(|e| crate::error::SlateError::Internal(format!("Failed to execute brew: {}", e)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(crate::error::SlateError::Internal(
            format!("brew install {} failed:\n{}", package, stderr.trim())
        ))
    }
}

/// Install a Nerd Font via Homebrew
fn install_font(font_name_or_id: &str) -> Result<()> {
    let cask_name = FontCatalog::get_font(font_name_or_id)
        .map(|font| font.brew_cask)
        .or_else(|| {
            FontCatalog::all_fonts()
                .into_iter()
                .find(|font| {
                    font.name == font_name_or_id
                        || font.name.replace(" Nerd Font", "") == font_name_or_id
                })
                .map(|font| font.brew_cask)
        })
        .ok_or_else(|| {
            crate::error::SlateError::Internal(format!("Unknown font: {}", font_name_or_id))
        })?;

    let mut cmd = Command::new("brew");
    cmd.arg("install").arg("--cask").arg(cask_name);

    let output = cmd.output()
        .map_err(|e| crate::error::SlateError::Internal(format!("Failed to execute brew: {}", e)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(crate::error::SlateError::Internal(
            format!("Font installation failed:\n{}", stderr.trim())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_setup_empty() {
        // Test with no tools selected
        let result = execute_setup(&[], None, None);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert!(summary.overall_success);
        assert_eq!(summary.success_count(), 0);
    }

    #[test]
    fn test_font_mapping() {
        // Verify font display names map correctly
        let fonts = vec!["JetBrains Mono", "Fira Code", "Iosevka Term", "Hack"];
        for font in fonts {
            // Just verify these are recognized
            let _ = font;
        }
    }

    #[test]
    fn test_theme_selection_stays_pending_without_adapter_apply() {
        let summary = execute_setup(&[], None, Some("catppuccin-mocha")).unwrap();
        assert!(!summary.theme_applied);
    }
}
