use crate::brand::events::{dispatch, BrandEvent, SuccessKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::font::resolve_font_choice;
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::opacity::OpacityPreset;
use crate::theme::ThemeRegistry;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ToolImportFlags {
    starship: bool,
    highlighting: bool,
    fastfetch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportRequest {
    theme: Option<String>,
    font: Option<String>,
    opacity: Option<OpacityPreset>,
    tools: ToolImportFlags,
}

/// Export current slate config as a shareable URI.
/// Format: slate://theme/font/opacity/tools
/// Example: slate://catppuccin-mocha/JetBrainsMono/frosted/s,h,f
/// Tool flags: s=starship, h=highlighting, f=fastfetch
pub fn handle_export() -> Result<()> {
    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;

    let theme = config
        .get_current_theme()?
        .unwrap_or_else(|| "none".to_string());

    let font = config
        .get_current_font()?
        .unwrap_or_else(|| "none".to_string())
        .replace(' ', "-");

    let opacity = config
        .get_current_opacity()?
        .unwrap_or_else(|| "solid".to_string())
        .to_lowercase();

    let mut tools = Vec::new();
    if config.is_starship_enabled()? {
        tools.push("s");
    }
    if config.is_zsh_highlighting_enabled()? {
        tools.push("h");
    }
    if config.has_fastfetch_autorun()? {
        tools.push("f");
    }
    let tools_str = if tools.is_empty() {
        "none".to_string()
    } else {
        tools.join(",")
    };

    let uri = format!("slate://{}/{}/{}/{}", theme, font, opacity, tools_str);

    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);

    println!();
    println!("  {}", path_text(r.as_ref(), &uri));
    println!();
    println!("  Share this with anyone — they can run:");
    println!("  slate import \"{}\"", path_text(r.as_ref(), &uri));
    println!();

    Ok(())
}

/// Import a slate config from a shareable URI.
/// Parses the URI and applies theme, font, opacity, and tool toggles.
/// On success emits the share-success line via `Roles::status_success`
/// (theme.green per D-01a — NEVER lavender) and dispatches
/// `BrandEvent::Success(SuccessKind::ConfigSet)` so 
/// SoundSink can ring the share-import completion moment alongside the
/// other config-mutation surfaces.
pub fn handle_import(uri: &str) -> Result<()> {
    let env = SlateEnv::from_process()?;
    handle_import_with_env(uri, &env)
}

fn handle_import_with_env(uri: &str, env: &SlateEnv) -> Result<()> {
    let request = parse_import_request(uri)?;

    if let Some(font) = request.font.as_deref() {
        crate::cli::font::handle_font(Some(font))?;
    }

    if let Some(theme) = request.theme.clone() {
        crate::cli::theme::handle_theme(Some(theme), false, false)?;
    }

    let config = ConfigManager::with_env(env)?;

    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);

    if let Some(opacity) = request.opacity {
        crate::cli::apply::apply_opacity(
            env,
            opacity,
            crate::cli::apply::OpacityApplyOptions {
                persist_state: true,
                reload_terminals: true,
            },
        )?;
        let value = opacity.to_string().to_lowercase();
        println!(
            "{}",
            status_success_line(
                r.as_ref(),
                &format!("Opacity set to {}", code_text(r.as_ref(), &value)),
            )
        );
    }

    apply_imported_tool_flags(&config, request.tools)?;

    println!();
    println!(
        "  {}",
        status_success_line(r.as_ref(), "Config imported successfully")
    );
    println!("  Open a new terminal to see all changes.");
    println!();

    // a successful import is a config-set moment
    // maps this onto the success SFX channel.
    dispatch(BrandEvent::Success(SuccessKind::ConfigSet));

    Ok(())
}

/// Format a `log::success` body via `Roles::status_success` (theme.green
/// NEVER lavender per D-01a), falling back to plain `✓ message`.
fn status_success_line(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_success(message),
        None => format!("✓ {}", message),
    }
}

