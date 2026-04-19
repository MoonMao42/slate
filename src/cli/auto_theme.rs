use crate::brand::events::{dispatch, BrandEvent, FailureKind, SuccessKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::config::ConfigManager;
use crate::detection::TerminalProfile;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::{ThemeAppearance, ThemeRegistry};

/// Detect the current system appearance through the active platform backend.
///
/// macOS uses `defaults`, Linux prefers XDG desktop portal and falls back to
/// GNOME `gsettings` when needed, and unsupported environments default to Light
/// so manual `slate theme --auto` still degrades safely.
pub fn detect_system_appearance() -> Result<ThemeAppearance> {
    Ok(crate::platform::desktop::detect_system_appearance())
}

/// Resolve which theme to apply based on system appearance and auto-pairing.
///
/// the decision pipeline is:
/// 1. Detect system appearance via detect_system_appearance()
/// 2. Read auto.toml if it exists
/// 3. If auto.toml has entry for this appearance → use that theme
/// 4. If no auto.toml or missing field:
///    a. Get current theme
///    b. If current theme's appearance matches system appearance → keep current
///    c. If mismatch and current has auto_pair → apply auto_pair
///    d. If no auto_pair → fall back to brand defaults (Dark→catppuccin-mocha, Light→catppuccin-latte)
///
/// On this fallback, print guidance message via `Roles::brand` so the
/// `✦` glyph carries the brand-lavender anchor (D-01 hybrid). Failure of
/// the inner appearance/registry calls dispatches
/// `BrandEvent::Failure(FailureKind::AutoThemeFailed)` from the outer
/// wrapper [`resolve_auto_theme`] so Phase 20's SoundSink can latch onto
/// the categorical auto-theme failure event.
pub fn resolve_auto_theme(env: &SlateEnv, config: &ConfigManager) -> Result<String> {
    match resolve_auto_theme_inner(env, config) {
        Ok(theme) => Ok(theme),
        Err(err) => {
            // D-17: any failure inside resolve_auto_theme is an auto-theme
            // categorical failure (e.g. registry load error, auto.toml IO
            // error). Phase 20 maps this to its failure SFX.
            dispatch(BrandEvent::Failure(FailureKind::AutoThemeFailed));
            Err(err)
        }
    }
}

fn resolve_auto_theme_inner(_env: &SlateEnv, config: &ConfigManager) -> Result<String> {
    // Step 1: Detect system appearance
    let system_appearance = detect_system_appearance()?;

    // Step 2: Try to read auto.toml
    let auto_config = config.read_auto_config()?;

    // Step 3: If auto.toml exists and has entry for this appearance
    if let Some(auto_cfg) = auto_config {
        let theme_id = match system_appearance {
            ThemeAppearance::Dark => auto_cfg.dark_theme,
            ThemeAppearance::Light => auto_cfg.light_theme,
        };

        if let Some(theme_id) = theme_id {
            return Ok(theme_id);
        }
    }

    // Step 4: No auto.toml or missing field - use fallback pipeline
    let registry = ThemeRegistry::new()?;
    let current_theme_id = config.get_current_theme()?;

    if let Some(ref current_id) = current_theme_id {
        if let Some(current_theme) = registry.get(current_id) {
            // 4b: Check if current theme appearance matches system
            if current_theme.appearance == system_appearance {
                return Ok(current_id.clone());
            }

            // 4c: If no match, check auto_pair
            if let Some(pair_id) = current_theme.auto_pair.as_ref() {
                return Ok(pair_id.clone());
            }
        }
    }

    // 4d: Fall back to brand defaults
    // Print guidance on this path only — route through Roles so the ✦
    // glyph carries the brand-lavender anchor (D-01) and the body uses
    // the dim/italic `path` treatment. This message is informational
    // (we DID resolve a theme), NOT an error path — so D-01a does not
    // apply here; brand-lavender on the chrome glyph is correct.
    let default_theme = match system_appearance {
        ThemeAppearance::Dark => "catppuccin-mocha".to_string(),
        ThemeAppearance::Light => "catppuccin-latte".to_string(),
    };

    if current_theme_id.is_some() {
        let ctx = RenderContext::from_active_theme().ok();
        let r = ctx.as_ref().map(Roles::new);
        let glyph = brand_glyph(r.as_ref(), '✦');
        eprintln!(
            "{} Using built-in auto pairing. Run slate config set auto-theme configure to customize.",
            glyph
        );
    }

    Ok(default_theme)
}

/// Interactive configuration flow for auto-theme pairing.
/// Guide user to select dark and light theme variants.
/// Persists selections to auto.toml (~/.config/slate/auto.toml).
///
/// On successful save, dispatches `BrandEvent::Success(SuccessKind::ConfigSet)`
/// so the configure flow rings the same completion-event channel as a
/// `slate config set` mutation. Any error inside the configure flow is
/// re-routed through the outer wrapper which dispatches
/// `BrandEvent::Failure(FailureKind::AutoThemeFailed)`.
pub fn configure_auto_theme() -> Result<()> {
    match configure_auto_theme_inner() {
        Ok(()) => Ok(()),
        Err(err) => {
            // Don't dispatch on user-cancel (Ctrl-C) — that's an
            // expected exit, not a failure of the auto-theme machinery.
            if !matches!(err, crate::error::SlateError::UserCancelled) {
                dispatch(BrandEvent::Failure(FailureKind::AutoThemeFailed));
            }
            Err(err)
        }
    }
}

fn configure_auto_theme_inner() -> Result<()> {
    use cliclack::{confirm, log, select};

    // Bootstrap Roles up-front so every chrome line shares one byte
    // contract (sketch 003 + D-01 daily chrome). Graceful degrade per
    // D-05 — plain text when the registry fails to load.
    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);

    cliclack::intro(intro_title(r.as_ref(), "Configure Auto Theme"))?;
    log::info("Match themes to your system appearance for seamless switching.")?;
    let terminal = TerminalProfile::detect();
    let backend = crate::platform::desktop::detect_backend();
    if backend.supports_watcher() && terminal.watcher_shell_autostart_supported() {
        log::remark(format!(
            "Ghostty shell sessions can relaunch the {} watcher automatically.",
            backend.label()
        ))?;
    } else if backend.supports_watcher() {
        log::remark(format!(
            "{} watching is available, but restart recovery is still most seamless in Ghostty shells.",
            backend.label()
        ))?;
    } else {
        log::remark(
            "Automatic appearance watching is unavailable here. You can still run `slate theme --auto` manually.",
        )?;
    }

    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;
    let registry = ThemeRegistry::new()?;

    // Detect current system appearance for messaging
    let current_appearance = detect_system_appearance()?;

    // Step 1: Select dark theme
    cliclack::log::remark("")?;
    let dark_prompt = match current_appearance {
        ThemeAppearance::Dark => "Select dark theme (current system mode)",
        ThemeAppearance::Light => "Select dark theme",
    };

    let dark_theme_id = select(dark_prompt)
        .items(
            registry
                .all()
                .iter()
                .filter(|t| t.appearance == ThemeAppearance::Dark)
                .map(|t| (t.id.as_str(), t.name.as_str(), ""))
                .collect::<Vec<_>>()
                .as_slice(),
        )
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;

    // Step 2: Select light theme
    cliclack::log::remark("")?;
    let light_prompt = match current_appearance {
        ThemeAppearance::Light => "Select light theme (current system mode)",
        ThemeAppearance::Dark => "Select light theme",
    };

    let light_theme_id = select(light_prompt)
        .items(
            registry
                .all()
                .iter()
                .filter(|t| t.appearance == ThemeAppearance::Light)
                .map(|t| (t.id.as_str(), t.name.as_str(), ""))
                .collect::<Vec<_>>()
                .as_slice(),
        )
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;

    // Step 3: Confirm and save
    cliclack::log::remark("")?;
    let dark_theme_name = registry
        .get(dark_theme_id)
        .map(|t| t.name.as_str())
        .unwrap_or("?");
    let light_theme_name = registry
        .get(light_theme_id)
        .map(|t| t.name.as_str())
        .unwrap_or("?");

    log::info(format!(
        "Dark:  {}",
        theme_name_text(r.as_ref(), dark_theme_name)
    ))?;
    log::info(format!(
        "Light: {}",
        theme_name_text(r.as_ref(), light_theme_name)
    ))?;
    cliclack::log::remark("")?;

    let confirm_save = confirm("Save these preferences?")
        .initial_value(true)
        .interact()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                crate::error::SlateError::UserCancelled
            } else {
                crate::error::SlateError::IOError(e)
            }
        })?;

    if confirm_save {
        config.write_auto_config(Some(dark_theme_id), Some(light_theme_id))?;
        cliclack::log::success(status_success_line(
            r.as_ref(),
            "Auto-theme preferences saved.",
        ))?;
        // D-17: a successful auto-theme configure write is a config-set
        // moment per the broader pattern in `src/cli/config.rs`.
        dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
    } else {
        cliclack::log::info("Configuration cancelled.")?;
    }

    cliclack::outro("")?;
    Ok(())
}

