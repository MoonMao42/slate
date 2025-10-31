use crate::adapter::{BatAdapter, DeltaAdapter, GhosttyAdapter, LazygitAdapter, StarshipAdapter};
use crate::{
    available_themes, get_theme, normalize_theme_name, parse_theme_input, ApplyThemeResult,
    ThemeError, ThemeResult, ToolRegistry,
};

/// Handle the `set` subcommand: apply theme to all detected tools
pub fn handle_set_command(theme_input: &str, verbose: bool) -> ThemeResult<ApplyThemeResult> {
    // Normalize input to kebab-case
    let normalized = normalize_theme_name(theme_input);

    // Try to get the theme directly
    let final_theme_name = if let Some(_theme) = get_theme(&normalized) {
        normalized
    } else {
        // Try parsing to see if it's a family name
        if let Some((family, variant_opt)) = parse_theme_input(&normalized) {
            if variant_opt.is_none() {
                // Incomplete family name - show interactive selection
                let selected = crate::cli::pick_theme_variant(&family.to_string())?;
                selected
            } else {
                // Has variant but theme not found - error
                let available = available_themes().join(", ");
                return Err(ThemeError::ThemeNotFound(normalized, available));
            }
        } else {
            // Not a valid family - error
            let available = available_themes().join(", ");
            return Err(ThemeError::ThemeNotFound(normalized, available));
        }
    };

    // Verify the final theme exists
    let theme = get_theme(&final_theme_name).ok_or_else(|| {
        let available = available_themes().join(", ");
        ThemeError::ThemeNotFound(final_theme_name.clone(), available)
    })?;

    // Create registry and register adapters
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(GhosttyAdapter));
    registry.register(Box::new(StarshipAdapter));
    registry.register(Box::new(BatAdapter));
    registry.register(Box::new(DeltaAdapter));
    registry.register(Box::new(LazygitAdapter));

    // Print verbose detection if requested
    if verbose {
        let mut detected = Vec::new();
        for adapter in registry.adapters() {
            if let Ok(true) = adapter.is_installed() {
                if let Ok(path) = adapter.config_path() {
                    // Distinguish existing config vs config that will be created
                    let label = if path.exists() {
                        format!("Found at {}", path.display())
                    } else {
                        format!("Installed (config will be created at {})", path.display())
                    };
                    detected.push((adapter.tool_name().to_string(), Some(label)));
                }
            } else {
                detected.push((adapter.tool_name().to_string(), None));
            }
        }
        println!("{}", crate::cli::format_verbose_detection(&detected));
        println!();
    }

    // Auto-create configs for installed tools if needed
    for adapter in registry.adapters() {
        if let Ok(true) = adapter.is_installed() {
            if let Ok(config_path) = adapter.config_path() {
                let _ = crate::cli::auto_create_config(adapter.tool_name(), &config_path);
            }
        }
    }

    // Apply theme to all tools
    let result = registry.apply_theme_to_all(&theme)?;

    // 0 tools detected = error before printing any success output
    if result.count_successful() == 0 && result.count_failed() == 0 {
        return Err(ThemeError::NoToolsDetected);
    }

    // Print results only after confirming at least one tool was processed
    println!("{}", crate::cli::format_success_header(&final_theme_name));
    println!();

    for tool in &result.successful {
        println!("{}", crate::cli::format_tool_status(tool, true, "Updated"));
    }

    for (tool, error) in &result.failed {
        println!("{}", crate::cli::format_tool_status(tool, false, error));
    }

    println!();
    println!(
        "{}",
        crate::cli::format_summary(
            result.count_successful(),
            result.count_successful() + result.count_failed(),
            result.count_failed()
        )
    );

    if result.is_success() {
        Ok(result)
    } else {
        Err(ThemeError::PartialFailure(result.count_failed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_set_command_with_valid_theme() {
        let result = handle_set_command("catppuccin-mocha", false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_set_command_with_invalid_theme() {
        let result = handle_set_command("nonexistent-theme", false);
        assert!(result.is_err());
    }
}

/// Handle the `status` subcommand: show current theme state of installed tools
pub fn handle_status_command(verbose: bool) -> ThemeResult<()> {
    // Create registry and register all 5 adapters
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(GhosttyAdapter));
    registry.register(Box::new(StarshipAdapter));
    registry.register(Box::new(BatAdapter));
    registry.register(Box::new(DeltaAdapter));
    registry.register(Box::new(LazygitAdapter));

    // Collect status for installed tools
    let mut found_any = false;
    let mut output_lines = Vec::new();

    for adapter in registry.adapters() {
        if let Ok(true) = adapter.is_installed() {
            found_any = true;
            let tool_name = adapter.tool_name();

            // Try to read current theme
            let current_theme = match adapter.get_current_theme() {
                Ok(Some(theme)) => theme,
                Ok(None) => "unknown".to_string(),
                Err(_) => {
                    if verbose {
                        eprintln!("[Warning] Could not read theme state for {}", tool_name);
                    }
                    "unknown".to_string()
                }
            };

            output_lines.push((tool_name.to_string(), current_theme));
        }
    }

    // If no tools found, print informational message
    if !found_any {
        println!("No supported tools detected");
        return Ok(());
    }

    // Print status header
    println!("{}", crate::cli::format_status_header());

    // Print each tool's status
    for (tool, theme) in output_lines {
        println!("{}", crate::cli::format_status_line(&tool, &theme));
    }

    Ok(())
}

/// Handle the `list` subcommand: show available themes
pub fn handle_list_command() -> ThemeResult<()> {
    use atty::Stream;

    // Check if stdout is a TTY
    if !atty::is(Stream::Stdout) {
        // Plain text mode: grouped by family with descriptions
        print_plain_theme_list()?;
    } else {
        // Interactive mode: family -> variant -> confirm -> apply
        run_interactive_list()?;
    }

    Ok(())
}

/// Print themes grouped by family in plain text format (for piping)
fn print_plain_theme_list() -> ThemeResult<()> {
    let themes = available_themes();

    // Group themes by family
    let families = vec![
        (
            "Catppuccin",
            vec![
                "catppuccin-latte",
                "catppuccin-frappe",
                "catppuccin-macchiato",
                "catppuccin-mocha",
            ],
        ),
        ("Tokyo Night", vec!["tokyo-night-light", "tokyo-night-dark"]),
        ("Dracula", vec!["dracula"]),
        ("Nord", vec!["nord"]),
    ];

    for (family_name, theme_names) in families {
        println!("{}:", family_name);
        for theme_name in theme_names {
            if themes.contains(&theme_name.to_string()) {
                let description = crate::cli::get_theme_description(theme_name);
                println!("  {} - {}", theme_name, description);
            }
        }
        println!();
    }

    Ok(())
}

/// Run interactive theme picker: family -> variant -> confirm -> apply
fn run_interactive_list() -> ThemeResult<()> {
    use dialoguer::Confirm;

    // Pick family
    let family = crate::cli::pick_theme_family()?;

    // Pick variant
    let theme = crate::cli::pick_theme_variant(&family)?;

    // Confirm before applying
    let description = crate::cli::get_theme_description(&theme);
    let prompt = format!("Apply theme: {} ({})?\nConfirm", theme, description);

    let confirmed = Confirm::new()
        .with_prompt(&prompt)
        .default(true)
        .interact()
        .map_err(|_| ThemeError::Other("Theme confirmation cancelled".to_string()))?;

    if confirmed {
        // Apply the theme using handle_set_command
        handle_set_command(&theme, false)?;
    } else {
        println!("Theme application cancelled");
    }

    Ok(())
}

/// Handle the `restore` subcommand: restore from backups or manage restore points
/// Mode validation: exactly one mode must be selected
/// - `restore_point_id` (positional): restore by ID
/// - `--list`: list all restore points
/// - `--cleanup <id>`: delete one restore point
/// - `--clear-all`: delete all restore points
/// - no args + TTY: interactive selection
/// - no args + non-TTY: error with guidance
pub fn handle_restore_command(
    restore_point_id: Option<String>,
    list: bool,
    cleanup: Option<String>,
    clear_all: bool,
) -> ThemeResult<()> {
    // Validate mode combinations - exactly one mode should be active
    let active_modes = [
        restore_point_id.is_some(),
        list,
        cleanup.is_some(),
        clear_all,
    ]
    .iter()
    .filter(|&&m| m)
    .count();

    if active_modes > 1 {
        return Err(ThemeError::Other(
            "Error: Conflicting modes\n\n    Problem: Cannot combine --list, --cleanup, --clear-all, or a restore point ID\n\nGuidance: Use one mode at a time:\n    themectl restore <id>          # Restore by ID\n    themectl restore --list        # List restore points\n    themectl restore --cleanup <id> # Delete a restore point\n    themectl restore --clear-all   # Delete all restore points".to_string()
        ));
    }

    // Handle --list mode
    if list {
        let restore_points = crate::config::backup::list_restore_points()?;
        
        if restore_points.is_empty() {
            println!("No restore points available");
            return Ok(());
        }

        println!("{}", crate::cli::format_restore_point_list(&restore_points));
        return Ok(());
    }

    // Handle --cleanup mode
    if let Some(id) = cleanup {
        crate::config::backup::validate_restore_point(&id)?;
        let deleted_count = crate::config::backup::delete_restore_point(&id)?;
        println!("Deleted {} backup file(s) from restore point: {}", deleted_count, id);
        return Ok(());
    }

    // Handle --clear-all mode
    if clear_all {
        let deleted_count = crate::config::backup::clear_all_restore_points()?;
        println!("Deleted all {} restore point(s)", deleted_count);
        return Ok(());
    }

    // Handle direct restore by ID
    if let Some(id) = restore_point_id {
        crate::config::backup::validate_restore_point(&id)?;
        let restore_point = crate::config::backup::get_restore_point(&id)?;
        let result = crate::config::backup::restore_restore_point(&id)?;
        println!("{}", crate::cli::format_restore_result(&restore_point.theme_name, &result));
        return Ok(());
    }

    // No args - check if TTY
    use atty::Stream;
    if !atty::is(Stream::Stdout) {
        return Err(ThemeError::Other(
            "Error: No restore point specified in non-interactive mode\n\n    Problem: themectl restore requires either a restore point ID or TTY interactive selection\n\nGuidance: Use one of:\n    themectl restore --list              # List available restore points\n    themectl restore <restore_point_id>  # Restore by ID\n    Run in a terminal for interactive selection".to_string()
        ));
    }

    // TTY mode - interactive selection
    let restore_points = crate::config::backup::list_restore_points()?;
    
    if restore_points.is_empty() {
        return Err(ThemeError::Other(
            "No restore points available to restore from".to_string()
        ));
    }

    let selected = crate::cli::pick_restore_point(&restore_points)?;
    let result = crate::config::backup::restore_restore_point(&selected.id)?;
    println!("{}", crate::cli::format_restore_result(&selected.theme_name, &result));
    Ok(())
}
