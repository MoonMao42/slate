use crate::error::{Result, SlateError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

// Re-export theme variants
pub mod catppuccin;
pub mod dracula;
pub mod everforest;
pub mod gruvbox;
pub mod kanagawa;
pub mod nord;
pub mod rose_pine;
pub mod tokyo_night;

/// Shared default theme ID used when Slate needs a fallback theme.
pub const DEFAULT_THEME_ID: &str = "catppuccin-mocha";

const REQUIRED_TOOL_REFS: &[&str] = &[
    "alacritty",
    "bat",
    "delta",
    "eza",
    "fastfetch",
    "ghostty",
    "lazygit",
    "starship",
    "tmux",
    "zsh_syntax_highlighting",
];

const EMBEDDED_THEMES_TOML: &str = include_str!("../../themes/themes.toml");
static EMBEDDED_THEMES: OnceLock<std::result::Result<EmbeddedThemes, String>> = OnceLock::new();

/// Color palette for a theme.
/// Hybrid design with semantic UI colors (5) + ANSI normal/bright (16) as named fields,
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

    // Semantic background variants: language-neutral names)
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
    /// Verify palette has all required fields populated with valid hex colors.
    pub fn validate(&self) -> Result<()> {
        validate_hex_color("foreground", &self.foreground)?;
        validate_hex_color("background", &self.background)?;
        validate_hex_color("black", &self.black)?;
        validate_hex_color("red", &self.red)?;
        validate_hex_color("green", &self.green)?;
        validate_hex_color("yellow", &self.yellow)?;
        validate_hex_color("blue", &self.blue)?;
        validate_hex_color("magenta", &self.magenta)?;
        validate_hex_color("cyan", &self.cyan)?;
        validate_hex_color("white", &self.white)?;
        validate_hex_color("bright_black", &self.bright_black)?;
        validate_hex_color("bright_red", &self.bright_red)?;
        validate_hex_color("bright_green", &self.bright_green)?;
        validate_hex_color("bright_yellow", &self.bright_yellow)?;
        validate_hex_color("bright_blue", &self.bright_blue)?;
        validate_hex_color("bright_magenta", &self.bright_magenta)?;
        validate_hex_color("bright_cyan", &self.bright_cyan)?;
        validate_hex_color("bright_white", &self.bright_white)?;

        validate_optional_hex_color("cursor", self.cursor.as_deref())?;
        validate_optional_hex_color("selection_bg", self.selection_bg.as_deref())?;
        validate_optional_hex_color("selection_fg", self.selection_fg.as_deref())?;
        validate_optional_hex_color("bg_dim", self.bg_dim.as_deref())?;
        validate_optional_hex_color("bg_darker", self.bg_darker.as_deref())?;
        validate_optional_hex_color("bg_darkest", self.bg_darkest.as_deref())?;
        validate_optional_hex_color("rosewater", self.rosewater.as_deref())?;
        validate_optional_hex_color("flamingo", self.flamingo.as_deref())?;
        validate_optional_hex_color("pink", self.pink.as_deref())?;
        validate_optional_hex_color("mauve", self.mauve.as_deref())?;
        validate_optional_hex_color("lavender", self.lavender.as_deref())?;
        validate_optional_hex_color("text", self.text.as_deref())?;
        validate_optional_hex_color("subtext1", self.subtext1.as_deref())?;
        validate_optional_hex_color("subtext0", self.subtext0.as_deref())?;
        validate_optional_hex_color("overlay2", self.overlay2.as_deref())?;
        validate_optional_hex_color("overlay1", self.overlay1.as_deref())?;
        validate_optional_hex_color("overlay0", self.overlay0.as_deref())?;
        validate_optional_hex_color("surface2", self.surface2.as_deref())?;
        validate_optional_hex_color("surface1", self.surface1.as_deref())?;
        validate_optional_hex_color("surface0", self.surface0.as_deref())?;

        for (name, value) in &self.extras {
            validate_hex_color(&format!("extras.{name}"), value)?;
        }

        Ok(())
    }

    /// Resolve a semantic color role to a palette color (hex string).
    /// Maps semantic roles to ANSI slots and semantic colors.
    pub fn resolve(&self, role: crate::cli::picker::preview_panel::SemanticColor) -> String {
        use crate::cli::picker::preview_panel::SemanticColor;

        match role {
            // Git-related
            SemanticColor::GitBranch => self.blue.clone(),
            SemanticColor::GitAdded => self.green.clone(),
            SemanticColor::GitModified => self.yellow.clone(),
            SemanticColor::GitUntracked => self.red.clone(),

            // File system
            SemanticColor::Directory => self.cyan.clone(),
            SemanticColor::FileExec => self.green.clone(),
            SemanticColor::FileSymlink => self.magenta.clone(),
            SemanticColor::FileDir => self.cyan.clone(),

            // Prompt & interaction
            SemanticColor::Prompt => self.blue.clone(),
            SemanticColor::Accent => self.cyan.clone(),
            SemanticColor::Error => self.red.clone(),
            SemanticColor::Muted => self.bright_black.clone(),

            // Starship/shell specific
            SemanticColor::Success => self.green.clone(),
            SemanticColor::Warning => self.yellow.clone(),
            SemanticColor::Failed => self.red.clone(),
            SemanticColor::Status => self.cyan.clone(),

            // Text levels
            SemanticColor::Text => self.foreground.clone(),
            SemanticColor::Subtext => self.white.clone(),
        }
    }
}

