//! Native flavor integration; no keymaps, plugins, launch wrappers or signals.
use super::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::{detection, env::SlateEnv, error::Result, theme::ThemeVariant};
use std::path::PathBuf;

pub(crate) mod config;
mod palette;

pub struct YaziAdapter;

impl YaziAdapter {
    pub fn config_path(env: &SlateEnv) -> PathBuf {
        env.yazi_config_home().join("theme.toml")
    }
    pub fn flavor_path(env: &SlateEnv) -> PathBuf {
        env.yazi_config_home()
            .join("flavors/slate-sync.yazi/flavor.toml")
    }
    pub fn syntax_path(env: &SlateEnv) -> PathBuf {
        env.yazi_config_home()
            .join("flavors/slate-sync.yazi/tmtheme.xml")
    }
    pub fn paths(env: &SlateEnv) -> [PathBuf; 3] {
        [
            Self::config_path(env),
            Self::flavor_path(env),
            Self::syntax_path(env),
        ]
    }
}

impl ToolAdapter for YaziAdapter {
    fn tool_name(&self) -> &'static str {
        "yazi"
    }
    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }
    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env("yazi", env).installed)
    }
    fn integration_config_path(&self) -> Result<PathBuf> {
        Ok(Self::config_path(&SlateEnv::from_process()?))
    }
    fn managed_config_path(&self) -> PathBuf {
        SlateEnv::from_process()
            .map(|env| Self::flavor_path(&env))
            .unwrap_or_else(|_| PathBuf::from(".config/yazi/flavors/slate-sync.yazi/flavor.toml"))
    }
    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }
    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        self.apply_theme_with_env(theme, &SlateEnv::from_process()?)
    }
    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        config::apply(env, theme)?;
        Ok(ApplyOutcome::applied_no_shell())
    }
}
