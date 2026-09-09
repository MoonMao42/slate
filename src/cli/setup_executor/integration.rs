use crate::adapter::{AlacrittyAdapter, BatAdapter, GhosttyAdapter, StarshipAdapter};
use crate::cli::theme_apply;
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::platform::shell::ShellBackend;
use crate::theme::{ThemeRegistry, ThemeVariant};
use std::path::Path;

pub(crate) fn resolve_selected_theme(theme: Option<&str>, env: &SlateEnv) -> Result<ThemeVariant> {
    let theme_id = if let Some(theme_name) = theme {
        theme_name.to_string()
    } else if let Some(current_theme) = ConfigManager::from_env_paths(env).get_current_theme()? {
        current_theme
    } else {
        crate::theme::DEFAULT_THEME_ID.to_string()
    };

    let registry = ThemeRegistry::new()?;
    registry.get(&theme_id).cloned().ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!(
            "Theme '{}' not found",
            theme_id.escape_default()
        ))
    })
}

/// Setup shell integration: generate env files, then wire the detected shell loader.
#[allow(dead_code)] // Compatibility helper, also exercised directly by unit tests.
pub(crate) fn setup_shell_integration_with_env(
    theme: Option<&str>,
    env: &SlateEnv,
    tools_to_configure: &[String],
) -> Result<(ThemeVariant, theme_apply::ThemeApplyReport)> {
    let shell = crate::platform::shell::detect_backend();
    validate_shell(shell)?;
    let selected_theme = resolve_selected_theme(theme, env)?;
    let report = setup_prepared_shell_integration(&selected_theme, env, tools_to_configure, shell)?;
    Ok((selected_theme, report))
}

pub(super) fn validate_shell(shell: ShellBackend) -> Result<()> {
    if shell == ShellBackend::Unsupported {
        return Err(SlateError::PlatformError(
            "Slate shell integration currently targets zsh, bash, and fish only.".into(),
        ));
    }
    Ok(())
}

pub(super) fn setup_prepared_shell_integration(
    selected_theme: &ThemeVariant,
    env: &SlateEnv,
    tools_to_configure: &[String],
    shell: ShellBackend,
) -> Result<theme_apply::ThemeApplyReport> {
    validate_shell(shell)?;
    let loader = super::shell_loader::PreparedShellLoader::capture(env, shell)?;
    setup_with_loader(selected_theme, env, tools_to_configure, &loader)
}

pub(super) fn setup_with_loader(
    selected_theme: &ThemeVariant,
    env: &SlateEnv,
    tools_to_configure: &[String],
    loader: &super::shell_loader::PreparedShellLoader,
) -> Result<theme_apply::ThemeApplyReport> {
    loader.verify(env)?;

    // The coordinator writes shared env files even when all tools are skipped.
    // Only connect the loader after a successful pass, preserving the old shell
    // environment if a tool fails or the safety snapshot cannot be created.
    let mut report = theme_apply::apply_theme_selection_for_tools_with_env(
        selected_theme,
        env,
        Some(tools_to_configure),
    )?;
    report.ensure_no_failures()?;

    loader.publish(env)?;

    // Unlike a no-target theme apply, setup has just connected a real shell
    // loader. Record that choice even when no adapters needed configuration,
    // or the next setup/Neovim follow-up would use the old/default theme.
    // Preserve the coordinator's ordinary no-adapter guard for other callers.
    if report.applied_count() == 0 && theme_apply_issues(&report.results).is_empty() {
        if let Err(error) = ConfigManager::from_env_paths(env).set_current_theme(&selected_theme.id)
        {
            report.commit_failure = Some(theme_apply::ThemeCommitFailure {
                stage: theme_apply::ThemeCommitStage::CurrentTheme,
                error,
            });
            report.ensure_no_failures()?;
        }
    }

    Ok(report)
}

#[allow(dead_code)]
pub(crate) fn setup_shell_integration(
    theme: Option<&str>,
) -> Result<(ThemeVariant, theme_apply::ThemeApplyReport)> {
    let env = SlateEnv::from_process()?;
    setup_shell_integration_with_env(theme, &env, &[])
}