/// Per-tool theme references.
/// ToolRefs is now a HashMap<String, String> type alias, enabling new adapters to be added
/// without modifying the core type definition (Open/Closed principle).
///
/// Each tool uses different naming convention.
/// Example:
/// Ghostty: "Catppuccin Mocha" (Title Case with spaces)
/// Alacritty: "catppuccin_mocha" (snake_case)
/// bat: "Catppuccin Mocha" (Title Case)
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
    pub id: String,     // Unique identifier (e.g., "catppuccin-mocha") — D-30: kebab-case
    pub name: String,   // Display name (e.g., "Catppuccin Mocha")
    pub family: String, // Family (e.g., "Catppuccin")
    pub tool_refs: ToolRefs, // D-32: Now HashMap<String, String>
    pub palette: Palette, // D-28: Raw colors for tools without built-in support
    pub appearance: ThemeAppearance, // Per D-19: Dark or Light classification
    pub auto_pair: Option<String>, // Per D-19: Preferred paired variant ID, if applicable
}

impl ThemeVariant {
    /// Validate theme variant.
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(SlateError::InvalidThemeData(
                "theme id must not be empty".to_string(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(SlateError::InvalidThemeData(format!(
                "theme '{}' is missing a display name",
                self.id
            )));
        }
        if self.family.trim().is_empty() {
            return Err(SlateError::InvalidThemeData(format!(
                "theme '{}' is missing a family name",
                self.id
            )));
        }

        for key in REQUIRED_TOOL_REFS {
            let Some(value) = self.tool_refs.get(*key) else {
                return Err(SlateError::InvalidThemeData(format!(
                    "theme '{}' is missing required tool_ref '{}'",
                    self.id, key
                )));
            };
            if value.trim().is_empty() {
                return Err(SlateError::InvalidThemeData(format!(
                    "theme '{}' has empty tool_ref '{}'",
                    self.id, key
                )));
            }
        }

        self.palette.validate()?;
        Ok(())
    }
}

#[derive(Debug)]
struct EmbeddedThemes {
    ordered: Vec<ThemeVariant>,
    index_by_id: HashMap<String, usize>,
}

impl EmbeddedThemes {
    fn get(&self, id: &str) -> Option<&ThemeVariant> {
        self.index_by_id.get(id).map(|index| &self.ordered[*index])
    }

    fn all(&self) -> impl Iterator<Item = &ThemeVariant> {
        self.ordered.iter()
    }
}

/// Theme loader and registry.
/// Embedded in binary; loads and validates all variants once per process.
pub struct ThemeRegistry {
    embedded: &'static EmbeddedThemes,
}

#[derive(Debug, Deserialize)]
struct ThemeFile {
    theme: Vec<ThemeVariant>,
}

fn validate_hex_color(field: &str, value: &str) -> Result<()> {
    if value.len() != 7
        || !value.starts_with('#')
        || !value[1..].chars().all(|ch| ch.is_ascii_hexdigit())
    {
        return Err(SlateError::InvalidThemeData(format!(
            "invalid hex color for {field}: '{value}'"
        )));
    }
    Ok(())
}

fn validate_optional_hex_color(field: &str, value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        validate_hex_color(field, value)?;
    }
    Ok(())
}

fn validate_auto_pair(
    theme: &ThemeVariant,
    themes: &[ThemeVariant],
    ids: &HashMap<String, usize>,
) -> Result<()> {
    let Some(pair_id) = theme.auto_pair.as_deref() else {
        return Ok(());
    };

    let Some(pair_index) = ids.get(pair_id) else {
        return Err(SlateError::InvalidThemeData(format!(
            "theme '{}' references missing auto_pair '{}'",
            theme.id, pair_id
        )));
    };
    let pair = &themes[*pair_index];

    if pair_id != theme.id && pair.appearance == theme.appearance {
        return Err(SlateError::InvalidThemeData(format!(
            "theme '{}' auto_pair '{}' must switch to the opposite appearance",
            theme.id, pair_id
        )));
    }

    if pair_id == theme.id {
        let has_opposite_in_family = themes.iter().any(|candidate| {
            candidate.family == theme.family
                && candidate.id != theme.id
                && candidate.appearance != theme.appearance
        });
        if has_opposite_in_family {
            return Err(SlateError::InvalidThemeData(format!(
                "theme '{}' self-pairs even though family '{}' has an opposite appearance variant",
                theme.id, theme.family
            )));
        }
    }

    Ok(())
}

