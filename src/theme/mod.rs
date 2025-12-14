use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export theme variants
pub mod catppuccin;
pub mod dracula;
pub mod gruvbox;
pub mod nord;
pub mod tokyo_night;

/// Shared default theme ID used when Slate needs a fallback theme.
pub const DEFAULT_THEME_ID: &str = "catppuccin-mocha";

/// Color palette for a theme.
/// Per revised: Hybrid design with semantic UI colors (5) + ANSI normal/bright (16) as named fields,
/// plus extras for theme-specific colors. Zero-allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Palette {
    // Semantic UI colors (all themes have these)
    pub foreground: String, // Hex: #RRGGBB
    pub background: String,
    pub cursor: Option<String>,
    pub selection_bg: Option<String>,
    pub selection_fg: Option<String>,

    // Standard ANSI colors (black/8 colors + bright variants)
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,

    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,

    // Semantic background variants (language-neutral names)
    pub bg_dim: Option<String>, // Medium background, was "base" in Catppuccin
    pub bg_darker: Option<String>, // Darker background, was "mantle" in Catppuccin
    pub bg_darkest: Option<String>, // Darkest background, was "crust" in Catppuccin

    // Catppuccin-specific colors (optional)
    pub rosewater: Option<String>,
    pub flamingo: Option<String>,
    pub pink: Option<String>,
    pub mauve: Option<String>,
    pub lavender: Option<String>,
    pub text: Option<String>,
    pub subtext1: Option<String>,
    pub subtext0: Option<String>,
    pub overlay2: Option<String>,
    pub overlay1: Option<String>,
    pub overlay0: Option<String>,
    pub surface2: Option<String>,
    pub surface1: Option<String>,
    pub surface0: Option<String>,

    // extras HashMap for theme-specific color values
    #[serde(default)]
    pub extras: HashMap<String, String>,
}

impl Palette {
    /// Verify palette has all required fields populated
    pub fn validate(&self) -> Result<()> {
        if self.foreground.is_empty() || self.background.is_empty() {
            return Err(crate::error::SlateError::InvalidThemeData(
                "Palette missing required colors".to_string(),
            ));
        }
        Ok(())
    }
}

/// Per-tool theme references.
/// ToolRefs is now a HashMap<String, String> type alias, enabling new adapters to be added
/// without modifying the core type definition (Open/Closed principle).
/// Each tool uses different naming convention.
/// Example:
/// - Ghostty: "Catppuccin Mocha" (Title Case with spaces)
/// - Alacritty: "catppuccin_mocha" (snake_case)
/// - bat: "Catppuccin Mocha" (Title Case)
pub type ToolRefs = HashMap<String, String>;

/// Theme appearance classification for auto-follow detection.
/// Themes are classified as either Dark or Light.
/// This enables the auto-follow feature to match system appearance (macOS Settings)
/// with the appropriate theme variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeAppearance {
    /// Dark theme (suitable when macOS is in Dark mode)
    Dark,
    /// Light theme (suitable when macOS is in Light mode)
    Light,
}

/// A single theme variant (e.g., "Catppuccin Mocha").
/// Contains both tool_refs and palette for complete theme data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeVariant {
    pub id: String,     // Unique identifier (e.g., "catppuccin-mocha") — kebab-case
    pub name: String,   // Display name (e.g., "Catppuccin Mocha")
    pub family: String, // Family (e.g., "Catppuccin")
    pub tool_refs: ToolRefs, // Now HashMap<String, String>
    pub palette: Palette, // Raw colors for tools without built-in support
    pub appearance: ThemeAppearance, // Dark or Light classification
    pub auto_pair: Option<&'static str>, // Paired dark/light variant ID, if applicable
}

impl ThemeVariant {
    /// Validate theme variant
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() || self.name.is_empty() {
            return Err(crate::error::SlateError::InvalidThemeData(format!(
                "Theme {} missing required fields",
                self.id
            )));
        }
        self.palette.validate()?;
        Ok(())
    }
}

/// Theme loader and registry.
/// Embedded in binary; loads all 10 variants at startup.
pub struct ThemeRegistry {
    variants: HashMap<String, ThemeVariant>,
}

