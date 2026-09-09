//! btop consumes themes from its enumerated user themes directory, not an
//! arbitrary external file. Keep one owned asset there and edit only its
//! color_theme reference. No wrapper, startup hook, process signal or UI reset.
use super::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::{detection, env::SlateEnv, error::Result, theme::ThemeVariant};
use std::path::PathBuf;

pub(crate) mod config;
mod palette;

pub struct BtopAdapter;

impl BtopAdapter {
    pub fn config_path(env: &SlateEnv) -> PathBuf {
        env.xdg_config_home().join("btop/btop.conf")
    }

    pub fn theme_path(env: &SlateEnv) -> PathBuf {
        env.xdg_config_home().join("btop/themes/slate-sync.theme")
    }
}

impl ToolAdapter for BtopAdapter {
    fn tool_name(&self) -> &'static str {
        "btop"
    }

    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env("btop", env).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Ok(Self::config_path(&SlateEnv::from_process()?))
    }

    fn managed_config_path(&self) -> PathBuf {
        SlateEnv::from_process()
            .map(|env| Self::theme_path(&env))
            .unwrap_or_else(|_| PathBuf::from(".config/btop/themes/slate-sync.theme"))
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        self.apply_theme_with_env(theme, &SlateEnv::from_process()?)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        config::apply(env, theme)?;
        // No shell env changes. The next btop launch reads its configuration;
        // this is not a claim that an already-running instance has reloaded.
        Ok(ApplyOutcome::applied_no_shell())
    }
}