fn load_embedded_themes() -> Result<EmbeddedThemes> {
    let file: ThemeFile = toml::from_str(EMBEDDED_THEMES_TOML).map_err(|error| {
        SlateError::InvalidThemeData(format!("failed to parse embedded theme TOML: {error}"))
    })?;

    if file.theme.is_empty() {
        return Err(SlateError::InvalidThemeData(
            "embedded theme TOML did not contain any themes".to_string(),
        ));
    }

    let mut seen_ids = HashSet::new();
    let mut index_by_id = HashMap::new();
    for (index, theme) in file.theme.iter().enumerate() {
        theme.validate()?;
        if !seen_ids.insert(theme.id.clone()) {
            return Err(SlateError::InvalidThemeData(format!(
                "duplicate theme id '{}' in embedded theme TOML",
                theme.id
            )));
        }
        index_by_id.insert(theme.id.clone(), index);
    }

    for theme in &file.theme {
        validate_auto_pair(theme, &file.theme, &index_by_id)?;
    }

    Ok(EmbeddedThemes {
        ordered: file.theme,
        index_by_id,
    })
}

fn embedded_themes() -> Result<&'static EmbeddedThemes> {
    match EMBEDDED_THEMES.get_or_init(|| load_embedded_themes().map_err(|error| error.to_string()))
    {
        Ok(themes) => Ok(themes),
        Err(message) => Err(SlateError::InvalidThemeData(message.clone())),
    }
}

pub(crate) fn load_theme(theme_id: &str) -> Result<ThemeVariant> {
    embedded_themes()?
        .get(theme_id)
        .cloned()
        .ok_or_else(|| SlateError::InvalidThemeData(format!("theme '{theme_id}' not found")))
}

impl ThemeRegistry {
    /// Create registry with all embedded themes.
    pub fn new() -> Result<Self> {
        Ok(Self {
            embedded: embedded_themes()?,
        })
    }

    /// Get theme variant by ID.
    pub fn get(&self, id: &str) -> Option<&ThemeVariant> {
        self.embedded.get(id)
    }

    /// Get all theme variants in their TOML order.
    pub fn all(&self) -> Vec<&ThemeVariant> {
        self.embedded.all().collect()
    }

    /// List all theme IDs in their TOML order.
    pub fn list_ids(&self) -> Vec<String> {
        self.embedded.all().map(|theme| theme.id.clone()).collect()
    }