impl ThemeRegistry {
    /// Create registry with all embedded themes
    pub fn new() -> Result<Self> {
        let mut variants = HashMap::new();

        // Catppuccin variants (sync fresh from official repo)
        let cat_latte = catppuccin::catppuccin_latte()?;
        let cat_frappe = catppuccin::catppuccin_frappe()?;
        let cat_macchiato = catppuccin::catppuccin_macchiato()?;
        let cat_mocha = catppuccin::catppuccin_mocha()?;

        // Tokyo Night variants
        let tn_light = tokyo_night::tokyo_night_light()?;
        let tn_dark = tokyo_night::tokyo_night_dark()?;

        // Dracula
        let drac = dracula::dracula()?;

        // Nord
        let nd = nord::nord()?;

        // Gruvbox variants
        let gruvbox_dark = gruvbox::gruvbox_dark()?;
        let gruvbox_light = gruvbox::gruvbox_light()?;

        // Register all variants
        for variant in &[
            &cat_latte,
            &cat_frappe,
            &cat_macchiato,
            &cat_mocha,
            &tn_light,
            &tn_dark,
            &drac,
            &nd,
            &gruvbox_dark,
            &gruvbox_light,
        ] {
            variants.insert(variant.id.clone(), (*variant).clone());
        }

        Ok(Self { variants })
    }

    /// Get theme variant by ID
    pub fn get(&self, id: &str) -> Option<&ThemeVariant> {
        self.variants.get(id)
    }

    /// Get all theme variants
    pub fn all(&self) -> Vec<&ThemeVariant> {
        self.variants.values().collect()
    }

    /// List all theme IDs
    pub fn list_ids(&self) -> Vec<String> {
        self.variants.keys().cloned().collect()
    }

    /// Get themes grouped by family
    pub fn by_family(&self) -> HashMap<String, Vec<&ThemeVariant>> {
        let mut families = HashMap::new();
        for variant in self.variants.values() {
            families
                .entry(variant.family.clone())
                .or_insert_with(Vec::new)
                .push(variant);
        }
        families
    }
}

impl Default for ThemeRegistry {
    fn default() -> Self {
        Self::new().expect("Failed to initialize ThemeRegistry with embedded themes")
    }
}

/// Static family sort order
/// Guides users toward most popular and well-regarded themes first
pub const FAMILY_SORT_ORDER: &[&str] = &[
    "Catppuccin",
    "Tokyo Night",
    "Rosé Pine",
    "Kanagawa",
    "Everforest",
    "Dracula",
    "Nord",
    "Gruvbox",
];

