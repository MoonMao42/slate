//! A GUI-only palette fragment. Lazygit requires lists of color attributes,
//! not scalar YAML strings. Personal layout, bindings and pagers stay untouched.

use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::path::PathBuf;

/// lazygit adapter implementing the ToolAdapter trait.
pub struct LazygitAdapter;

impl LazygitAdapter {
    pub fn theme_path(env: &SlateEnv) -> PathBuf {
        env.managed_file("managed/lazygit/config.yml")
    }
}

impl ToolAdapter for LazygitAdapter {
    fn tool_name(&self) -> &'static str {
        "lazygit"
    }

    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Ok(SlateEnv::from_process()?
            .lazygit_primary_config()
            .to_owned())
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("lazygit")
        } else {
            PathBuf::from(".config/slate/managed/lazygit")
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
        let managed_content = self.generate_yaml_config(theme)?;
        let path = Self::theme_path(env);
        if path.to_str().is_none_or(|s| s.contains(',')) {
            return Err(SlateError::InvalidConfig(
                "Lazygit's comma-separated LG_CONFIG_FILE cannot represent this managed path"
                    .into(),
            ));
        }
        super::managed_fragment::write(env, &path, managed_content.as_bytes(), "lazygit")?;

        // lazygit reads LG_CONFIG_FILE at launch; the managed config only
        // reaches a new lazygit process spawned from a fresh shell.
        Ok(ApplyOutcome::applied_needs_new_shell())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

impl LazygitAdapter {
    pub(crate) fn generate_yaml_config(&self, theme: &ThemeVariant) -> Result<String> {
        let p = &theme.palette;
        p.validate()?;
        // A neutral selected row replaces the former solid error-red block.
        // Keep ordinary foreground legible instead of assuming ANSI accents
        // work as backgrounds in both light and dark palettes.
        let selected = [
            p.selection_bg.as_deref(),
            p.surface0.as_deref(),
            p.bg_dim.as_deref(),
            Some(p.background.as_str()),
        ]
        .into_iter()
        .flatten()
        .find(|bg| crate::wcag::contrast_hex(&p.foreground, bg) >= 4.5)
        .unwrap_or(&p.background);
        let cherry_fg = crate::wcag::pick_accessible_fg_for_bg(&[&p.blue, &p.foreground], selected);
        let marked_fg =
            crate::wcag::pick_accessible_fg_for_bg(&[&p.yellow, &p.foreground], selected);
        let mut config = String::from("# Managed by Slate: GUI colors only.\ngui:\n  theme:\n");
        for (key, color, bold) in [
            ("activeBorderColor", p.brand_accent.as_str(), true),
            (
                "inactiveBorderColor",
                p.overlay0.as_deref().unwrap_or(&p.bright_black),
                false,
            ),
            ("searchingActiveBorderColor", &p.yellow, true),
            ("optionsTextColor", &p.blue, false),
            ("selectedLineBgColor", selected, false),
            ("inactiveViewSelectedLineBgColor", selected, false),
            ("cherryPickedCommitFgColor", cherry_fg.as_str(), false),
            ("cherryPickedCommitBgColor", selected, false),
            ("markedBaseCommitFgColor", marked_fg.as_str(), false),
            ("markedBaseCommitBgColor", selected, false),
            ("unstagedChangesColor", &p.red, false),
            ("defaultFgColor", &p.foreground, false),
        ] {
            config.push_str(&format!(
                "    {key}: ['{color}'{}]\n",
                if bold { ", 'bold'" } else { "" }
            ));
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    };

    #[test]
    fn lazygit_palettes_use_native_color_lists_and_legible_selected_rows() {
        for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
            let config = LazygitAdapter.generate_yaml_config(theme).unwrap();
            let lines = config
                .lines()
                .filter(|line| line.starts_with("    "))
                .collect::<Vec<_>>();
            assert_eq!(lines.len(), 12);
            for line in &lines {
                let (_, values) = line.split_once(": ").unwrap();
                assert!(
                    values.starts_with("['#") && values.ends_with("']"),
                    "{line}"
                );
                assert!(values.len() == 11 || values.len() == 19, "{line}");
            }
            let row = lines
                .iter()
                .find(|line| line.contains("selectedLineBgColor:"))
                .unwrap();
            let bg = &row[row.find('#').unwrap()..][..7];
            assert!(
                crate::wcag::contrast_hex(&theme.palette.foreground, bg) >= 4.5,
                "{}: {row}",
                theme.id
            );
            assert_ne!(bg, theme.palette.red);
            assert!(!config.contains("pager:") && !config.contains("git:"));
        }
    }

    #[test]
    fn lazygit_apply_only_writes_its_fragment_and_preserves_mode_and_noop_identity() {
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let path = LazygitAdapter::theme_path(&env);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "gui:\n  theme:\n    activeBorderColor: '#89b4fa'\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let personal = env.lazygit_default_config();
        fs::create_dir_all(personal.parent().unwrap()).unwrap();
        fs::write(personal, "# PRIVATE\ngui:\n  scrollHeight: 7\n").unwrap();
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        for _ in 0..2 {
            assert_eq!(
                LazygitAdapter.apply_theme_with_env(&theme, &env).unwrap(),
                ApplyOutcome::applied_needs_new_shell()
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
            assert_eq!(
                fs::read_to_string(personal).unwrap(),
                "# PRIVATE\ngui:\n  scrollHeight: 7\n"
            );
            assert!(!env.managed_file("config.toml").exists());
            assert!(!env.managed_file("current").exists());
            assert!(!env.slate_cache_dir().exists());
        }
        let before = fs::metadata(&path).unwrap();
        LazygitAdapter.apply_theme_with_env(&theme, &env).unwrap();
        let after = fs::metadata(&path).unwrap();
        assert_eq!(before.ino(), after.ino());
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        let mut invalid = theme;
        invalid.palette.red = "#123456'\ngit: bad".into();
        assert!(LazygitAdapter.apply_theme_with_env(&invalid, &env).is_err());
        assert_eq!(fs::metadata(&path).unwrap().ino(), before.ino());
    }

    #[test]
    fn lazygit_apply_rejects_unrepresentable_and_unsafe_targets_before_writing() {
        let temp = tempfile::tempdir().unwrap();
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let comma = SlateEnv::with_home(temp.path().join("comma,home"));
        assert!(LazygitAdapter.apply_theme_with_env(&theme, &comma).is_err());
        assert!(!comma.home().exists());
        let env = SlateEnv::with_home(temp.path().join("home"));
        let path = LazygitAdapter::theme_path(&env);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let outside = temp.path().join("outside");
        fs::write(&outside, "PRIVATE").unwrap();
        symlink(&outside, &path).unwrap();
        assert!(LazygitAdapter.apply_theme_with_env(&theme, &env).is_err());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "PRIVATE");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(LazygitAdapter.apply_theme_with_env(&theme, &env).is_err());
    }

    #[test]
    fn test_tool_name() {
        let adapter = LazygitAdapter;
        assert_eq!(adapter.tool_name(), "lazygit");
    }

    #[test]
    fn test_apply_strategy_returns_environment_variable() {
        let adapter = LazygitAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = LazygitAdapter;
        let path = adapter.managed_config_path();
        assert!(path
            .to_string_lossy()
            .contains(".config/slate/managed/lazygit"));
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = LazygitAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_generate_yaml_config_includes_gui_theme_colors() {
        let adapter = LazygitAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let config = adapter.generate_yaml_config(&theme).unwrap();

        assert!(config.contains("gui:\n  theme:"));
        assert!(config.contains("activeBorderColor:"));
        assert!(config.contains("inactiveBorderColor:"));
        assert!(config.contains("selectedLineBgColor:"));
        assert!(!config.contains("  theme:\n  gui:"));
        assert!(!config.contains("pager:"));
        assert!(config.contains("activeBorderColor: ['"));
        assert_eq!(
            config
                .lines()
                .filter(|line| line.starts_with("    "))
                .count(),
            12
        );
    }
}
