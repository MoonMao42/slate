//! Native Zellij theme file and scoped KDL theme choices; no session commands.
use super::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::{env::SlateEnv, error::Result, theme::ThemeVariant};
use std::path::PathBuf;

pub(crate) mod config;
mod kdl_guard;
mod palette;
pub struct ZellijAdapter;

impl ZellijAdapter {
    pub fn config_path(env: &SlateEnv) -> Result<PathBuf> {
        Ok(env.zellij_paths()?.1.to_owned())
    }
    pub fn paths(env: &SlateEnv) -> Result<[PathBuf; 2]> {
        config::paths(env)
    }
}

impl ToolAdapter for ZellijAdapter {
    fn tool_name(&self) -> &'static str {
        "zellij"
    }
    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }
    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(crate::detection::detect_tool_presence_with_env("zellij", env).installed)
    }
    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::config_path(&SlateEnv::from_process()?)
    }
    fn managed_config_path(&self) -> PathBuf {
        SlateEnv::from_process()
            .and_then(|env| Self::paths(&env))
            .map(|paths| paths[1].clone())
            .unwrap_or_else(|_| PathBuf::from(".config/zellij/themes/slate-sync.kdl"))
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