    /// Get themes grouped by family.
    pub fn by_family(&self) -> HashMap<String, Vec<&ThemeVariant>> {
        let mut families = HashMap::new();
        for variant in self.embedded.all() {
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
        "rose-pine-main" => Some("Dark, cozy & romantic. Love-inspired palette."),
        "rose-pine-moon" => Some("Dark, moodier variant. Deep forest nights."),
        "rose-pine-dawn" => Some("Light, warm & inviting. Sunrise through pines."),
        "kanagawa-wave" => Some("Dark, Japanese ukiyo-e aesthetic. Calm waves."),
        "kanagawa-dragon" => Some("Dark, deeper variant. Mountain mist & shadow."),
        "kanagawa-lotus" => Some("Light, serene & elegant. Lotus pond reflection."),
        "everforest-dark" => Some("Dark, nature-inspired. Forest-friendly alternative to Gruvbox."),
        "everforest-light" => Some("Light, earthy & warm. Sunlit forest floor."),
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

    fn sample_palette() -> Palette {
        Palette {
            foreground: "#ffffff".to_string(),
            background: "#000000".to_string(),
            cursor: Some("#ffffff".to_string()),
            selection_bg: Some("#222222".to_string()),
            selection_fg: Some("#eeeeee".to_string()),
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
        }
    }

    #[test]
    fn test_palette_validation_accepts_valid_hex_colors() {
        assert!(sample_palette().validate().is_ok());
    }

    #[test]
    fn test_palette_validation_rejects_invalid_hex_colors() {
        let mut palette = sample_palette();
        palette.surface2 = Some("#98989 2".to_string());
        let err = palette.validate().expect_err("invalid color should fail");
        assert!(err.to_string().contains("surface2"));
    }

    #[test]
    fn test_theme_registry_reuses_cached_embedded_data() {
        let first = ThemeRegistry::new().expect("first registry constructs");
        let second = ThemeRegistry::new().expect("second registry constructs");
        let first_theme = first.get(DEFAULT_THEME_ID).expect("default theme exists");
        let second_theme = second.get(DEFAULT_THEME_ID).expect("default theme exists");
        assert!(std::ptr::eq(first_theme, second_theme));
    }

    #[test]
    fn test_embedded_themes_have_unique_ids_and_valid_pairs() {
        let registry = ThemeRegistry::new().expect("registry constructs");
        let mut ids = HashSet::new();
        for theme in registry.all() {
            assert!(
                ids.insert(theme.id.clone()),
                "duplicate theme id: {}",
                theme.id
            );
            if let Some(pair_id) = theme.auto_pair.as_deref() {
                assert!(
                    registry.get(pair_id).is_some(),
                    "missing auto_pair target for {}",
                    theme.id
                );
            }
        }
    }

    #[test]
    fn test_tool_refs_lookup() {
        let registry = ThemeRegistry::new().expect("registry constructs");
        let theme = registry
            .get(DEFAULT_THEME_ID)
            .expect("default theme exists");
        for key in REQUIRED_TOOL_REFS {
            assert!(theme.tool_refs.contains_key(*key), "missing tool ref {key}");
        }
        assert_eq!(
            theme.tool_refs.get("ghostty").map(String::as_str),
            Some("Catppuccin Mocha")
        );
        assert_eq!(
            theme.tool_refs.get("bat").map(String::as_str),
            Some("Catppuccin Mocha")
        );
        assert_eq!(theme.tool_refs.get("unknown"), None);
    }

    /// Regression guard for the Ghostty theme-name naming convention.
    ///
    /// Ghostty ships built-in themes under specific names — slate's ghostty
    /// tool_ref strings must match those names exactly or Ghostty raises
    /// `theme "X" not found` at reload time. This caught the tokyo-night
    /// mismatch where slate was writing `"Tokyo Night Light"`/`"Tokyo Night"`
    /// but Ghostty ships them as `"TokyoNight Day"`/`"TokyoNight"`.
    ///
    /// The expected values below were captured from
    /// `ghostty +list-themes` on Ghostty 1.3.1. When Ghostty adds or renames
    /// built-ins, update this table alongside the corresponding theme file.
    #[test]
    fn test_ghostty_tool_refs_match_builtin_theme_names() {
        let registry = ThemeRegistry::new().expect("registry constructs");
        let expected: &[(&str, &str)] = &[
            ("catppuccin-latte", "Catppuccin Latte"),
            ("catppuccin-frappe", "Catppuccin Frappe"),
            ("catppuccin-macchiato", "Catppuccin Macchiato"),
            ("catppuccin-mocha", "Catppuccin Mocha"),
            ("tokyo-night-light", "TokyoNight Day"),
            ("tokyo-night-dark", "TokyoNight"),
            ("rose-pine-main", "Rose Pine"),
            ("rose-pine-moon", "Rose Pine Moon"),
            ("rose-pine-dawn", "Rose Pine Dawn"),
            ("kanagawa-wave", "Kanagawa Wave"),
            ("kanagawa-dragon", "Kanagawa Dragon"),
            ("kanagawa-lotus", "Kanagawa Lotus"),
            ("everforest-dark", "Everforest Dark Hard"),
            ("everforest-light", "Everforest Light Med"),
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
                "ghostty tool_ref for '{}' does not match Ghostty's built-in theme name",
                theme_id
            );
        }
    }

    /// Cross-check ghostty tool_refs against the real Ghostty themes directory.
    ///
    /// Unlike the hardcoded table above (which can drift silently if both the
    /// theme file and the table are updated with the same wrong string), this
    /// test reads the actual files Ghostty ships and fails if any theme_ref
    /// cannot be resolved. Only runs on macOS when Ghostty.app is installed;
    /// otherwise it is a no-op (CI without Ghostty still passes).
    #[test]
    #[cfg(target_os = "macos")]
    fn test_ghostty_tool_refs_exist_in_installed_ghostty() {
        let themes_dir =
            std::path::PathBuf::from("/Applications/Ghostty.app/Contents/Resources/ghostty/themes");
        if !themes_dir.exists() {
            eprintln!("skipping: Ghostty not installed at /Applications/Ghostty.app");
            return;
        }

        let registry = ThemeRegistry::new().expect("registry constructs");
        let mut missing: Vec<(String, String)> = Vec::new();
        for theme in registry.all() {
            let Some(name) = theme.tool_refs.get("ghostty") else {
                continue;
            };
            if !themes_dir.join(name).exists() {
                missing.push((theme.id.clone(), name.clone()));
            }
        }

        assert!(
            missing.is_empty(),
            "ghostty tool_refs reference themes missing in {:?}; either Ghostty renamed them or slate theme refs are wrong. Fix: {:?}",
            themes_dir,
            missing
        );
    }
}
