//! fastfetch adapter with JSONC config generation and Apple logo theming.
//! Implements EnvironmentVariable strategy.
//! Generates managed JSONC config with themed colors while preserving Apple logo.

use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::path::PathBuf;

/// fastfetch adapter implementing the ToolAdapter trait.
pub struct FastfetchAdapter;

impl FastfetchAdapter {
    pub fn theme_path(env: &SlateEnv) -> PathBuf {
        env.managed_file("managed/fastfetch/config.jsonc")
    }
    /// Get config home directory (XDG default)
    fn config_home() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Ok(env.xdg_config_home().to_path_buf())
    }
}

impl ToolAdapter for FastfetchAdapter {
    fn tool_name(&self) -> &'static str {
        "fastfetch"
    }

    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        let config_home = Self::config_home()?;
        Ok(config_home.join("fastfetch/config.jsonc"))
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("fastfetch")
        } else {
            PathBuf::from(".config/slate/managed/fastfetch")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        // Fastfetch uses generated palette colors, not a native named theme.
        let managed_content = self.generate_jsonc_config(theme)?;

        // Write only after the palette has been validated.
        super::managed_fragment::write(
            env,
            &Self::theme_path(env),
            managed_content.as_bytes(),
            "fastfetch",
        )?;

        // fastfetch is invoked at shell startup via the managed wrapper;
        // updated colors are visible the next time a shell runs it.
        Ok(ApplyOutcome::applied_needs_new_shell())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

impl FastfetchAdapter {
    pub fn generate_jsonc_config(&self, theme: &ThemeVariant) -> Result<String> {
        use crate::adapter::palette_renderer::PaletteRenderer;
        use serde_json::json;

        let palette = &theme.palette;
        palette.validate()?;

        // Use subtext color for keys (muted), accent for separators (subtle pop)
        let key_hex = palette.subtext1.as_deref().unwrap_or(&palette.foreground);
        let (r_key, g_key, b_key) = PaletteRenderer::hex_to_rgb(key_hex)?;
        let (r_acc, g_acc, b_acc) = PaletteRenderer::hex_to_rgb(&palette.blue)?;
        let (r_fg, g_fg, b_fg) = PaletteRenderer::hex_to_rgb(&palette.foreground)?;

        let color_keys = format!("38;2;{};{};{}", r_key, g_key, b_key);
        let color_separator = format!("38;2;{};{};{}", r_acc, g_acc, b_acc);
        let color_output = format!("38;2;{};{};{}", r_fg, g_fg, b_fg);

        let config = json!({
            "$schema": "https://github.com/fastfetch-cli/fastfetch/raw/dev/doc/json_schema.json",
            "logo": {
                "type": "builtin",
                "source": if cfg!(target_os = "macos") { "apple_small" } else { "auto" },
                "padding": { "top": 1 }
            },
            "display": {
                "separator": " ",
                "color": {
                    "keys": color_keys,
                    "separator": color_separator,
                    "output": color_output
                }
            },
            "modules": [
                { "type": "title" },
                { "type": "separator" },
                { "type": "os" },
                { "type": "kernel" },
                { "type": "uptime" },
                { "type": "terminal" },
                { "type": "shell" },
                { "type": "cpu" },
                { "type": "memory" },
                { "type": "break" },
                { "type": "colors" }
            ]
        });

        Ok(serde_json::to_string_pretty(&config)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires explicit SLATE_FASTFETCH_BINARY; fixed-text native rendering only"]
    fn fastfetch_native_display_colors_match_all_palettes_without_system_modules() {
        use std::{fs, time::Duration};
        let binary = fs::canonicalize(
            std::env::var_os("SLATE_FASTFETCH_BINARY").expect("set native fastfetch explicitly"),
        )
        .unwrap();
        let home = tempfile::tempdir().unwrap();
        let config_path = home.path().join("preset.jsonc");
        let ansi = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
            let mut config: serde_json::Value =
                serde_json::from_str(&FastfetchAdapter.generate_jsonc_config(theme).unwrap())
                    .unwrap();
            // Keep the generated display object, but replace system probes with
            // a fixed custom module. This does not validate the system modules.
            config["modules"] = serde_json::json!([{"type": "custom", "key": "FixtureKey", "format": "FixtureValue"}]);
            for payload in [
                serde_json::to_vec(&config).unwrap(),
                format!(
                    "/* fixed fixture comment */\n{}\n// trailing comment\n",
                    serde_json::to_string_pretty(&config).unwrap()
                )
                .into_bytes(),
            ] {
                // Slate's wrapper selects a JSONC file. Fastfetch's stdin
                // parser uses a different (strict JSON) contract.
                fs::write(&config_path, payload).unwrap();
                let result = assert_cmd::Command::new(&binary)
                    .env_clear()
                    .env("HOME", home.path())
                    .env("TERM", "xterm-256color")
                    .current_dir(home.path())
                    .arg("--config")
                    .arg(&config_path)
                    .args(["--logo", "none", "--pipe", "false"])
                    .timeout(Duration::from_secs(5))
                    .assert()
                    .success()
                    .stderr("");
                let output = String::from_utf8_lossy(&result.get_output().stdout);
                assert!(
                    ansi.replace_all(&output, "")
                        .contains("FixtureKey FixtureValue"),
                    "{}: {output:?}",
                    theme.id
                );
                for (label, color) in [
                    (
                        "FixtureKey",
                        theme
                            .palette
                            .subtext1
                            .as_deref()
                            .unwrap_or(&theme.palette.foreground),
                    ),
                    ("FixtureValue", theme.palette.foreground.as_str()),
                    (" ", theme.palette.blue.as_str()),
                ] {
                    let (r, g, b) =
                        crate::adapter::palette_renderer::PaletteRenderer::hex_to_rgb(color)
                            .unwrap();
                    let pattern = format!(
                        r"\x1b\[[0-9;]*38;2;{r};{g};{b}(?:;[0-9]+)*m{}",
                        regex::escape(label)
                    );
                    assert!(
                        regex::Regex::new(&pattern).unwrap().is_match(&output),
                        "{} {label}: {output:?}",
                        theme.id
                    );
                }
            }
        }
        fs::remove_file(config_path).unwrap();
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    }

    #[test]
    fn fastfetch_apply_needs_only_palette_not_unused_native_theme_references() {
        let themes = crate::theme::ThemeRegistry::new().unwrap();
        for theme in themes.all() {
            let home = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(home.path().to_owned());
            let expected = FastfetchAdapter.generate_jsonc_config(theme).unwrap();
            let mut palette_only = theme.clone();
            palette_only.tool_refs.clear();
            FastfetchAdapter
                .apply_theme_with_env(&palette_only, &env)
                .unwrap();
            let path = FastfetchAdapter::theme_path(&env);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
            palette_only.palette.red = "invalid".into();
            assert!(FastfetchAdapter
                .apply_theme_with_env(&palette_only, &env)
                .is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
            assert!(!env
                .xdg_config_home()
                .join("fastfetch/config.jsonc")
                .exists());
            assert!(!env.zshrc_path().exists());
        }
    }

    #[test]
    fn fastfetch_presets_keep_layout_across_palettes_and_reject_invalid_colors_before_writing() {
        let themes = crate::theme::ThemeRegistry::new().unwrap();
        let mut layout = None;
        for theme in themes.all() {
            let mut config: serde_json::Value =
                serde_json::from_str(&FastfetchAdapter.generate_jsonc_config(theme).unwrap())
                    .unwrap();
            assert!(config["display"]["color"]["keys"]
                .as_str()
                .unwrap()
                .starts_with("38;2;"));
            config["display"].as_object_mut().unwrap().remove("color");
            if let Some(expected) = &layout {
                assert_eq!(
                    &config, expected,
                    "{} unexpectedly changes the preset layout",
                    theme.id
                );
            } else {
                layout = Some(config);
            }
        }
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let mut invalid = themes.get("nord").unwrap().clone();
        invalid.palette.red = "not-a-color".into();
        assert!(FastfetchAdapter
            .apply_theme_with_env(&invalid, &env)
            .is_err());
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    }

    #[test]
    fn test_tool_name() {
        let adapter = FastfetchAdapter;
        assert_eq!(adapter.tool_name(), "fastfetch");
    }

    #[test]
    fn test_apply_strategy_returns_environment_variable() {
        let adapter = FastfetchAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = FastfetchAdapter;
        let path = adapter.managed_config_path();
        assert!(path
            .to_string_lossy()
            .contains(".config/slate/managed/fastfetch"));
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = FastfetchAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