/// Ensure integration config files exist for detected tools so adapters can write to them.
pub(crate) fn ensure_tool_configs(
    env: &SlateEnv,
    user_selected: &[String],
    just_installed: &[String],
) -> Vec<String> {
    use std::fs;

    fn touch_config(tool_id: &str, path: &Path, issues: &mut Vec<String>) {
        use std::io::Write;
        match fs::symlink_metadata(path) {
            Ok(_) => return, // Includes dangling links; never create through one.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Err(error) = crate::config::file_read::confirm_missing(path) {
                    issues.push(format!(
                        "Could not inspect {tool_id} configuration at {}: {error}",
                        path.display()
                    ));
                    return;
                }
            }
            Err(error) => {
                issues.push(format!(
                    "Could not inspect {tool_id} configuration at {}: {error}",
                    path.display()
                ));
                return;
            }
        }

        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                issues.push(format!(
                    "Could not create {} config directory at {}: {}",
                    tool_id,
                    parent.display(),
                    err
                ));
                return;
            }
        }

        let comment = match tool_id {
            "ghostty" => "# Ghostty configuration — managed by ~/.config/slate/managed/ghostty/\n",
            "alacritty" => "# Alacritty configuration — managed imports in general.import\n",
            "kitty" => "# Kitty configuration — managed imports\n",
            "bat" => "# bat configuration — managed imports\n",
            "delta" => "# git configuration — managed imports\n",
            _ => "# Slate configuration\n",
        };

        // A config created after inspection belongs to the user. Exclusive
        // creation prevents truncating it or following a newly inserted link.
        if let Err(err) = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut file| file.write_all(comment.as_bytes()))
        {
            issues.push(format!(
                "Could not initialize {} config file at {}: {}",
                tool_id,
                path.display(),
                err
            ));
        }
    }

    fn seed_starship_config(env: &SlateEnv, path: &Path, issues: &mut Vec<String>) {
        let needs_seed = match fs::read(path) {
            Ok(content) => match String::from_utf8(content) {
                Ok(content) => {
                    crate::config::shell_integration::should_upgrade_seeded_starship_content(
                        &content,
                    )
                }
                Err(_) => {
                    issues.push(format!(
                        "Could not inspect starship config at {}: file is not valid UTF-8",
                        path.display()
                    ));
                    false
                }
            },
            Err(err) => {
                issues.push(format!(
                    "Could not inspect starship config at {}: {}",
                    path.display(),
                    err
                ));
                false
            }
        };

        if needs_seed {
            if let Err(err) = crate::config::prompt::starter_content(env)
                .and_then(|content| fs::write(path, content).map_err(Into::into))
            {
                issues.push(format!(
                    "Could not seed starship config at {}: {}",
                    path.display(),
                    err
                ));
            }
        }
    }

    let mut installed = crate::cli::tool_selection::detect_installed_tools_with_env(env);
    for tool_id in just_installed {
        installed.insert(
            tool_id.clone(),
            crate::detection::ToolPresence {
                installed: true,
                in_path: true,
                evidence: None,
            },
        );
    }
    let mut issues = Vec::new();

    let user_set: std::collections::HashSet<&str> = user_selected
        .iter()
        .chain(just_installed.iter())
        .map(|selection| selection.as_str())
        .collect();
    // Policy: terminal adapters that are already installed as the user's primary terminal
    // (tier1) are configured automatically even when the user didn't explicitly tick them in
    // the wizard — the starter kit's value proposition is a one-shot "my terminal looks good",
    // and asking users to re-select their current terminal every setup is friction that fails
    // the 30-second promise. Non-terminal tools still require explicit user selection.
    let should_configure = |id: &str| -> bool {
        let presence = installed.get(id);
        let is_detected = presence
            .map(|tool_presence| tool_presence.installed)
            .unwrap_or(false);
        if !is_detected {
            return false;
        }
        if id == "ghostty" || id == "alacritty" || id == "kitty" {
            return presence
                .map(|tool_presence| tool_presence.is_tier1())
                .unwrap_or(false)
                || user_set.contains(id);
        }
        user_set.contains(id)
    };

    if should_configure("ghostty") {
        let adapter = GhosttyAdapter;
        match adapter.integration_config_path_with_env(env) {
            Ok(path) => touch_config("ghostty", &path, &mut issues),
            Err(err) => issues.push(format!("Could not resolve ghostty config path: {}", err)),
        }
    }
    if should_configure("starship") {
        let path = StarshipAdapter::integration_config_path_with_env(env);
        touch_config("starship", &path, &mut issues);
        if path.exists() {
            seed_starship_config(env, &path, &mut issues);
        }
    }
    if should_configure("alacritty") {
        touch_config(
            "alacritty",
            &AlacrittyAdapter::integration_config_path_with_env(env),
            &mut issues,
        );
    }
    if should_configure("kitty") {
        touch_config(
            "kitty",
            &crate::adapter::kitty::KittyAdapter::resolve_config_path_with_env(env),
            &mut issues,
        );
    }
    if should_configure("bat") {
        let adapter = BatAdapter;
        match adapter.integration_config_path_with_env(env) {
            Ok(path) => touch_config("bat", &path, &mut issues),
            Err(err) => issues.push(format!("Could not resolve bat config path: {}", err)),
        }
    }
    if should_configure("delta") {
        touch_config("delta", &env.home().join(".gitconfig"), &mut issues);
    }

    issues
}

pub(crate) fn theme_apply_issues(results: &[crate::adapter::ToolApplyResult]) -> Vec<String> {
    results
        .iter()
        .filter_map(|result| match &result.status {
            crate::adapter::ToolApplyStatus::Skipped(
                crate::adapter::SkipReason::MissingIntegrationConfig,
            ) => Some(format!(
                "{} is installed, but slate could not initialize its integration file.",
                result.tool_name
            )),
            crate::adapter::ToolApplyStatus::Failed(err) => Some(format!(
                "{} failed during theme apply: {}",
                result.tool_name, err
            )),
            crate::adapter::ToolApplyStatus::Skipped(crate::adapter::SkipReason::NotInstalled)
            | crate::adapter::ToolApplyStatus::Skipped(
                crate::adapter::SkipReason::ThemeNotCommitted,
            )
            | crate::adapter::ToolApplyStatus::Applied => None,
        })
        .collect()
}

#[cfg(test)]
#[path = "integration/shell_paths_tests.rs"]
mod shell_paths_tests;