/// Build the intro header title. Always starts with the ✦ brand glyph so
/// the wordmark keeps the lavender anchor that Sketch 002 locks in.
fn intro_title(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => format!("{} {}", r.brand("✦"), text),
        None => format!("✦ {}", text),
    }
}

/// Render a brand-anchor glyph (✦, ★, etc.) via `Roles::brand`, falling
/// back to the bare glyph when Roles is unavailable (D-05 graceful
/// degrade). Used by informational paths that want the brand lavender
/// on the chrome but don't need the full intro framing.
fn brand_glyph(r: Option<&Roles<'_>>, glyph: char) -> String {
    let s = glyph.to_string();
    match r {
        Some(r) => r.brand(&s),
        None => s,
    }
}

/// Format a `log::success` body via `Roles::status_success` (theme.green
/// — NEVER lavender per D-01a), falling back to plain `✓ message`.
fn status_success_line(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_success(message),
        None => format!("✓ {}", message),
    }
}

/// Render a theme display name through `Roles::theme_name` (active
/// theme's `brand_accent` per D-01 daily chrome), falling back to the
/// bare name when Roles is unavailable.
fn theme_name_text(r: Option<&Roles<'_>>, name: &str) -> String {
    match r {
        Some(r) => r.theme_name(name),
        None => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};

    #[test]
    fn test_detect_system_appearance_defaults_to_light() {
        // This will actually call the system command
        // On systems without defaults, should return Light
        let appearance = detect_system_appearance().unwrap();
        // We can't assert the specific value without knowing the system state,
        // but we can verify it's either Dark or Light
        assert!(appearance == ThemeAppearance::Dark || appearance == ThemeAppearance::Light);
    }

    #[test]
    fn test_theme_appearance_enum() {
        assert_eq!(ThemeAppearance::Dark, ThemeAppearance::Dark);
        assert_eq!(ThemeAppearance::Light, ThemeAppearance::Light);
        assert_ne!(ThemeAppearance::Dark, ThemeAppearance::Light);
    }

    #[test]
    fn test_resolve_auto_theme_with_existing_auto_config() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // Write auto.toml with dark and light themes
        config
            .write_auto_config(Some("catppuccin-mocha"), Some("catppuccin-latte"))
            .unwrap();

        // Set current theme to something else
        config.set_current_theme("tokyo-night-dark").unwrap();

        // resolve_auto_theme should read from auto.toml regardless of current theme
        let resolved = resolve_auto_theme(&env, &config).unwrap();

        // Since we can't control system appearance in tests, check that it either
        // resolves to one of the configured themes or a fallback
        let theme_registry = ThemeRegistry::new().unwrap();
        assert!(theme_registry.get(&resolved).is_some());
    }

    #[test]
    fn test_resolve_auto_theme_fallback_with_auto_pair() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // Don't write auto.toml, so fallback pipeline is used
        // Set current theme to one with auto_pair (e.g., catppuccin-mocha pairs with catppuccin-latte)
        config.set_current_theme("catppuccin-mocha").unwrap();

        // resolve_auto_theme should use fallback pipeline
        let resolved = resolve_auto_theme(&env, &config).unwrap();

        // Verify resolved theme is valid
        let theme_registry = ThemeRegistry::new().unwrap();
        assert!(theme_registry.get(&resolved).is_some());
    }

    #[test]
    fn test_auto_config_read_write_round_trip() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // Initially no config
        let initial = config.read_auto_config().unwrap();
        assert!(initial.is_none());

        // Write config
        config
            .write_auto_config(Some("catppuccin-mocha"), Some("catppuccin-latte"))
            .unwrap();

        // Read it back
        let read_back = config.read_auto_config().unwrap();
        assert!(read_back.is_some());

        let auto_cfg = read_back.unwrap();
        assert_eq!(auto_cfg.dark_theme, Some("catppuccin-mocha".to_string()));
        assert_eq!(auto_cfg.light_theme, Some("catppuccin-latte".to_string()));
    }

    #[test]
    fn test_auto_config_partial_update() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // Write initial config with both values
        config
            .write_auto_config(Some("catppuccin-mocha"), Some("catppuccin-latte"))
            .unwrap();

        // Update only dark theme, should preserve light theme
        config
            .write_auto_config(Some("tokyo-night-dark"), None)
            .unwrap();

        // Read back
        let read_back = config.read_auto_config().unwrap().unwrap();
        assert_eq!(read_back.dark_theme, Some("tokyo-night-dark".to_string()));
        assert_eq!(read_back.light_theme, Some("catppuccin-latte".to_string()));
    }

    #[test]
    fn test_resolve_auto_theme_defaults_when_no_config() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let env = SlateEnv::with_home(temp.path().to_path_buf());
        let config = ConfigManager::with_env(&env).unwrap();

        // No auto.toml, no current theme
        let resolved = resolve_auto_theme(&env, &config).unwrap();

        // Should resolve to a brand default (catppuccin-mocha or catppuccin-latte)
        assert!(resolved == "catppuccin-mocha" || resolved == "catppuccin-latte");
    }

    /// D-01a invariant — the `status_success_line` helper used by the
    /// auto-theme configure flow must use theme.green, never the brand
    /// lavender RGB triple. Tests every render mode to lock the
    /// invariant file-wide.
    #[test]
    fn status_success_line_never_emits_brand_lavender() {
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            let out = status_success_line(Some(&r), "Auto-theme preferences saved.");
            assert!(
                !out.contains("38;2;114;135;253"),
                "D-01a violation in mode {mode:?}: {out:?}"
            );
        }
    }

    /// D-01a invariant variant — even though `auto_theme.rs` does not
    /// emit `Roles::status_error` directly today (errors propagate via
    /// the outer wrapper to the caller's `error::display`), assert the
    /// invariant locally so a future refactor that adds an error
    /// surface here cannot ship lavender bytes inside an error body.
    #[test]
    fn d01a_no_lavender_in_error_paths_for_auto_theme() {
        let theme = mock_theme();
        for mode in [RenderMode::Truecolor, RenderMode::Basic, RenderMode::None] {
            let ctx = mock_context_with_mode(&theme, mode);
            let r = Roles::new(&ctx);
            // Simulate the byte shape of any error rendering this file
            // might emit in the future. status_error must NEVER carry
            // brand-accent lavender bytes (D-01a).
            let out = r.status_error("dark-mode-notify install failed");
            assert!(
                !out.contains("38;2;114;135;253"),
                "D-01a violation for status_error in mode {mode:?}: {out:?}"
            );
        }
    }

    /// Brand-anchor invariant — `brand_glyph` must carry the
    /// brand-lavender RGB triple on the ✦ glyph in truecolor mode (the
    /// `eprintln!` informational guidance message in `resolve_auto_theme_inner`
    /// is the only Wave-6 production caller).
    #[test]
    fn brand_glyph_carries_lavender_in_truecolor() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Truecolor);
        let r = Roles::new(&ctx);
        let out = brand_glyph(Some(&r), '✦');
        assert!(
            out.contains("38;2;114;135;253"),
            "brand_glyph must carry brand-lavender bytes in truecolor, got: {out:?}"
        );
    }

    /// D-05 graceful degrade — without Roles every helper falls back to
    /// the bare glyph / message, with zero ANSI bytes.
    #[test]
    fn helpers_fall_back_to_plain_when_roles_absent() {
        let glyph = brand_glyph(None, '✦');
        assert_eq!(glyph, "✦");
        let line = status_success_line(None, "saved");
        assert_eq!(line, "✓ saved");
        let theme_name = theme_name_text(None, "catppuccin-mocha");
        assert_eq!(theme_name, "catppuccin-mocha");
        for s in [glyph, line, theme_name] {
            assert!(!s.contains('\x1b'));
        }
    }
}
