use crate::{
    available_themes, get_theme, normalize_theme_name, parse_theme_input,
    ApplyThemeResult, ThemeError, ThemeResult, ToolRegistry,
};
use crate::adapter::{GhosttyAdapter, StarshipAdapter, BatAdapter, DeltaAdapter};

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
    let theme = get_theme(&final_theme_name)
        .ok_or_else(|| {
            let available = available_themes().join(", ");
            ThemeError::ThemeNotFound(final_theme_name.clone(), available)
        })?;

    // Create registry and register adapters
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(GhosttyAdapter));
    registry.register(Box::new(StarshipAdapter));
    registry.register(Box::new(BatAdapter));
    registry.register(Box::new(DeltaAdapter));

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
