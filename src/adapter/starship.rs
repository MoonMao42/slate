//! Starship adapter with scoped [palettes.slate] editing.
//! Explicit exception to managed-first — Starship has no documented
//! include/import mechanism, so uses EditInPlace strategy to modify user's
//! starship.toml in-place with careful scoping to [palettes.slate] section.

use crate::adapter::{ApplyStrategy, ToolAdapter};
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::fs;
use std::path::PathBuf;
use toml_edit::DocumentMut;
use which::which;

/// Starship adapter implementing v2 ToolAdapter trait.
pub struct StarshipAdapter;

impl StarshipAdapter {
    /// Pure path resolution: env override → XDG default (no global state)
    fn resolve_path(starship_config: Option<&str>, config_home: &PathBuf) -> PathBuf {
        if let Some(val) = starship_config {
            if !val.is_empty() {
                return PathBuf::from(val);
            }
        }
        config_home.join("starship.toml")
    }
}

impl ToolAdapter for StarshipAdapter {
    fn tool_name(&self) -> &'static str {
        "starship"
    }

    fn is_installed(&self) -> Result<bool> {
        let binary_exists = which("starship").is_ok();

        let config_exists = match self.integration_config_path() {
            Ok(path) => path.exists(),
            Err(_) => false,
        };

        Ok(binary_exists || config_exists)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        let home = env.home().to_str().ok_or(SlateError::MissingHomeDir)?;
        let config_home = PathBuf::from(home).join(".config");
        Ok(Self::resolve_path(
            std::env::var("STARSHIP_CONFIG").ok().as_deref(),
            &config_home,
        ))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        let home = env
            .as_ref()
            .and_then(|e| e.home().to_str().map(|s| s.to_string()));
        if let Some(h) = home {
            PathBuf::from(h).join(".config/slate/managed/starship")
        } else {
            PathBuf::from(".config/slate/managed/starship")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EditInPlace
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<()> {
        let config_path = self.integration_config_path()?;

        // Step 0: Backup before any modification
        let config_manager = ConfigManager::new()?;
        let _backup_path = config_manager.backup_file(&config_path)?;

        // Step 1: Read and parse TOML (preserves comments via toml_edit)
        let content = fs::read_to_string(&config_path).map_err(|e| {
            SlateError::ConfigReadError(config_path.display().to_string(), e.to_string())
        })?;

        let mut doc: DocumentMut = content.parse().map_err(SlateError::TomlParseError)?;

        // Step 2: Set palette = "slate" at root level
        doc["palette"] = toml_edit::value("slate");

        // Step 3: Ensure [palettes.slate] table exists
        if doc.get("palettes").is_none() {
            doc["palettes"] = toml_edit::Item::Table(toml_edit::Table::new());
        }

        if let Some(palettes) = doc["palettes"].as_table_mut() {
            let mut sp = toml_edit::Table::new();
            let p = &theme.palette;

            // Helper: pick first available Option<String>, or fallback
            fn pick(opts: &[&Option<String>], fallback: &str) -> String {
                opts.iter()
                    .filter_map(|o| o.as_ref())
                    .next()
                    .cloned()
                    .unwrap_or_else(|| fallback.to_string())
            }

            // Core ANSI — always available
            sp["red"] = toml_edit::value(p.red.as_str());
            sp["yellow"] = toml_edit::value(p.yellow.as_str());
            sp["green"] = toml_edit::value(p.green.as_str());
            sp["blue"] = toml_edit::value(p.blue.as_str());
            sp["white"] = toml_edit::value(p.white.as_str());
            sp["foreground"] = toml_edit::value(p.foreground.as_str());
            sp["background"] = toml_edit::value(p.background.as_str());
            sp["text"] = toml_edit::value(p.text.as_deref().unwrap_or(&p.foreground));

            // Starship segment colors — must be 6 visually distinct values.
            // peach (warm accent between red and yellow segments): must differ from both.
            let peach_candidates: Vec<&str> = [
                p.extras.get("peach"),
                p.extras.get("orange"),
                p.extras.get("rose"),
            ].iter().filter_map(|o| o.map(|s| s.as_str())).collect();

            let peach = peach_candidates.iter()
                .find(|c| **c != p.red && **c != p.yellow)
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    if p.bright_red != p.red && p.bright_red != p.yellow { p.bright_red.clone() }
                    else if p.bright_yellow != p.yellow && p.bright_yellow != p.red { p.bright_yellow.clone() }
                    else { p.magenta.clone() }
                });
            sp["peach"] = toml_edit::value(peach.as_str());

            // sapphire (cool accent): sapphire → foam → bright_blue (if ≠ blue) → cyan
            let sapphire = p.extras.get("sapphire")
                .or(p.extras.get("foam"))
                .cloned()
                .unwrap_or_else(|| {
                    if p.bright_blue != p.blue { p.bright_blue.clone() }
                    else { p.cyan.clone() }
                });
            sp["sapphire"] = toml_edit::value(sapphire.as_str());

            // lavender (purple accent): lavender → iris → mauve → bright_magenta → magenta
            let lavender = p.lavender.clone()
                .or_else(|| p.extras.get("lavender").cloned())
                .or_else(|| p.extras.get("iris").cloned())
                .unwrap_or_else(|| pick(&[&p.mauve, &Some(p.bright_magenta.clone())], &p.magenta));
            sp["lavender"] = toml_edit::value(lavender.as_str());

            // Secondary palette names used by some starship configs
            sp["teal"] = toml_edit::value(p.cyan.as_str());
            sp["maroon"] = toml_edit::value(p.extras.get("maroon")
                .unwrap_or(&p.bright_red).as_str());
            sp["sky"] = toml_edit::value(p.bright_cyan.as_str());
            sp["pink"] = toml_edit::value(p.pink.as_deref()
                .or(p.extras.get("pink").map(|s| s.as_str()))
                .unwrap_or(&p.bright_magenta));

            // crust: semantic darkest background
            sp["crust"] = toml_edit::value(p.bg_darkest.as_deref().unwrap_or(&p.black));

            // powerline_fg: adaptive high-contrast foreground for segment text
            let powerline_fg = if theme.appearance == crate::theme::ThemeAppearance::Light {
                &p.foreground
            } else {
                p.bg_darkest.as_ref().unwrap_or(&p.black)
            };
            sp["powerline_fg"] = toml_edit::value(powerline_fg.as_str());

            palettes["slate"] = toml_edit::Item::Table(sp);
        }

        // Step 3b: Replace fg:crust with fg:powerline_fg in format strings
        // so powerline segments use the adaptive contrast color.
        let new_content = doc.to_string().replace("fg:crust", "fg:powerline_fg");
        use atomic_write_file::AtomicWriteFile;
        use std::io::Write;

        let mut file = AtomicWriteFile::open(&config_path).map_err(|e| {
            SlateError::ConfigWriteError(config_path.display().to_string(), e.to_string())
        })?;

        file.write_all(new_content.as_bytes()).map_err(|e| {
            SlateError::ConfigWriteError(config_path.display().to_string(), e.to_string())
        })?;

        file.commit().map_err(|e| {
            SlateError::ConfigWriteError(config_path.display().to_string(), e.to_string())
        })?;

        Ok(())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let adapter = StarshipAdapter;
        assert_eq!(adapter.tool_name(), "starship");
    }

    #[test]
    fn test_apply_strategy_returns_edit_in_place() {
        let adapter = StarshipAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EditInPlace);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = StarshipAdapter;
        let path = adapter.managed_config_path();
        assert!(path
            .to_string_lossy()
            .contains(".config/slate/managed/starship"));
    }

    #[test]
    fn test_resolve_path_with_env_override() {
        let config_home = PathBuf::from("/home/user/.config");
        let path = StarshipAdapter::resolve_path(Some("/custom/starship.toml"), &config_home);
        assert_eq!(path, PathBuf::from("/custom/starship.toml"));
    }

    #[test]
    fn test_resolve_path_empty_env_uses_default() {
        let config_home = PathBuf::from("/home/user/.config");
        let path = StarshipAdapter::resolve_path(Some(""), &config_home);
        assert_eq!(path, PathBuf::from("/home/user/.config/starship.toml"));
    }

    #[test]
    fn test_resolve_path_default_xdg() {
        let config_home = PathBuf::from("/home/user/.config");
        let path = StarshipAdapter::resolve_path(None, &config_home);
        assert_eq!(path, PathBuf::from("/home/user/.config/starship.toml"));
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = StarshipAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
