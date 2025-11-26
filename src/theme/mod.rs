use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::error::Result;

// Re-export theme variants
pub mod catppuccin;
pub mod tokyo_night;
pub mod dracula;
pub mod nord;
pub mod gruvbox;

/// Shared default theme ID used when Slate needs a fallback theme.
pub const DEFAULT_THEME_ID: &str = "catppuccin-mocha";

/// Color palette for a theme.
/// Per revised: Hybrid design with semantic UI colors (5) + ANSI normal/bright (16) as named fields,
/// plus extras for theme-specific colors. Zero-allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Palette {
    // Semantic UI colors (all themes have these)
    pub foreground: String,    // Hex: #RRGGBB
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

    // Catppuccin-specific colors (optional)
    // All themes must populate base, mantle, crust fields for Starship powerline compatibility.
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
    pub base: Option<String>,         // Catppuccin base (darker background variant)
    pub mantle: Option<String>,       // Catppuccin mantle (slightly lighter background)
    pub crust: Option<String>,        // Catppuccin crust (darkest, almost black)
}

impl Palette {
    /// Verify palette has all required fields populated
    pub fn validate(&self) -> Result<()> {
        if self.foreground.is_empty() || self.background.is_empty() {
            return Err(crate::error::SlateError::InvalidThemeData(
                "Palette missing required colors".to_string()
            ));
        }
        Ok(())
    }
}

/// Per-tool theme references.
/// Each tool uses different naming convention.
/// Example:
/// - Ghostty: "Catppuccin Mocha" (Title Case with spaces)
/// - Alacritty: "catppuccin_mocha" (snake_case)
/// - bat: "Catppuccin Mocha" (Title Case)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRefs {
    pub ghostty: String,
    pub alacritty: String,
    pub bat: String,
    pub delta: String,
    pub starship: String,
    pub eza: String,
    pub lazygit: String,
    pub fastfetch: String,
    pub tmux: String,
    pub zsh_syntax_highlighting: String,
}

impl ToolRefs {
    /// Get theme reference for a specific tool
    pub fn get(&self, tool: &str) -> Option<&str> {
        match tool {
            "ghostty" => Some(&self.ghostty),
            "alacritty" => Some(&self.alacritty),
            "bat" => Some(&self.bat),
            "delta" => Some(&self.delta),
            "starship" => Some(&self.starship),
            "eza" => Some(&self.eza),
            "lazygit" => Some(&self.lazygit),
            "fastfetch" => Some(&self.fastfetch),
            "tmux" => Some(&self.tmux),
            "zsh-syntax-highlighting" => Some(&self.zsh_syntax_highlighting),
            _ => None,
        }
    }
}

/// A single theme variant (e.g., "Catppuccin Mocha").
/// Contains both tool_refs and palette for complete theme data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeVariant {
    pub id: String,           // Unique identifier (e.g., "catppuccin-mocha") — kebab-case
    pub name: String,         // Display name (e.g., "Catppuccin Mocha")
    pub family: String,       // Family (e.g., "Catppuccin")
    pub tool_refs: ToolRefs,  // Per-tool theme names
    pub palette: Palette,     // Raw colors for tools without built-in support
}

impl ThemeVariant {
    /// Validate theme variant
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() || self.name.is_empty() {
            return Err(crate::error::SlateError::InvalidThemeData(
                format!("Theme {} missing required fields", self.id)
            ));
        }
        self.palette.validate()?;
        Ok(())
    }
}

/// Theme loader and registry.
/// Embedded in binary; loads all 8 variants at startup.
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
        for variant in &[&cat_latte, &cat_frappe, &cat_macchiato, &cat_mocha,
                        &tn_light, &tn_dark, &drac, &nd, &gruvbox_dark, &gruvbox_light] {
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
            base: None,
            mantle: None,
            crust: None,
        };

        assert!(palette.validate().is_ok());
    }

    #[test]
    fn test_tool_refs_lookup() {
        let refs = ToolRefs {
            ghostty: "Test Ghostty".to_string(),
            alacritty: "test_alacritty".to_string(),
            bat: "Test Bat".to_string(),
            delta: "test_delta".to_string(),
            starship: "test_starship".to_string(),
            eza: "test_eza".to_string(),
            lazygit: "test_lazygit".to_string(),
            fastfetch: "test_fastfetch".to_string(),
            tmux: "test_tmux".to_string(),
            zsh_syntax_highlighting: "test_zsh".to_string(),
        };

        assert_eq!(refs.get("ghostty"), Some("Test Ghostty"));
        assert_eq!(refs.get("bat"), Some("Test Bat"));
        assert_eq!(refs.get("unknown"), None);
    }
}
