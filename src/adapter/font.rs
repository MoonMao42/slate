//! Nerd Font adapter for font detection and installation support.
//! Per through Detects installed Nerd Fonts on macOS and provides
//! installation command mapping. Scope: detect + install mapping only (no config writing).

use crate::adapter::{ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// Pure data structure for aggregated font discovery 
pub struct FontDiscovery {
    pub nerd_fonts: Vec<String>,
    pub system_fonts: Vec<String>,
}

/// Nerd Font adapter implementing v2 ToolAdapter trait.
pub struct FontAdapter;

impl FontAdapter {
    const CANONICAL_SUFFIXES: [(&'static str, &'static str); 8] = [
        ("Nerd Font Complete Mono", " Nerd Font Mono"),
        ("Nerd Font Complete", " Nerd Font"),
        ("NerdFontMono", " Nerd Font Mono"),
        ("Nerd Font Mono", " Nerd Font Mono"),
        ("NerdFontPropo", " Nerd Font Propo"),
        ("Nerd Font Propo", " Nerd Font Propo"),
        ("NerdFont", " Nerd Font"),
        ("Nerd Font", " Nerd Font"),
    ];

    /// Get home directory from process environment
    fn home() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(env.home().to_path_buf())
    }

    fn looks_like_nerd_font(name: &str) -> bool {
        name.contains("NerdFont") || name.contains("Nerd Font")
    }

    /// Normalize a font filename into the family name terminal configs expect.
    /// Example: "JetBrainsMonoNerdFont-Regular.ttf" -> "JetBrainsMono Nerd Font"
    pub(crate) fn normalize_font_family(name: &str) -> String {
        let stem = name
            .rsplit_once('.')
            .map(|(value, _)| value)
            .unwrap_or(name)
            .trim();
        let family_candidate = stem.split('-').next().unwrap_or(stem).trim();

        for (suffix, canonical_suffix) in Self::CANONICAL_SUFFIXES {
            if let Some(prefix) = family_candidate.strip_suffix(suffix) {
                let prefix = prefix.trim();
                return format!("{}{}", prefix, canonical_suffix);
            }
        }

        family_candidate.to_string()
    }

    /// Collapse spacing/punctuation so display names and filesystem family names
    /// can be compared safely.
    pub(crate) fn family_match_key(name: &str) -> String {
        name.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    }

    /// Detect installed Nerd Fonts by scanning font directories.
    /// Returns canonical family names suitable for terminal config files.
    /// JetBrainsMono Nerd Font is marked as recommended and placed first.
    pub fn detect_installed_fonts() -> Result<Vec<String>> {
        let env = SlateEnv::from_process()?;
        Self::detect_installed_fonts_with_env(&env)
    }

    /// Detect installed Nerd Fonts with injected SlateEnv (for testing)
    pub fn detect_installed_fonts_with_env(env: &SlateEnv) -> Result<Vec<String>> {
        let mut fonts = BTreeSet::new();

        // Scan user fonts directory
        if let Ok(user_fonts) = fs::read_dir(env.home().join("Library/Fonts")) {
            for entry in user_fonts.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if Self::looks_like_nerd_font(&name) {
                        fonts.insert(Self::normalize_font_family(&name));
                    }
                }
            }
        }

        // Scan system fonts directory
        if let Ok(sys_fonts) = fs::read_dir("/Library/Fonts") {
            for entry in sys_fonts.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if Self::looks_like_nerd_font(&name) {
                        fonts.insert(Self::normalize_font_family(&name));
                    }
                }
            }
        }

        // Convert to Vec and reorder with recommendation first 
        let mut fonts_vec: Vec<String> = fonts.into_iter().collect();
        Self::apply_recommendation_ordering(&mut fonts_vec);
        Ok(fonts_vec)
    }

    /// Apply recommendation ordering: JetBrainsMono Nerd Font first (if installed),
    /// then all others alphabetically. Per.
    fn apply_recommendation_ordering(fonts: &mut Vec<String>) {
        const RECOMMENDED: &str = "JetBrainsMono Nerd Font";

        // Find and move recommended font to front (if present)
        if let Some(pos) = fonts.iter().position(|f| f == RECOMMENDED) {
            fonts.remove(pos);
            fonts.insert(0, RECOMMENDED.to_string());
        } else {
            // If recommended font not installed, add it at the front with note
            fonts.insert(0, format!("{} (not installed)", RECOMMENDED));
        }

        // Keep rest alphabetically sorted
        fonts[1..].sort();
    }

    /// Helper: Check if filename is a font file (.ttf, .otf, or .ttc per)
    fn is_font_file(name: &str) -> bool {
        name.ends_with(".ttf") || name.ends_with(".otf") || name.ends_with(".ttc")
    }

    /// Detect only installed Nerd Fonts (pure data, no UI markers).
    /// Returns real, verified Nerd Fonts found in font directories.
    /// No "(not installed)" placeholders or UI badges — pure detection only.
    pub fn detect_installed_nerd_fonts() -> Result<Vec<String>> {
        let env = SlateEnv::from_process()?;
        Self::detect_installed_nerd_fonts_with_env(&env)
    }

    /// Detect installed Nerd Fonts with injected SlateEnv (for testing).
    /// Returns pure list of verified Nerd Fonts.
    pub fn detect_installed_nerd_fonts_with_env(env: &SlateEnv) -> Result<Vec<String>> {
        let mut fonts = BTreeSet::new();

        // Scan font directories (extended paths)
        let scan_paths = vec![
            env.home().join("Library/Fonts"),
            PathBuf::from("/Library/Fonts"),
            PathBuf::from("/System/Library/Fonts"),
            PathBuf::from("/System/Applications/Utilities/Terminal.app/Contents/Resources/Fonts"),
        ];

        for path in scan_paths {
            if let Ok(entries) = fs::read_dir(&path) {
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        // Add .ttc extension support
                        if Self::is_font_file(&name) && Self::looks_like_nerd_font(&name) {
                            fonts.insert(Self::normalize_font_family(&name));
                        }
                    }
                }
            }
        }

        // Return as sorted Vec, no markers or placeholders (pure data)
        let mut fonts_vec: Vec<String> = fonts.into_iter().collect();
        fonts_vec.sort();
        Ok(fonts_vec)
    }

    /// Detect available system fonts from macOS whitelist (pure data, no UI markers).
    /// Returns only Monaco, Menlo, SF Mono if found.
    pub fn detect_available_system_fonts() -> Result<Vec<String>> {
        let env = SlateEnv::from_process()?;
        Self::detect_available_system_fonts_with_env(&env)
    }

    /// Detect system fonts with injected SlateEnv (for testing).
    /// Whitelist match only (Monaco, Menlo, SF Mono).
    pub fn detect_available_system_fonts_with_env(env: &SlateEnv) -> Result<Vec<String>> {
        let whitelist = ["Monaco", "Menlo", "SF Mono"];
        let mut fonts = BTreeSet::new();

        // Scan same paths as Nerd Font detection
        let scan_paths = vec![
            env.home().join("Library/Fonts"),
            PathBuf::from("/Library/Fonts"),
            PathBuf::from("/System/Library/Fonts"),
            PathBuf::from("/System/Applications/Utilities/Terminal.app/Contents/Resources/Fonts"),
        ];

        for path in scan_paths {
            if let Ok(entries) = fs::read_dir(&path) {
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        // Check if file is a font file (include .ttc)
                        if Self::is_font_file(&name) {
                            let family = Self::normalize_font_family(&name);
                            // Match against whitelist using canonical key
                            for candidate in &whitelist {
                                if Self::family_match_key(&family)
                                    == Self::family_match_key(candidate)
                                {
                                    fonts.insert(family);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Return as sorted Vec, no markers or placeholders (pure data)
        let mut fonts_vec: Vec<String> = fonts.into_iter().collect();
        fonts_vec.sort();
        Ok(fonts_vec)
    }

    /// Aggregation method: Returns both nerd and system fonts grouped.
    /// Convenience struct for picker assembly layer.
    pub fn discover_all_fonts() -> Result<FontDiscovery> {
        let env = SlateEnv::from_process()?;
        let nerd_fonts = Self::detect_installed_nerd_fonts_with_env(&env)?;
        let system_fonts = Self::detect_available_system_fonts_with_env(&env)?;
        Ok(FontDiscovery {
            nerd_fonts,
            system_fonts,
        })
    }

    /// Persist chosen font to config and apply to terminal adapters.
    /// Updates current-font, Ghostty font-family, and Alacritty [font.normal] family.
    pub fn apply_font(env: &SlateEnv, font_name: &str) -> Result<()> {
        let config = ConfigManager::with_env(env)?;

        // Persist to current-font file
        config.set_current_font(font_name)?;

        // Write to Ghostty managed config
        let font_conf_content = format!("font-family = \"{}\"\n", font_name);
        config.write_managed_file("ghostty", "font.conf", &font_conf_content)?;

        // Re-apply current theme so all adapters (Alacritty, etc.) pick up
        // the new font from current-font when they regenerate their configs.
        config.refresh_shell_integration()?;
        let current_theme_id = config
            .get_current_theme()?
            .unwrap_or_else(|| "catppuccin-mocha".to_string());
        let registry = crate::theme::ThemeRegistry::new()?;
        if let Some(theme) = registry.get(&current_theme_id) {
            crate::cli::setup_executor::apply_theme_selection(theme)?;
        }

        Ok(())
    }

    /// Map font name to brew cask name
    /// Example: "JetBrains Mono Nerd Font" -> "font-jetbrains-mono-nerd-font"
    pub fn font_to_cask_name(font_name: &str) -> String {
        // Remove "Nerd Font" suffix if present
        let base_name = font_name
            .strip_suffix(" Nerd Font")
            .unwrap_or(font_name)
            .trim();

        // Convert to kebab-case
        let kebab = base_name.to_lowercase().replace(" ", "-").replace("_", "-");

        // Ensure font- prefix and nerd-font suffix
        let cask_name = if kebab.starts_with("font-") {
            kebab
        } else {
            format!("font-{}", kebab)
        };

        if cask_name.ends_with("-nerd-font") {
            cask_name
        } else {
            format!("{}-nerd-font", cask_name)
        }
    }
}

impl ToolAdapter for FontAdapter {
    fn tool_name(&self) -> &'static str {
        "nerd-font"
    }

    fn is_installed(&self) -> Result<bool> {
        // Use nerd-only check per /
        match Self::detect_installed_nerd_fonts() {
            Ok(fonts) => Ok(!fonts.is_empty()),
            Err(_) => {
                // Gracefully handle permission errors
                Ok(false)
            }
        }
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let home = Self::home()?;
        Ok(home.join("Library/Fonts"))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().expect("Failed to read environment");
        env.config_dir().to_path_buf()
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::DetectAndInstall
    }

    fn apply_theme(&self, _theme: &ThemeVariant) -> Result<()> {
        // Per design: Nerd Font adapter only handles detection and installation
        // No theme application needed (fonts are tool-independent)
        Ok(())
    }

    fn reload(&self) -> Result<()> {
        // Fonts don't need reload
        Ok(())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // Return name of first installed Nerd Font, if any
        match Self::detect_installed_nerd_fonts() {
            Ok(fonts) => Ok(fonts.first().cloned()),
            Err(_) => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = FontAdapter;
        assert_eq!(adapter.tool_name(), "nerd-font");
    }

    #[test]
    fn test_apply_strategy_returns_detect_and_install() {
        let adapter = FontAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::DetectAndInstall);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = FontAdapter;
        let path = adapter.managed_config_path();
        assert!(path.to_string_lossy().contains(".config/slate"));
    }

    #[test]
    fn test_integration_config_path_returns_fonts_dir() {
        let adapter = FontAdapter;
        let result = adapter.integration_config_path();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().contains("Library/Fonts"));
    }

    #[test]
    fn test_apply_theme_returns_ok() {
        let adapter = FontAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let result = adapter.apply_theme(&theme);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reload_returns_ok() {
        let adapter = FontAdapter;
        let result = adapter.reload();
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_current_theme_returns_option() {
        let adapter = FontAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        // Result may be None or Some depending on installed fonts
    }

    #[test]
    fn test_font_to_cask_name_jetbrains_mono() {
        let cask = FontAdapter::font_to_cask_name("JetBrains Mono Nerd Font");
        assert_eq!(cask, "font-jetbrains-mono-nerd-font");
    }

    #[test]
    fn test_font_to_cask_name_fira_code() {
        let cask = FontAdapter::font_to_cask_name("Fira Code Nerd Font");
        assert_eq!(cask, "font-fira-code-nerd-font");
    }

    #[test]
    fn test_font_to_cask_name_iosevka() {
        let cask = FontAdapter::font_to_cask_name("Iosevka Term Nerd Font");
        assert_eq!(cask, "font-iosevka-term-nerd-font");
    }

    #[test]
    fn test_font_to_cask_name_hack() {
        let cask = FontAdapter::font_to_cask_name("Hack Nerd Font");
        assert_eq!(cask, "font-hack-nerd-font");
    }

    #[test]
    fn test_is_installed_returns_result() {
        let adapter = FontAdapter;
        let result = adapter.is_installed();
        assert!(result.is_ok());
        // Result may be true or false depending on installed fonts
    }

    #[test]
    fn test_normalize_font_family_regular_file() {
        let family = FontAdapter::normalize_font_family("FiraCodeNerdFont-Regular.ttf");
        assert_eq!(family, "FiraCode Nerd Font");
    }

    #[test]
    fn test_normalize_font_family_mono_file() {
        let family = FontAdapter::normalize_font_family("FiraCodeNerdFontMono-SemiBold.ttf");
        assert_eq!(family, "FiraCode Nerd Font Mono");
    }

    #[test]
    fn test_normalize_font_family_preserves_base_name_shape() {
        let family =
            FontAdapter::normalize_font_family("JetBrainsMonoNerdFontPropo-ThinItalic.ttf");
        assert_eq!(family, "JetBrainsMono Nerd Font Propo");
    }

    #[test]
    fn test_family_match_key_ignores_spacing_differences() {
        let display = FontAdapter::family_match_key("JetBrains Mono Nerd Font");
        let detected = FontAdapter::family_match_key("JetBrainsMono Nerd Font");
        assert_eq!(display, detected);
    }

    #[test]
    fn test_detect_installed_fonts_with_env_uses_injected_home() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        // With empty tempdir, should return empty list (no fonts installed)
        let result = FontAdapter::detect_installed_fonts_with_env(&env);
        assert!(result.is_ok());
        // Result should be empty since no fonts exist in tempdir
    }

    #[test]
    fn test_recommendation_ordering_puts_jetbrains_first() {
        let mut fonts = vec![
            "Fira Code Nerd Font".to_string(),
            "JetBrainsMono Nerd Font".to_string(),
            "Iosevka Nerd Font".to_string(),
        ];
        FontAdapter::apply_recommendation_ordering(&mut fonts);
        assert_eq!(fonts[0], "JetBrainsMono Nerd Font");
        assert_eq!(fonts[1], "Fira Code Nerd Font");
        assert_eq!(fonts[2], "Iosevka Nerd Font");
    }

    #[test]
    fn test_recommendation_ordering_adds_not_installed_note() {
        let mut fonts = vec![
            "Fira Code Nerd Font".to_string(),
            "Iosevka Nerd Font".to_string(),
        ];
        FontAdapter::apply_recommendation_ordering(&mut fonts);
        assert_eq!(fonts[0], "JetBrainsMono Nerd Font (not installed)");
        assert_eq!(fonts[1], "Fira Code Nerd Font");
        assert_eq!(fonts[2], "Iosevka Nerd Font");
    }
}
