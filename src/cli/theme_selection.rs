use crate::error::Result;
/// Theme selection for setup wizard.
/// Per and from 02-.
/// Provides access to 10 theme variants grouped by family.
use crate::theme::{ThemeRegistry, DEFAULT_THEME_ID};
use std::collections::HashMap;

/// Theme choice helper: groups themes by family with descriptions
pub struct ThemeSelector {
    registry: ThemeRegistry,
}

impl ThemeSelector {
    /// Create a new theme selector with all embedded themes
    pub fn new() -> Result<Self> {
        Ok(Self {
            registry: ThemeRegistry::new()?,
        })
    }

    /// Get all available theme variants
    /// Should be exactly 10 (Catppuccin 4 + Tokyo Night 2 + Dracula + Nord + Gruvbox 2)
    pub fn all_themes(&self) -> Vec<&crate::theme::ThemeVariant> {
        self.registry.all()
    }

    /// Get themes grouped by family (for manual mode step)
    /// Returns HashMap<family_name, Vec<theme_variant>>
    pub fn themes_by_family(&self) -> HashMap<String, Vec<&crate::theme::ThemeVariant>> {
        self.registry.by_family()
    }

    /// Get theme by ID
    pub fn get_theme(&self, id: &str) -> Option<&crate::theme::ThemeVariant> {
        self.registry.get(id)
    }

    /// Get default theme for quick mode ("catppuccin-mocha")
    pub fn default_theme_id() -> &'static str {
        DEFAULT_THEME_ID
    }

    /// Get brief description for a theme family (for UX)
    pub fn family_description(family: &str) -> &'static str {
        match family {
            "Catppuccin" => "Cozy, colorful community-driven palettes",
            "Tokyo Night" => "Vibrant Japanese-inspired themes",
            "Dracula" => "High contrast dark palette with vivid colors",
            "Nord" => "Arctic polar night color scheme",
            "Gruvbox" => "Retro warm palette inspired by classic Vim",
            _ => "Beautiful color scheme",
        }
    }

    /// Get all theme IDs (for validation)
    pub fn all_theme_ids(&self) -> Vec<String> {
        self.registry.list_ids()
    }

    /// Count themes (should be exactly 10)
    pub fn theme_count(&self) -> usize {
        self.all_themes().len()
    }

    /// Verify all 10 required theme variants are present
    pub fn verify_all_variants_present(&self) -> Result<()> {
        let themes = self.all_theme_ids();
        let expected = vec![
            "catppuccin-latte",
            "catppuccin-frappe",
            "catppuccin-macchiato",
            "catppuccin-mocha",
            "tokyo-night-light",
            "tokyo-night-dark",
            "dracula",
            "nord",
            "gruvbox-dark",
            "gruvbox-light",
        ];

        for theme_id in &expected {
            if !themes.contains(&theme_id.to_string()) {
                return Err(crate::error::SlateError::InvalidThemeData(format!(
                    "Missing required theme: {}",
                    theme_id
                )));
            }
        }

        if themes.len() != expected.len() {
            return Err(crate::error::SlateError::InvalidThemeData(format!(
                "Expected {} theme variants, found {}",
                expected.len(),
                themes.len()
            )));
        }

        Ok(())
    }
}

impl Default for ThemeSelector {
    fn default() -> Self {
        Self::new().expect("Failed to initialize ThemeSelector")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_10_themes_available() {
        let selector = ThemeSelector::new().unwrap();
        assert_eq!(
            selector.theme_count(),
            10,
            "Must have exactly 10 theme variants per "
        );
    }

    #[test]
    fn test_required_variants_present() {
        let selector = ThemeSelector::new().unwrap();
        assert!(
            selector.verify_all_variants_present().is_ok(),
            "All required theme variants must be present"
        );
    }

    #[test]
    fn test_themes_grouped_by_family() {
        let selector = ThemeSelector::new().unwrap();
        let families = selector.themes_by_family();

        // Should have 5 families
        assert_eq!(
            families.len(),
            5,
            "Expected 5 theme families: Catppuccin, Tokyo Night, Dracula, Nord, Gruvbox"
        );

        // Each family should have the right count
        assert_eq!(
            families.get("Catppuccin").map(|v| v.len()),
            Some(4),
            "Catppuccin should have 4 variants"
        );
        assert_eq!(
            families.get("Tokyo Night").map(|v| v.len()),
            Some(2),
            "Tokyo Night should have 2 variants"
        );
        assert_eq!(
            families.get("Gruvbox").map(|v| v.len()),
            Some(2),
            "Gruvbox should have 2 variants"
        );
        assert_eq!(
            families.get("Dracula").map(|v| v.len()),
            Some(1),
            "Dracula should have 1 variant"
        );
        assert_eq!(
            families.get("Nord").map(|v| v.len()),
            Some(1),
            "Nord should have 1 variant"
        );
    }

    #[test]
    fn test_default_theme_exists() {
        let selector = ThemeSelector::new().unwrap();
        let default_id = ThemeSelector::default_theme_id();
        assert!(
            selector.get_theme(default_id).is_some(),
            "Default theme must exist: {}",
            default_id
        );
    }

    #[test]
    fn test_family_descriptions_exist() {
        let families = vec!["Catppuccin", "Tokyo Night", "Dracula", "Nord", "Gruvbox"];
        for family in families {
            let desc = ThemeSelector::family_description(family);
            assert!(
                !desc.is_empty(),
                "Family description should not be empty: {}",
                family
            );
        }
    }

    #[test]
    fn test_get_theme_by_id() {
        let selector = ThemeSelector::new().unwrap();
        assert!(selector.get_theme("catppuccin-mocha").is_some());
        assert!(selector.get_theme("gruvbox-dark").is_some());
        assert!(selector.get_theme("nonexistent").is_none());
    }

    #[test]
    fn test_gruvbox_themes_selectable() {
        // Verify Gruvbox Dark and Light are in the selection
        let selector = ThemeSelector::new().unwrap();
        assert!(
            selector.get_theme("gruvbox-dark").is_some(),
            "Gruvbox Dark must be available"
        );
        assert!(
            selector.get_theme("gruvbox-light").is_some(),
            "Gruvbox Light must be available"
        );
    }

    #[test]
    fn test_rerun_behavior_awareness() {
        // ThemeSelector provides data for rerun behavior awareness
        let selector = ThemeSelector::new().unwrap();
        // Verify selectors are ready to detect and display current theme
        assert!(!selector.all_theme_ids().is_empty());
        assert!(selector.theme_count() > 0);
    }
}
