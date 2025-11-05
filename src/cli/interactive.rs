use crate::{ThemeError, ThemeResult};
use dialoguer::Select;

/// Get available theme variants for a given family
/// Returns vec of (theme_name, description) tuples for all themes
/// that start with the given family name.
pub fn get_available_variants(family: &str) -> Vec<(String, String)> {
    let variants = vec![
        // Catppuccin variants
        ("catppuccin-latte", "Light theme"),
        ("catppuccin-frappe", "Cool tone"),
        ("catppuccin-macchiato", "Balanced"),
        ("catppuccin-mocha", "Dark theme"),
        // Tokyo Night variants
        ("tokyo-night-light", "Light theme"),
        ("tokyo-night-dark", "Dark theme"),
        // Single-variant themes
        ("dracula", "Dark theme"),
        ("nord", "Cool north-inspired"),
    ];

    let family_lower = family.to_lowercase();

    variants
        .into_iter()
        .filter(|(name, _)| name.starts_with(&family_lower))
        .map(|(name, desc)| (name.to_string(), desc.to_string()))
        .collect()
}

/// Interactively select a theme variant from incomplete family name
/// Triggered when user enters incomplete theme name (e.g., "catppuccin" without variant).
/// Uses dialoguer::Select to present menu with descriptions.
pub fn pick_theme_variant(family: &str) -> ThemeResult<String> {
    let variants = get_available_variants(family);

    if variants.is_empty() {
        return Err(ThemeError::ThemeNotFound(
            family.to_string(),
            "No variants found for this family".to_string(),
        ));
    }

    if variants.len() == 1 {
        return Ok(variants[0].0.clone());
    }

    // Build display items
    let items: Vec<String> = variants
        .iter()
        .map(|(name, desc)| format!("{:<24} — {}", name, desc))
        .collect();

    let selection = Select::new()
        .with_prompt(&format!(
            "Multiple variants of '{}' found. Which one?",
            family
        ))
        .items(&items)
        .default(0)
        .interact()
        .map_err(|_| ThemeError::Other("Theme selection cancelled".to_string()))?;

    Ok(variants[selection].0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_available_variants_catppuccin() {
        let variants = get_available_variants("catppuccin");
        assert!(!variants.is_empty());
        assert!(variants.iter().any(|(name, _)| name == "catppuccin-mocha"));
        assert!(variants.iter().any(|(name, _)| name == "catppuccin-latte"));
    }

    #[test]
    fn test_get_available_variants_tokyo_night() {
        let variants = get_available_variants("tokyo-night");
        assert!(!variants.is_empty());
        assert!(variants.iter().any(|(name, _)| name == "tokyo-night-dark"));
        assert!(variants.iter().any(|(name, _)| name == "tokyo-night-light"));
    }

    #[test]
    fn test_get_available_variants_single() {
        let variants = get_available_variants("dracula");
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].0, "dracula");
    }

    #[test]
    fn test_get_available_variants_none() {
        let variants = get_available_variants("nonexistent");
        assert!(variants.is_empty());
    }

    #[test]
    fn test_pick_theme_variant_single_variant() {
        let result = pick_theme_variant("dracula");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "dracula");
    }

    #[test]
    fn test_pick_theme_variant_no_variants() {
        let result = pick_theme_variant("nonexistent");
        assert!(result.is_err());
    }
}

/// Get all available theme families
pub fn available_theme_families() -> Vec<&'static str> {
    vec!["catppuccin", "tokyo-night", "dracula", "nord"]
}

/// Get description for a theme
pub fn get_theme_description(theme_name: &str) -> &'static str {
    match theme_name {
        "catppuccin-latte" => "Catppuccin Latte - Light theme with warm pastels",
        "catppuccin-frappe" => "Catppuccin Frappé - Cool tones with smooth contrasts",
        "catppuccin-macchiato" => "Catppuccin Macchiato - Balanced warm and cool tones",
        "catppuccin-mocha" => "Catppuccin Mocha - Dark theme with rich colors",
        "tokyo-night-light" => "Tokyo Night Light - Light theme inspired by Tokyo",
        "tokyo-night-dark" => "Tokyo Night Dark - Dark theme inspired by Tokyo",
        "dracula" => "Dracula - Popular dark theme with vibrant colors",
        "nord" => "Nord - Arctic, north-bluish color palette",
        _ => "Unknown theme",
    }
}

/// Interactively select a theme family
pub fn pick_theme_family() -> ThemeResult<String> {
    let families = available_theme_families();

    if families.is_empty() {
        return Err(ThemeError::Other("No theme families available".to_string()));
    }

    let selection = Select::new()
        .with_prompt("Choose a theme family")
        .items(&families)
        .default(0)
        .interact()
        .map_err(|_| ThemeError::Other("Theme family selection cancelled".to_string()))?;

    Ok(families[selection].to_string())
}

/// Interactively select a restore point
/// Presents a menu with restore point timestamp, theme name, and tools
/// Returns the selected RestorePoint or error if cancelled/no points available
pub fn pick_restore_point(
    restore_points: &[crate::config::backup::RestorePoint],
) -> ThemeResult<crate::config::backup::RestorePoint> {
    if restore_points.is_empty() {
        return Err(ThemeError::Other("No restore points available".to_string()));
    }

    // Build display items
    let items: Vec<String> = restore_points
        .iter()
        .map(|rp| {
            let tools_str = crate::config::backup::display_tools(&rp.entries).join(", ");
            format!("{}  {}  [{}]", rp.id, rp.theme_name, tools_str)
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select a restore point to restore")
        .items(&items)
        .default(0)
        .interact()
        .map_err(|_| ThemeError::Other("Restore cancelled".to_string()))?;

    Ok(restore_points[selection].clone())
}