/// Get display description for a theme
/// Used by `slate list` command 
pub fn get_theme_description(theme_id: &str) -> Option<&'static str> {
    match theme_id {
        "catppuccin-mocha" => Some("Deep, warm mocha with sophisticated contrast"),
        "catppuccin-frappe" => Some("Elegant frappé with subtle charm"),
        "catppuccin-macchiato" => Some("Smooth macchiato for balanced aesthetics"),
        "catppuccin-latte" => Some("Bright, airy latte perfect for light mode"),
        "tokyo-night-dark" => Some("Modern dark with electric blues and purples"),
        "tokyo-night-light" => Some("Crisp light theme with Tokyo Night flair"),
        "gruvbox-dark" => Some("Retro-inspired dark with earthy tones"),
        "gruvbox-light" => Some("Vintage light theme with warm nostalgia"),
        "dracula" => Some("Moody and dramatic with vibrant accents"),
        "nord" => Some("Arctic, north-bluish dark color palette"),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_validation() {
        let palette = Palette {
            foreground: "#ffffff".to_string(),
            background: "#000000".to_string(),
            cursor: None,
            selection_bg: None,
            selection_fg: None,
            black: "#000000".to_string(),
            red: "#ff0000".to_string(),
            green: "#00ff00".to_string(),
            yellow: "#ffff00".to_string(),
            blue: "#0000ff".to_string(),
            magenta: "#ff00ff".to_string(),
            cyan: "#00ffff".to_string(),
            white: "#ffffff".to_string(),
            bright_black: "#555555".to_string(),
            bright_red: "#ff5555".to_string(),
            bright_green: "#55ff55".to_string(),
            bright_yellow: "#ffff55".to_string(),
            bright_blue: "#5555ff".to_string(),
            bright_magenta: "#ff55ff".to_string(),
            bright_cyan: "#55ffff".to_string(),
            bright_white: "#ffffff".to_string(),
            bg_dim: None,
            bg_darker: None,
            bg_darkest: None,
            rosewater: None,
            flamingo: None,
            pink: None,
            mauve: None,
            lavender: None,
            text: None,
            subtext1: None,
            subtext0: None,
            overlay2: None,
            overlay1: None,
            overlay0: None,
            surface2: None,
            surface1: None,
            surface0: None,
            extras: HashMap::new(),
        };

        assert!(palette.validate().is_ok());
    }

    #[test]
    fn test_tool_refs_lookup() {
        let mut refs = HashMap::new();
        refs.insert("ghostty".to_string(), "Test Ghostty".to_string());
        refs.insert("alacritty".to_string(), "test_alacritty".to_string());
        refs.insert("bat".to_string(), "Test Bat".to_string());
        refs.insert("delta".to_string(), "test_delta".to_string());
        refs.insert("starship".to_string(), "test_starship".to_string());
        refs.insert("eza".to_string(), "test_eza".to_string());
        refs.insert("lazygit".to_string(), "test_lazygit".to_string());
        refs.insert("fastfetch".to_string(), "test_fastfetch".to_string());
        refs.insert("tmux".to_string(), "test_tmux".to_string());
        refs.insert(
            "zsh_syntax_highlighting".to_string(),
            "test_zsh".to_string(),
        );

        assert_eq!(
            refs.get("ghostty").map(String::as_str),
            Some("Test Ghostty")
        );
        assert_eq!(refs.get("bat").map(String::as_str), Some("Test Bat"));
        assert_eq!(refs.get("unknown"), None);
    }

    /// Regression guard for the Ghostty theme-name naming convention.
    /// Ghostty ships built-in themes under specific names — slate's ghostty
    /// tool_ref strings must match those names exactly or Ghostty raises
    /// `theme "X" not found` at reload time. This caught the tokyo-night
    /// mismatch where slate was writing `"Tokyo Night Light"`/`"Tokyo Night"`
    /// but Ghostty ships them as `"TokyoNight Day"`/`"TokyoNight"`.
    /// The expected values below were captured from
    /// `ghostty +list-themes` on Ghostty 1.3.1. When Ghostty adds or renames
    /// built-ins, update this table alongside the corresponding theme file.
    #[test]
    fn test_ghostty_tool_refs_match_builtin_theme_names() {
        let registry = ThemeRegistry::new().expect("registry constructs");
        let expected: &[(&str, &str)] = &[
            ("catppuccin-latte", "Catppuccin Latte"),
            ("catppuccin-frappe", "Catppuccin Frappé"),
            ("catppuccin-macchiato", "Catppuccin Macchiato"),
            ("catppuccin-mocha", "Catppuccin Mocha"),
            ("tokyo-night-light", "TokyoNight Day"),
            ("tokyo-night-dark", "TokyoNight"),
            ("dracula", "Dracula"),
            ("nord", "Nord"),
            ("gruvbox-dark", "Gruvbox Dark"),
            ("gruvbox-light", "Gruvbox Light"),
        ];

        for (theme_id, expected_ghostty_name) in expected {
            let theme = registry
                .get(theme_id)
                .unwrap_or_else(|| panic!("theme '{}' missing from registry", theme_id));
            let actual = theme
                .tool_refs
                .get("ghostty")
                .unwrap_or_else(|| panic!("theme '{}' has no ghostty tool_ref", theme_id));
            assert_eq!(
                actual, expected_ghostty_name,
                "ghostty tool_ref for '{}' does not match Ghostty's built-in \
                 theme name — slate will write an invalid `theme = \"...\"` \
                 line and Ghostty will raise 'theme not found'",
                theme_id
            );
        }
    }
}
