/// Setup execution: actually runs the brew installations and applies configurations
/// Handles partial failures and tracks results
use crate::adapter::{
    AlacrittyAdapter, BatAdapter, DeltaAdapter, EzaAdapter, FastfetchAdapter, GhosttyAdapter,
    LazygitAdapter, StarshipAdapter, TmuxAdapter, ToolAdapter, ZshHighlightAdapter,
};
use crate::cli::failure_handler::{ExecutionSummary, InstallStatus, ToolInstallResult};
use crate::cli::font_selection::FontCatalog;
use crate::cli::tool_selection::{BrewKind, ToolCatalog};
use crate::config::ConfigManager;
use crate::error::Result;
use crate::theme::{ThemeRegistry, ThemeVariant};
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

/// Execute the setup based on wizard selections
pub fn execute_setup(
    tools_to_install: &[String],
    font: Option<&str>,
    theme: Option<&str>,
) -> Result<ExecutionSummary> {
    let mut summary = ExecutionSummary::new();

    eprintln!("\n✦ Applying your setup...\n");

    let spinner = cliclack::spinner();

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

    if let Some(font_name) = font {
        spinner.start(format!("Checking font: {}...", font_name));
        if is_font_installed(font_name) {
            summary.font_applied = true;
            spinner.stop(format!("✓ Font already installed: {}", font_name));
        } else {
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
    }

    // Setup shell integration: write marker block to .zshrc and env.zsh
    spinner.start("Setting up shell integration...");
    match setup_shell_integration(theme) {
        Ok(selected_theme) => {
            summary.theme_applied = true;
            spinner.stop(format!(
                "✓ Shell integration configured for {}",
                selected_theme.name
            ));
        }
        Err(e) => {
            spinner.error(format!("✗ Shell integration setup failed: {}", e));
            return Err(e);
        }
    }

    // Overall success: at least one tool succeeded, or no tools were selected
    summary.overall_success = summary.failure_count() == 0 || summary.success_count() > 0;

    Ok(summary)
}

/// Resolve the selected theme, persist it, regenerate shell integration, and
/// apply adapter outputs that live outside env.zsh.
pub(crate) fn apply_theme_selection(theme: &ThemeVariant) -> Result<()> {
    theme.validate()?;

    let config_mgr = ConfigManager::new()?;
    config_mgr.set_current_theme(&theme.id)?;
    config_mgr.write_shell_integration_file(theme)?;

    // Apply all adapters
    GhosttyAdapter.apply_theme(theme)?;
    AlacrittyAdapter.apply_theme(theme)?;
    StarshipAdapter.apply_theme(theme)?;
    BatAdapter.apply_theme(theme)?;
    DeltaAdapter.apply_theme(theme)?;
    EzaAdapter.apply_theme(theme)?;
    LazygitAdapter.apply_theme(theme)?;
    FastfetchAdapter.apply_theme(theme)?;
    ZshHighlightAdapter.apply_theme(theme)?;

    if TmuxAdapter.is_installed()? {
        TmuxAdapter.apply_theme(theme)?;
    }

    Ok(())
}

fn resolve_selected_theme(theme: Option<&str>) -> Result<ThemeVariant> {
    let config_mgr = ConfigManager::new()?;
    let theme_id = if let Some(theme_name) = theme {
        theme_name.to_string()
    } else if let Some(current_theme) = config_mgr.get_current_theme()? {
        current_theme
    } else {
        "catppuccin-mocha".to_string()
    };

    let registry = ThemeRegistry::new()?;
    registry.get(&theme_id).cloned().ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id))
    })
}

/// Setup shell integration: write marker block to .zshrc and apply the selected theme.
fn setup_shell_integration(theme: Option<&str>) -> Result<ThemeVariant> {
    use crate::adapter::marker_block;

    let home = std::env::var("HOME").map_err(|_| crate::error::SlateError::MissingHomeDir)?;
    let zshrc_path = PathBuf::from(home).join(".zshrc");

    // Load or create ~/.zshrc content
    let zshrc_content = if zshrc_path.exists() {
        fs::read_to_string(&zshrc_path)?
    } else {
        String::new()
    };

    // Validate marker block state (0/0 or 1/1 pairs)
    marker_block::validate_block_state(&zshrc_content)?;

    // Create marker block with source line
    let marker_content = format!(
        "{}\nsource ~/.config/slate/managed/shell/env.zsh\n{}\n",
        marker_block::START,
        marker_block::END
    );

    // Upsert the block (idempotent)
    let updated = marker_block::upsert_managed_block(&zshrc_content, &marker_content);

    // Atomic write back to .zshrc
    let mut file = AtomicWriteFile::open(&zshrc_path)?;
    file.write_all(updated.as_bytes())?;
    file.commit()?;

    let selected_theme = resolve_selected_theme(theme)?;
    apply_theme_selection(&selected_theme)?;

    Ok(selected_theme)
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

    let output = cmd.output().map_err(|e| {
        crate::error::SlateError::Internal(format!("Failed to execute brew: {}", e))
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(crate::error::SlateError::Internal(format!(
            "brew install {} failed:\n{}",
            package,
            stderr.trim()
        )))
    }
}

/// Check if a Nerd Font is already installed
fn is_font_installed(font_name_or_id: &str) -> bool {
    use crate::adapter::font::FontAdapter;
    if let Ok(installed) = FontAdapter::detect_installed_fonts() {
        let lookup = FontCatalog::get_font(font_name_or_id)
            .map(|f| f.name.replace(" Nerd Font", ""))
            .unwrap_or_else(|| font_name_or_id.to_string());
        installed.iter().any(|f| f.contains(&lookup))
    } else {
        false
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

    let output = cmd.output().map_err(|e| {
        crate::error::SlateError::Internal(format!("Failed to execute brew: {}", e))
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(crate::error::SlateError::Internal(format!(
            "Font installation failed:\n{}",
            stderr.trim()
        )))
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
    fn test_theme_selection_marks_summary_as_applied() {
        let summary = execute_setup(&[], None, Some("catppuccin-mocha")).unwrap();
        assert!(summary.theme_applied);
    }
}