/// Wrap a literal value (path, hex, opacity preset) in `Roles::code`
/// (inline-code pill per Sketch 001), falling back to bare text when
/// Roles is unavailable.
fn code_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.code(text),
        None => format!("`{}`", text),
    }
}

/// Render a path / URI through `Roles::path` (dim + italic per Sketch
/// 002), falling back to bare text when Roles is unavailable.
fn path_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.path(text),
        None => text.to_string(),
    }
}

fn parse_import_request(uri: &str) -> Result<ImportRequest> {
    let stripped = uri
        .strip_prefix("slate://")
        .ok_or_else(|| SlateError::InvalidConfig("URI must start with slate://".to_string()))?;

    let parts: Vec<&str> = stripped.split('/').collect();
    if parts.len() != 4 {
        return Err(SlateError::InvalidConfig(
            "Expected format: slate://theme/font/opacity/tools".to_string(),
        ));
    }

    Ok(ImportRequest {
        theme: parse_theme_segment(parts[0])?,
        font: parse_font_segment(parts[1])?,
        opacity: parse_opacity_segment(parts[2])?,
        tools: parse_tool_flags(parts[3])?,
    })
}

fn parse_theme_segment(theme: &str) -> Result<Option<String>> {
    if theme == "none" {
        return Ok(None);
    }

    let registry = ThemeRegistry::new()?;
    if registry.get(theme).is_none() {
        return Err(SlateError::ThemeNotFound(theme.to_string()));
    }

    Ok(Some(theme.to_string()))
}

fn parse_font_segment(font: &str) -> Result<Option<String>> {
    if font == "none" {
        return Ok(None);
    }

    let resolved = resolve_font_choice(font)?;
    Ok(Some(resolved.font_name().to_string()))
}

fn parse_opacity_segment(opacity: &str) -> Result<Option<OpacityPreset>> {
    if opacity == "none" {
        return Ok(None);
    }

    opacity.parse::<OpacityPreset>().map(Some).map_err(|_| {
        SlateError::InvalidConfig(format!(
            "Invalid opacity preset: '{}'. Must be one of: solid, frosted, clear",
            opacity
        ))
    })
}

fn parse_tool_flags(tools: &str) -> Result<ToolImportFlags> {
    if tools == "none" {
        return Ok(ToolImportFlags::default());
    }

    let mut flags = ToolImportFlags::default();
    let mut seen = std::collections::BTreeSet::new();

    for flag in tools.split(',') {
        if flag.is_empty() || !seen.insert(flag) {
            return Err(SlateError::InvalidConfig(format!(
                "Invalid tool flag list: '{}'. Use comma-separated values from: s, h, f",
                tools
            )));
        }

        match flag {
            "s" => flags.starship = true,
            "h" => flags.highlighting = true,
            "f" => flags.fastfetch = true,
            _ => {
                return Err(SlateError::InvalidConfig(format!(
                    "Invalid tool flag list: '{}'. Use comma-separated values from: s, h, f",
                    tools
                )))
            }
        }
    }

    Ok(flags)
}

