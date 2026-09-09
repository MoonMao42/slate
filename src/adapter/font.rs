//! Nerd Font adapter for font detection and installation support.
//! Detects installed Nerd Fonts across supported platforms and provides
//! installation mapping, plus prepared terminal/Shell font configuration changes.

use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::path::PathBuf;

mod change;
mod discovery;
pub(crate) mod references;
pub(crate) use change::{FontFileAction, PreparedFont};
pub use discovery::{FontScanIssue, FontScanReport};

/// Pure data structure for aggregated font discovery
#[derive(Debug, Default)]
pub struct FontDiscovery {
    pub nerd_fonts: Vec<String>,
    pub system_fonts: Vec<String>,
}

/// Nerd Font adapter implementing the ToolAdapter trait.
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

    fn looks_like_nerd_font(name: &str) -> bool {
        name.contains("NerdFont") || name.contains("Nerd Font")
    }

    pub fn is_nerd_font_name(name: &str) -> bool {
        Self::looks_like_nerd_font(name)
    }

    /// Normalize a font filename into the family name terminal configs expect.
    /// Example: "JetBrainsMonoNerdFont-Regular.ttf" -> "JetBrainsMono Nerd Font"
    pub(crate) fn normalize_font_family(name: &str) -> String {
        let stem = name
            .rsplit_once('.')
            .map(|(value, _)| value)
            .unwrap_or(name)
            .trim();
        for (suffix, canonical_suffix) in Self::CANONICAL_SUFFIXES {
            if let Some((prefix, style)) = stem.rsplit_once(suffix) {
                if !prefix.trim().is_empty() && (style.is_empty() || style.starts_with('-')) {
                    return format!("{}{}", prefix.trim(), canonical_suffix);
                }
            }
        }
        stem.rsplit_once('-')
            .map(|(family, _)| family)
            .unwrap_or(stem)
            .trim()
            .to_string()
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
        let mut fonts_vec = Self::detect_installed_nerd_fonts_with_env(env)?;
        Self::apply_recommendation_ordering(&mut fonts_vec);
        Ok(fonts_vec)
    }

    /// Apply recommendation ordering: JetBrainsMono Nerd Font first (if installed),
    /// then all others alphabetically.
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

    /// Detect only installed Nerd Fonts (pure data, no UI markers).
    /// Returns filename-derived candidates with a regular-file/header check.
    /// No "(not installed)" placeholders or UI badges — pure detection only.
    pub fn detect_installed_nerd_fonts() -> Result<Vec<String>> {
        let env = SlateEnv::from_process()?;
        Self::detect_installed_nerd_fonts_with_env(&env)
    }

    /// Detect installed Nerd Fonts with injected SlateEnv (for testing).
    /// Returns candidates, not proof of native registration or glyph coverage.
    pub fn detect_installed_nerd_fonts_with_env(env: &SlateEnv) -> Result<Vec<String>> {
        let report = Self::scan_fonts_with_env(env);
        report.require_complete()?;
        Ok(report.fonts.nerd_fonts)
    }

    /// Theme writers need real family names, never the UI's recommended
    /// "(not installed)" placeholder. Honor the caller's profile for discovery.
    pub(crate) fn preferred_installed_font_with_env(env: &SlateEnv) -> Result<Option<String>> {
        Ok(Self::preferred_installed_family(
            Self::detect_installed_nerd_fonts_with_env(env)?,
        ))
    }

    fn preferred_installed_family(fonts: Vec<String>) -> Option<String> {
        let mut first = None;
        for family in fonts {
            if super::font_config::validate_family(&family).is_err() {
                continue;
            }
            if family == "JetBrainsMono Nerd Font" {
                return Some(family);
            }
            if first.is_none() {
                first = Some(family);
            }
        }
        first
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
        let report = Self::scan_fonts_with_env(env);
        report.require_complete()?;
        Ok(report.fonts.system_fonts)
    }

    /// Aggregation method: Returns both nerd and system fonts grouped.
    /// Convenience struct for picker assembly layer.
    pub fn discover_all_fonts() -> Result<FontDiscovery> {
        let env = SlateEnv::from_process()?;
        let report = Self::scan_fonts_with_env(&env);
        report.require_complete()?;
        Ok(report.fonts)
    }

    /// One bounded pass supplies both UI groups and incomplete-scan evidence.
    pub fn scan_fonts_with_env(env: &SlateEnv) -> FontScanReport {
        discovery::scan(env)
    }

    /// Apply font to Ghostty, Alacritty and Kitty with localized refresh.
    /// Prepares every required file, writes outputs, then commits current-font.
    /// Refreshes shell integration but does not trigger full theme reapplication.
    pub fn apply_font(env: &SlateEnv, font_name: &str) -> Result<()> {
        super::font_config::validate_family(font_name)?;
        crate::config::recovery_paths::validate_storage_paths(env, "Font")?;
        let _guard = crate::config::ConfigWriteGuard::acquire(env)?;
        PreparedFont::capture(env, font_name)?.apply()
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
        Self::detect_installed_nerd_fonts().map(|fonts| !fonts.is_empty())
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(crate::platform::fonts::user_font_dir(&env))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().expect("Failed to read environment");
        env.config_dir().to_path_buf()
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::DetectAndInstall
    }

    fn apply_theme(&self, _theme: &ThemeVariant) -> Result<ApplyOutcome> {
        // Per design: Nerd Font adapter only handles detection and installation
        // No theme application needed (fonts are tool-independent).
        // Font availability is visible at next shell/terminal launch — the
        // font-family switch is not picked up by the currently-running
        // session.
        Ok(ApplyOutcome::applied_needs_new_shell())
    }

    fn reload(&self) -> Result<()> {
        // Fonts don't need reload
        Ok(())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // Return name of first installed Nerd Font, if any
        Self::detect_installed_nerd_fonts().map(|fonts| fonts.first().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_fallback_selects_real_families_without_inventing_a_recommendation() {
        assert_eq!(FontAdapter::preferred_installed_family(vec![]), None);
        assert_eq!(
            FontAdapter::preferred_installed_family(vec!["Bad\nNerd Font".into()]),
            None
        );
        assert_eq!(
            FontAdapter::preferred_installed_family(vec!["FiraCode Nerd Font".into()]),
            Some("FiraCode Nerd Font".into())
        );
        assert_eq!(
            FontAdapter::preferred_installed_family(vec![
                "FiraCode Nerd Font".into(),
                "JetBrainsMono Nerd Font".into()
            ]),
            Some("JetBrainsMono Nerd Font".into())
        );
    }

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
    fn font_paths_adapter_integration_config_uses_platform_environment() {
        let adapter = FontAdapter;
        let env = SlateEnv::from_process().unwrap();
        assert_eq!(
            adapter.integration_config_path().unwrap(),
            crate::platform::fonts::user_font_dir(&env)
        );
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
