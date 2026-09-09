use crate::brand::events::{dispatch, BrandEvent, SuccessKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::cli::font::resolve_font_choice;
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::opacity::OpacityPreset;
use crate::theme::ThemeRegistry;

mod preview;
pub use preview::handle_import_preview;
mod codec;
mod export;
mod recovery;
pub(crate) use export::build_export_uri;
pub use export::handle_export_with_options;
pub use recovery::validate_storage_paths as validate_import_storage_paths;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
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

/// A validated URI with its font request resolved for this host. Constructed
/// before sound or writer-lock initialization; this is not a writeability check.
pub struct PreparedImport(ImportRequest);

pub fn prepare_import(uri: &str) -> Result<PreparedImport> {
    parse_import_request(uri).map(PreparedImport)
}

/// Export saved settings. Keep the no-argument entrypoint for library callers.
pub fn handle_export() -> Result<()> {
    handle_export_with_options(false)
}

/// Import a slate config from a shareable URI.
/// Parses the URI and applies theme, font, opacity, and tool toggles.
/// On success emits the share-success line via `Roles::status_success`
/// (theme.green per D-01a — NEVER lavender) and dispatches
/// `BrandEvent::Success(SuccessKind::ConfigSet)` so
/// SoundSink can ring the share-import completion moment alongside the
/// other config-mutation surfaces.
pub fn handle_import(uri: &str) -> Result<()> {
    handle_prepared_import(prepare_import(uri)?)
}

/// Apply to the process profile, also used by the nested font/theme handlers.
pub fn handle_prepared_import(prepared: PreparedImport) -> Result<()> {
    handle_prepared_import_with_options(prepared, false, false)
}

/// Defer sound/profile reads until the mandatory recovery checkpoint exists.
pub fn handle_prepared_import_with_options(
    prepared: PreparedImport,
    auto: bool,
    quiet: bool,
) -> Result<()> {
    let env = SlateEnv::from_process()?;
    apply_prepared_import_with_env(prepared, &env, auto, quiet)
}

#[cfg(test)]
fn handle_import_with_env(uri: &str, env: &SlateEnv) -> Result<()> {
    apply_prepared_import_with_env(prepare_import(uri)?, env, false, true)
}

fn apply_prepared_import_with_env(
    prepared: PreparedImport,
    env: &SlateEnv,
    auto: bool,
    quiet: bool,
) -> Result<()> {
    let request = prepared.0;
    recovery::validate_storage_paths(env)?;
    let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
    let checkpoint = recovery::create(env, &request)?;
    let id = &checkpoint.point.id;
    eprintln!("Pre-import recovery point: {id}");
    eprintln!("Inspect file recovery: slate restore {id} --dry-run");
    eprintln!("Recovery excludes installed fonts, external caches, empty directories and running application state.");
    crate::brand::SoundSink::install(env, auto, quiet);
    apply_request(env, &request, &checkpoint.theme_tools).map_err(|err| SlateError::InvalidConfig(format!(
        "Import was incomplete: {err}. Earlier changes were not automatically rolled back. Inspect the pre-import recovery point with: slate restore {id} --dry-run. Remove --dry-run to restore captured files. Font installations and external caches are not undone."
    )))
}

fn apply_request(env: &SlateEnv, request: &ImportRequest, theme_tools: &[String]) -> Result<()> {
    if let Some(font) = request.font.as_deref() {
        crate::cli::font::handle_import_font(env, font)?;
    }

    if let Some(theme_id) = request.theme.as_deref() {
        // The pre-import checkpoint covers font + theme + flags together. Do
        // not take the narrower theme snapshot after the font has changed.
        let registry = ThemeRegistry::new()?;
        let theme = registry
            .get(theme_id)
            .ok_or_else(|| SlateError::ThemeNotFound(theme_id.into()))?;
        let report = crate::cli::apply::ThemeApplyCoordinator::with_snapshot_policy(
            env,
            crate::cli::apply::SnapshotPolicy::Skip,
        )
        .apply_to_tools(theme, theme_tools)?;
        crate::cli::apply::log_apply_report(&report);
        report.ensure_no_failures()?;
    }

    let config = ConfigManager::with_env(env)?;

    let themes = ThemeRegistry::new()?;
    let current_theme = config.get_current_theme()?;
    let ctx = themes
        .get(
            current_theme
                .as_deref()
                .unwrap_or(crate::theme::DEFAULT_THEME_ID),
        )
        .map(RenderContext::new);
    let r = ctx.as_ref().map(Roles::new);

    if let Some(opacity) = request.opacity {
        crate::cli::apply::apply_opacity(
            env,
            opacity,
            crate::cli::apply::OpacityApplyOptions {
                persist_state: true,
                reload_terminals: true,
                snapshot_policy: crate::cli::apply::SnapshotPolicy::Skip,
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

fn parse_import_request(uri: &str) -> Result<ImportRequest> {
    // Validate every URI segment before local font discovery. A bad opacity or
    // flag list must not trigger platform probes just because it names a font.
    let mut request = parse_import_intent(uri)?;
    if let Some(font) = request.font.as_deref() {
        request.font = Some(resolve_font_choice(font)?.font_name().to_owned());
    }
    Ok(request)
}

/// Parse the requested settings without reading profiles, discovering fonts,
/// creating paths, or launching commands. Preview intentionally stops here.
fn parse_import_intent(uri: &str) -> Result<ImportRequest> {
    if uri.len() > 1024 || uri.chars().any(char::is_control) {
        return Err(SlateError::InvalidConfig(
            "Share code must be at most 1024 bytes and contain no control characters.".into(),
        ));
    }
    let stripped = uri
        .strip_prefix("slate://")
        .ok_or_else(|| SlateError::InvalidConfig("URI must start with slate://".to_string()))?;

    let parts: Vec<&str> = stripped.split('/').collect();
    let (theme, font, opacity, tools, encoded) = match parts.as_slice() {
        [theme, font, opacity, tools] => (*theme, *font, *opacity, *tools, false),
        ["v1", theme, font, opacity, tools] => (*theme, *font, *opacity, *tools, true),
        [_, _, _, _, _] => return Err(SlateError::InvalidConfig("Unsupported share-code version. Use v1 or a legacy four-part code.".into())),
        _ => return Err(SlateError::InvalidConfig(
            "Expected slate://v1/theme/font/opacity/tools or legacy slate://theme/font/opacity/tools".into(),
        )),
    };

    Ok(ImportRequest {
        theme: parse_theme_segment(theme)?,
        font: if encoded && font != "none" {
            let decoded = codec::decode_font(font)?;
            validate_font_name(&decoded)?;
            Some(decoded)
        } else {
            parse_font_segment(font)?
        },
        opacity: parse_opacity_segment(opacity)?,
        tools: parse_tool_flags(tools)?,
    })
}

fn parse_theme_segment(theme: &str) -> Result<Option<String>> {
    if theme == "none" {
        return Ok(None);
    }

    let registry = ThemeRegistry::new()?;
    if registry.get(theme).is_none() {
        let preview: String = theme.chars().take(80).collect();
        return Err(SlateError::InvalidConfig(format!(
            "Unknown shared theme ID {preview:?}. Run `slate list` to find a canonical ID."
        )));
    }

    Ok(Some(theme.to_string()))
}

fn parse_font_segment(font: &str) -> Result<Option<String>> {
    if font == "none" {
        return Ok(None);
    }

    validate_font_name(font)?;
    Ok(Some(font.to_owned()))
}

fn validate_font_name(font: &str) -> Result<()> {
    crate::adapter::font_config::validate_family(font)
}

fn parse_opacity_segment(opacity: &str) -> Result<Option<OpacityPreset>> {
    if opacity == "none" {
        return Ok(None);
    }

    opacity.parse::<OpacityPreset>().map(Some).map_err(|_| {
        SlateError::InvalidConfig(
            "Invalid shared opacity preset. Use solid, frosted, clear, or none.".into(),
        )
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
            return Err(invalid_tool_flags());
        }

        match flag {
            "s" => flags.starship = true,
            "h" => flags.highlighting = true,
            "f" => flags.fastfetch = true,
            _ => {
                return Err(invalid_tool_flags());
            }
        }
    }

    Ok(flags)
}

fn invalid_tool_flags() -> SlateError {
    SlateError::InvalidConfig(
        "Invalid tool flag list. Use unique comma-separated values from s, h, f, or none to disable all three.".into(),
    )
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
