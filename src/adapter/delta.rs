//! delta adapter with marker block integration.
//! delta uses git config [include] blocks to reference managed config.
//! The adapter uses the MarkerBlock module for safe, validated editing,
//! and synchronizes bat --theme with delta --syntax-theme so the two agree.

use crate::adapter::{marker_block, ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::path::{Path, PathBuf};

/// delta adapter implementing the ToolAdapter trait.
pub struct DeltaAdapter;

impl DeltaAdapter {
    /// Path to ~/.gitconfig (integration file)
    fn gitconfig_path() -> Result<PathBuf> {
        let env = SlateEnv::from_process()?;
        Self::gitconfig_path_with_env(&env)
    }

    fn gitconfig_path_with_env(env: &SlateEnv) -> Result<PathBuf> {
        Ok(env.home().join(".gitconfig"))
    }

    /// Format include path for gitconfig
    fn format_gitconfig_include_path(config_path: &Path) -> Result<String> {
        let path = config_path.to_str().filter(|value| !value.chars().any(char::is_control))
            .ok_or_else(|| crate::error::SlateError::InvalidConfig(
                "Delta include path must be UTF-8 without control characters; no files were changed.".into()
            ))?;
        let escaped = path.replace('\\', r"\\").replace('"', "\\\"");
        Ok(format!(r#""{}""#, escaped))
    }

    /// Render delta config in gitconfig INI format with marker blocks
    fn render_delta_config(_theme: &ThemeVariant, managed_path: &Path) -> Result<String> {
        let managed_str = Self::format_gitconfig_include_path(managed_path)?;
        Ok(format!(
            "{}\n[include]\n\tpath = {}\n{}\n",
            marker_block::START,
            managed_str,
            marker_block::END
        ))
    }

    /// Render delta color theme settings (for managed config file).
    /// `dark` / `light` mirrors the active theme's appearance so delta picks
    /// the right default `+`/`-` line backgrounds and context-line styling.
    /// Hard-coding `dark = true` made every light-theme diff render with
    /// dark-terminal-tuned defaults — context lines washed out against the
    /// cream bg.
    fn render_delta_colors(theme: &ThemeVariant) -> Result<String> {
        let syntax_theme = theme
            .tool_refs
            .get("delta")
            .map(|s| s.as_str())
            .filter(|name| !name.trim().is_empty() && !name.chars().any(char::is_control))
            .ok_or_else(|| crate::error::SlateError::InvalidThemeData(
                "Delta requires a nonempty syntax theme reference without control characters; no fallback was selected.".into()
            ))?;
        let syntax_theme = syntax_theme.replace('\\', "\\\\").replace('"', "\\\"");
        let appearance_flag = match theme.appearance {
            crate::theme::ThemeAppearance::Light => "light = true",
            crate::theme::ThemeAppearance::Dark => "dark = true",
        };
        Ok(format!(
            "[delta]\n\
             syntax-theme = \"{}\"\n\
             {}\n\
             line-numbers = true\n",
            syntax_theme, appearance_flag
        ))
    }
}

impl ToolAdapter for DeltaAdapter {
    fn tool_name(&self) -> &'static str {
        "delta"
    }

    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Self::gitconfig_path()
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("delta")
        } else {
            PathBuf::from(".config/slate/managed/delta")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::WriteAndInclude
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        // Validate theme has palette data
        theme.palette.validate()?;

        let gitconfig_path = Self::gitconfig_path_with_env(env)?;
        crate::config::recovery_paths::validate_file_path(env, &gitconfig_path, "Delta")?;
        let destination = super::integration_publish::destination(
            &gitconfig_path,
            "Git configuration for Delta",
        )?;
        let source = crate::config::file_read::read(
            &gitconfig_path,
            crate::config::file_read::MAX_TOOL_CONFIG_BYTES,
            crate::config::file_read::Links::Reject,
        )
        .map_err(|_| {
            crate::error::SlateError::InvalidConfig(
                "Cannot safely read Git configuration for Delta; no files were changed.".into(),
            )
        })?;
        let Some(source) = source else {
            return Ok(ApplyOutcome::Skipped(
                crate::adapter::SkipReason::MissingIntegrationConfig,
            ));
        };

        // Render delta colors config
        let delta_colors = Self::render_delta_colors(theme)?;
        let managed_path = env.managed_file("managed/delta/colors");
        let new_block = Self::render_delta_config(theme, &managed_path)?;
        // Detect malformed existing markers before initializing or writing any
        // managed state. Publication below still re-reads the integration file.
        let updated =
            marker_block::upsert_managed_block_bytes(&source.bytes, new_block.as_bytes())?;
        if updated.len() as u64 > crate::config::file_read::MAX_TOOL_CONFIG_BYTES {
            return Err(crate::error::SlateError::InvalidConfig(
                "Adding Delta's include would exceed the 8 MiB Git configuration limit; no files were changed.".into(),
            ));
        }

        // Publish just this adapter's palette without initializing the profile.
        super::managed_fragment::write(env, &managed_path, delta_colors.as_bytes(), "delta")?;

        super::integration_publish::publish(
            env,
            &gitconfig_path,
            &destination,
            Some(&source),
            &updated,
            "Git configuration",
        )?;

        // delta reads colors from git config on every invocation; no shell
        // restart required.
        Ok(ApplyOutcome::applied_no_shell())
    }

    fn reload(&self) -> Result<()> {
        // delta is a pager, no reload mechanism
        Ok(())
    }
}

#[cfg(test)]
mod tests;
