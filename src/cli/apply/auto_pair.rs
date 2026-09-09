use super::ReloadWarning;
use crate::{config::ConfigManager, error::Result, theme::ThemeAppearance};

#[derive(Debug, Clone, Copy)]
pub(super) enum AutoPairPolicy {
    RememberManualSelection,
    Preserve,
}

impl AutoPairPolicy {
    /// Only called after the current-theme record has been committed. Pairing
    /// is a preference update, not a prerequisite for publishing that theme.
    pub(super) fn record_after_commit(
        self,
        config: &ConfigManager,
        theme_id: &str,
        detect_appearance: impl FnOnce() -> Result<ThemeAppearance>,
    ) -> Option<ReloadWarning> {
        if matches!(self, Self::Preserve) {
            return None;
        }
        let update = || -> Result<()> {
            if !config.is_auto_theme_enabled()? {
                return Ok(());
            }
            match detect_appearance()? {
                ThemeAppearance::Dark => config.write_auto_config(Some(theme_id), None),
                ThemeAppearance::Light => config.write_auto_config(None, Some(theme_id)),
            }
        };
        update().err().map(|err| ReloadWarning {
            informational: false,
            tool_name: "auto-theme".into(),
            message: format!(
                "Theme '{}' was already saved, but its automatic pairing could not be updated: {err}. The saved theme was not rolled back; the next automatic switch may use the previous pairing. Check auto.toml before retrying the manual selection.",
                theme_id.escape_default(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{env::SlateEnv, error::SlateError};
    use std::fs;

    #[test]
    fn auto_pair_preserve_never_reads_preferences_or_detects_appearance() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        // Even corrupt state is outside this policy's remit. Shared commit
        // validation and automatic theme resolution still validate their inputs.
        fs::write(env.managed_file("config.toml"), "not valid TOML").unwrap();
        for content in [None, Some("light_theme = 42\n# preserved\n")] {
            if let Some(content) = content {
                fs::write(env.managed_file("auto.toml"), content).unwrap();
            }
            assert!(AutoPairPolicy::Preserve
                .record_after_commit(&config, "nord", || panic!("must not detect"))
                .is_none());
            assert_eq!(
                fs::read_to_string(env.managed_file("auto.toml"))
                    .ok()
                    .as_deref(),
                content
            );
        }
    }

    #[test]
    fn auto_pair_manual_selection_updates_only_the_detected_slot() {
        for appearance in [ThemeAppearance::Dark, ThemeAppearance::Light] {
            let td = tempfile::tempdir().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let config = ConfigManager::with_env(&env).unwrap();
            config.set_auto_theme_enabled(true).unwrap();
            fs::write(env.managed_file("auto.toml"), "# saved preference\ndark_theme = 'nord'\nlight_theme = 'catppuccin-latte'\nextra = 7\n").unwrap();
            assert!(AutoPairPolicy::RememberManualSelection
                .record_after_commit(&config, "catppuccin-mocha", || Ok(appearance))
                .is_none());
            let pair = config.read_auto_config().unwrap().unwrap();
            let (dark, light) = match appearance {
                ThemeAppearance::Dark => ("catppuccin-mocha", "catppuccin-latte"),
                ThemeAppearance::Light => ("nord", "catppuccin-mocha"),
            };
            assert_eq!(pair.dark_theme.as_deref(), Some(dark));
            assert_eq!(pair.light_theme.as_deref(), Some(light));
            let text = fs::read_to_string(env.managed_file("auto.toml")).unwrap();
            assert!(text.contains("# saved preference\n"));
            assert!(text.contains("extra = 7\n"));
        }
    }

    #[test]
    fn auto_pair_disabled_skips_detection_and_failures_are_retained_without_contents() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        let config = ConfigManager::with_env(&env).unwrap();
        config.set_auto_theme_enabled(false).unwrap();
        let malformed = "light_theme = 'PRIVATE_VALUE'\ndark_theme = [\n";
        fs::write(env.managed_file("auto.toml"), malformed).unwrap();
        assert!(AutoPairPolicy::RememberManualSelection
            .record_after_commit(&config, "nord", || panic!("disabled"))
            .is_none());
        config.set_auto_theme_enabled(true).unwrap();
        let warning = AutoPairPolicy::RememberManualSelection
            .record_after_commit(&config, "nord", || Ok(ThemeAppearance::Dark))
            .unwrap();
        assert_eq!(warning.tool_name, "auto-theme");
        assert!(warning.message.contains("already saved"));
        assert!(warning.message.contains("previous pairing"));
        assert!(!warning.message.contains("PRIVATE_VALUE"));
        assert_eq!(
            fs::read_to_string(env.managed_file("auto.toml")).unwrap(),
            malformed
        );

        let warning = AutoPairPolicy::RememberManualSelection
            .record_after_commit(&config, "nord", || {
                Err(SlateError::Internal("injected detection failure".into()))
            })
            .unwrap();
        assert!(warning.message.contains("injected detection failure"));
        fs::write(
            env.managed_file("config.toml"),
            "[auto_theme]\nenabled = 'PRIVATE_FLAG'\n",
        )
        .unwrap();
        let warning = AutoPairPolicy::RememberManualSelection
            .record_after_commit(&config, "nord", || {
                panic!("invalid flag must not become disabled")
            })
            .unwrap();
        assert!(warning.message.contains("must be a boolean"));
        assert!(!warning.message.contains("PRIVATE_FLAG"));
    }
}