fn apply_imported_tool_flags(config: &ConfigManager, flags: ToolImportFlags) -> Result<()> {
    let previous_starship = config.is_starship_enabled()?;
    let previous_highlighting = config.is_zsh_highlighting_enabled()?;
    let previous_fastfetch = config.has_fastfetch_autorun()?;

    let starship_changed = previous_starship != flags.starship;
    let highlighting_changed = previous_highlighting != flags.highlighting;
    let fastfetch_changed = previous_fastfetch != flags.fastfetch;

    if !starship_changed && !highlighting_changed && !fastfetch_changed {
        return Ok(());
    }

    if starship_changed {
        config.set_starship_enabled(flags.starship)?;
    }
    if highlighting_changed {
        config.set_zsh_highlighting_enabled(flags.highlighting)?;
    }
    if fastfetch_changed {
        if flags.fastfetch {
            config.enable_fastfetch_autorun()?;
        } else {
            config.disable_fastfetch_autorun()?;
        }
    }

    if let Err(err) = config.refresh_shell_integration() {
        let mut rollback_errors: Vec<String> = Vec::new();
        if starship_changed {
            if let Err(e) = config.set_starship_enabled(previous_starship) {
                rollback_errors.push(format!("starship: {}", e));
            }
        }
        if highlighting_changed {
            if let Err(e) = config.set_zsh_highlighting_enabled(previous_highlighting) {
                rollback_errors.push(format!("zsh-highlighting: {}", e));
            }
        }
        if fastfetch_changed {
            let fastfetch_rollback = if previous_fastfetch {
                config.enable_fastfetch_autorun()
            } else {
                config.disable_fastfetch_autorun()
            };
            if let Err(e) = fastfetch_rollback {
                rollback_errors.push(format!("fastfetch: {}", e));
            }
        }
        if let Err(e) = config.refresh_shell_integration() {
            rollback_errors.push(format!("shell integration refresh: {}", e));
        }

        if rollback_errors.is_empty() {
            return Err(err);
        }
        return Err(SlateError::InvalidConfig(format!(
            "{} (rollback also failed: {})",
            err,
            rollback_errors.join("; ")
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

    #[test]
    fn test_export_produces_valid_uri() {
        let temp = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        config.set_current_theme("catppuccin-mocha").unwrap();
        config.set_starship_enabled(true).unwrap();

        // Verify config was set
        assert_eq!(
            config.get_current_theme().unwrap(),
            Some("catppuccin-mocha".to_string())
        );
        assert!(config.is_starship_enabled().unwrap());
    }

    #[test]
    fn test_import_rejects_invalid_uri() {
        let result = handle_import("invalid-uri");
        assert!(result.is_err());
    }

    #[test]
    fn test_import_rejects_wrong_segment_count() {
        let result = handle_import("slate://only-one-part");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_import_request_rejects_invalid_font() {
        let result = parse_import_request("slate://none/Definitely-Not-A-Font/solid/none");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_import_request_rejects_invalid_opacity() {
        let result = parse_import_request("slate://none/none/not-real/none");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_import_request_rejects_invalid_tool_flags() {
        let result = parse_import_request("slate://none/none/solid/s,x");
        assert!(result.is_err());
    }

    fn managed_tool_dir(env: &SlateEnv, tool: &str) -> std::path::PathBuf {
        env.config_dir().join("managed").join(tool)
    }

    #[test]
    fn test_import_opacity_only_applies_managed_files_immediately() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());

        handle_import_with_env("slate://none/none/frosted/none", &env).unwrap();

        assert_eq!(
            std::fs::read_to_string(env.managed_file("current-opacity")).unwrap(),
            "frosted"
        );
        assert!(managed_tool_dir(&env, "ghostty")
            .join("opacity.conf")
            .exists());
        assert!(managed_tool_dir(&env, "ghostty").join("blur.conf").exists());
        assert!(managed_tool_dir(&env, "kitty")
            .join("opacity.conf")
            .exists());
        assert!(managed_tool_dir(&env, "alacritty")
            .join("opacity.toml")
            .exists());
    }

    /// D-01a invariant — the share-import success line uses theme.green,
    /// never brand-lavender, across every render mode.
    #[test]
    fn share_status_success_line_never_emits_brand_lavender() {
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let out = status_success_line(Some(&r), "Config imported successfully");
            assert!(
                !out.contains("38;2;114;135;253"),
                "D-01a violation in mode {mode:?}: {out:?}"
            );
        }
    }

    /// Round-trip — `code_text` wraps in inline-code pill chrome in
    /// truecolor; falls back to backticks when Roles is unavailable.
    #[test]
    fn code_text_wraps_value_in_pill_chrome() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let out = code_text(Some(&r), "frosted");
        assert!(out.contains("frosted"));
        // None-fallback returns plain backticked text.
        let plain = code_text(None, "frosted");
        assert_eq!(plain, "`frosted`");
    }
}
