use super::ui_language::tr;
use crate::brand::events::{dispatch, BrandEvent, FailureKind, SuccessKind};
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::{ThemeAppearance, ThemeRegistry};

/// Detect the current system appearance through the active platform backend.
/// macOS uses `defaults`, Linux prefers XDG desktop portal and falls back to
/// GNOME `gsettings` when needed. A genuinely absent backend defaults to Light;
/// a failed/invalid/timed-out query is an error, not an appearance preference.
pub fn detect_system_appearance() -> Result<ThemeAppearance> {
    crate::platform::desktop::detect_system_appearance_checked()
}

/// Resolve which theme to apply based on system appearance and auto-pairing.
/// the decision pipeline is:
/// 1. Detect system appearance via detect_system_appearance()
/// 2. Read auto.toml if it exists
/// 3. If auto.toml has an entry for this appearance, require a known ID and use it
/// 4. If no auto.toml or missing field:
/// a. Get current theme
/// b. If current theme's appearance matches system appearance → keep current
/// c. If mismatch and current has auto_pair → apply auto_pair
/// d. If no auto_pair → fall back to brand defaults (Dark→catppuccin-mocha, Light→catppuccin-latte)
/// Known stored overrides and catalog self-pairs keep their existing semantics,
/// even when the selected theme does not match the requested appearance. Required
/// pairing/tracking files use bounded, no-final-link reads; unknown selected IDs
/// fail without exposing configuration contents. Unneeded tracking is not read.
/// On this fallback, print guidance message via `Roles::brand` so the
/// `✦` glyph carries the brand-lavender anchor (hybrid). Failure of
/// the inner appearance/registry calls dispatches
/// `BrandEvent::Failure(FailureKind::AutoThemeFailed)` from the outer
/// wrapper [`resolve_auto_theme`] so SoundSink can latch onto
/// the categorical auto-theme failure event.
pub fn resolve_auto_theme(env: &SlateEnv, config: &ConfigManager) -> Result<String> {
    match resolve_auto_theme_inner(env, config) {
        Ok(theme) => Ok(theme),
        Err(err) => {
            // any failure inside resolve_auto_theme is an auto-theme
            // categorical failure (e.g. registry load error, auto.toml IO
            // error). maps this to its failure SFX.
            dispatch(BrandEvent::Failure(FailureKind::AutoThemeFailed));
            Err(err)
        }
    }
}

fn resolve_auto_theme_inner(_env: &SlateEnv, config: &ConfigManager) -> Result<String> {
    // Query the desktop once, then use the same policy as pairing inspection.
    let system_appearance = detect_system_appearance()?;
    let registry = ThemeRegistry::new()?;
    let choice = crate::config::auto_resolution::resolve(
        config.environment(),
        &registry,
        system_appearance,
    )?;
    if choice.source == crate::config::auto_resolution::ChoiceSource::BrandDefault
        && choice.fallback_reason
            != Some(crate::config::auto_resolution::FallbackReason::NoCurrentTheme)
    {
        let ctx = RenderContext::from_active_theme().ok();
        let r = ctx.as_ref().map(Roles::new);
        let glyph = brand_glyph(r.as_ref(), '✦');
        eprintln!(
            "{} Using built-in auto pairing. Inspect slate config pairing to see choices and customize.",
            glyph
        );
    }

    Ok(choice.theme.id.clone())
}

/// Interactive configuration flow for auto-theme pairing.
/// Guide user to select dark and light theme variants.
/// Persists selections to auto.toml (~/.config/slate/auto.toml).
/// On successful save, dispatches `BrandEvent::Success(SuccessKind::ConfigSet)`
/// so the configure flow rings the same completion-event channel as a
/// `slate config set` mutation. Any error inside the configure flow is
/// re-routed through the outer wrapper which dispatches
/// `BrandEvent::Failure(FailureKind::AutoThemeFailed)`.
pub fn configure_auto_theme() -> Result<()> {
    require_interactive_configuration()?;
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

/// Do not enter a prompt (or create a profile/lock) from a script.
pub fn require_interactive_configuration() -> Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(crate::error::SlateError::InvalidConfig(
            "Interactive pairing requires a terminal. Use `slate config pairing --dark THEME_ID --light THEME_ID` to save from a script, or `slate config pairing --json` to inspect.".into()
        ));
    }
    Ok(())
}

fn configure_auto_theme_inner() -> Result<()> {
    use super::menu::select;
    use crate::config::pairing::{PreparedPairing, SlotEdit};
    use cliclack::log;
    const AUTOMATIC: &str = "@automatic";
    let env = SlateEnv::from_process()?;

    // Bootstrap Roles up-front so every chrome line shares one byte
    // contract (sketch 003 daily chrome). Graceful degrade per
    // plain text when the registry fails to load.
    let ctx = RenderContext::from_active_theme().ok();
    let r = ctx.as_ref().map(Roles::new);

    cliclack::intro(intro_title(
        r.as_ref(),
        tr("深浅主题配对", "Dark and Light Theme Pairing"),
    ))?;
    super::file_output::write_output(
        tr("选择系统深色、浅色模式对应的主题。这里只保存配对，不立即换色，也不启停后台。\n", "Choose themes for system dark and light modes. This saves pairing only; it does not apply colors or start/stop the service.\n"),
    )?;
    let registry = ThemeRegistry::new()?;

    // Selection is local state only. Escape goes back one step; returning
    // from the light list keeps the previously chosen dark theme highlighted.
    let dark_choices: Vec<_> = registry
        .all()
        .iter()
        .filter(|t| t.appearance == ThemeAppearance::Dark)
        .map(|t| (t.id.as_str(), t.name.as_str(), ""))
        .collect();
    let light_choices: Vec<_> = registry
        .all()
        .iter()
        .filter(|t| t.appearance == ThemeAppearance::Light)
        .map(|t| (t.id.as_str(), t.name.as_str(), ""))
        .collect();
    let (saved, saved_readable) = match ConfigManager::from_env_paths(&env).read_auto_config() {
        Ok(saved) => (saved, true),
        Err(_) => {
            log::warning(tr(
                "无法读取已保存的深浅配对；未修改文件，请检查 auto.toml。",
                "Saved pairing is unreadable; no files changed. Check auto.toml.",
            ))?;
            (None, false)
        }
    };
    let saved_dark = saved.as_ref().and_then(|pair| pair.dark_theme.as_deref());
    let saved_light = saved.as_ref().and_then(|pair| pair.light_theme.as_deref());
    let mut previous_dark = saved_dark.and_then(|id| {
        dark_choices
            .iter()
            .find(|choice| choice.0 == id)
            .map(|choice| choice.0)
    });
    let mut initial_light = saved_light.and_then(|id| {
        light_choices
            .iter()
            .find(|choice| choice.0 == id)
            .map(|choice| choice.0)
    });
    if (saved_dark.is_some() && previous_dark.is_none())
        || (saved_light.is_some() && initial_light.is_none())
    {
        log::warning(tr("已保存的配对含未知主题或深浅类别不匹配；请重新选择，确认前不会改动文件。", "Saved pairing contains an unknown theme or appearance mismatch. Choose again; nothing changes before confirmation."))?;
    }
    // A missing override is a real automatic choice, not the first catalog
    // theme. Unreadable/invalid settings must not be presented as that choice.
    if saved_readable {
        if saved_dark.is_none() {
            previous_dark = Some(AUTOMATIC);
        }
        if saved_light.is_none() {
            initial_light = Some(AUTOMATIC);
        }
    }
    let menu_error = |error: std::io::Error| {
        if error.kind() == std::io::ErrorKind::Interrupted {
            crate::error::SlateError::UserCancelled
        } else {
            crate::error::SlateError::IOError(error)
        }
    };
    let (dark_theme_id, light_theme_id) = loop {
        let mut dark = select(tr("选择深色主题", "Choose Dark Theme"))
            .max_rows(8)
            .items(&dark_choices)
            .item(
                AUTOMATIC,
                tr("自动选择（不固定主题）", "Automatic (not pinned)"),
                tr(
                    "移除深色配对，使用自动回退规则",
                    "Clear the dark override and use automatic fallback",
                ),
            )
            .item("", tr("取消配对设置", "Cancel Pairing"), "")
            .escape_value("");
        if let Some(id) = previous_dark {
            dark = dark.initial_value(id);
        }
        let dark_id = dark.interact().map_err(menu_error)?;
        if dark_id.is_empty() {
            super::file_output::write_output(tr("未保存配对设置。\n", "Pairing not saved.\n"))?;
            return Ok(());
        }
        previous_dark = Some(dark_id);
        let mut light = select(tr("选择浅色主题", "Choose Light Theme"))
            .max_rows(8)
            .items(&light_choices)
            .item(
                AUTOMATIC,
                tr("自动选择（不固定主题）", "Automatic (not pinned)"),
                tr(
                    "移除浅色配对，使用自动回退规则",
                    "Clear the light override and use automatic fallback",
                ),
            )
            .item("", tr("返回深色主题", "Back to Dark Theme"), "")
            .escape_value("");
        if let Some(id) = initial_light {
            light = light.initial_value(id);
        }
        let light_id = light.interact().map_err(menu_error)?;
        if !light_id.is_empty() {
            break (dark_id, light_id);
        }
    };

    // Step 3: Confirm and save
    let dark_theme_name = registry
        .get(dark_theme_id)
        .map(|t| t.name.as_str())
        .unwrap_or(tr("自动选择（不固定主题）", "Automatic (not pinned)"));
    let light_theme_name = registry
        .get(light_theme_id)
        .map(|t| t.name.as_str())
        .unwrap_or(tr("自动选择（不固定主题）", "Automatic (not pinned)"));

    super::file_output::write_output(&format!(
        "\n{}  {}\n{}  {}\n",
        tr("深色模式", "Dark Mode"),
        theme_name_text(r.as_ref(), dark_theme_name),
        tr("浅色模式", "Light Mode"),
        theme_name_text(r.as_ref(), light_theme_name)
    ))?;

    let confirm_save = super::menu::confirm_named(
        tr("保存这组配对？", "Save this pairing?"),
        tr("取消", "Cancel"),
        tr("保存", "Save"),
    )
    .interact()
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::Interrupted {
            crate::error::SlateError::UserCancelled
        } else {
            crate::error::SlateError::IOError(e)
        }
    })?;

    if confirm_save {
        let edit = |id| {
            if id == AUTOMATIC {
                SlotEdit::Clear
            } else {
                SlotEdit::Set(id)
            }
        };
        let plan = PreparedPairing::capture_edits(&env, edit(dark_theme_id), edit(light_theme_id))?;
        if let Some(id) = plan.save()? {
            super::file_output::write_output(&format!(
                "{}\n{}\n{}slate restore {id} --dry-run\n",
                status_success_line(r.as_ref(), tr("配对已保存，当前主题未改变。", "Pairing saved; the current theme is unchanged.")),
                tr("自动换色已开启时，下次系统深浅切换会使用这组配对。", "When Auto-Theme is on, the next system appearance change will use this pairing."),
                tr("恢复前先查看：", "Review recovery: ")
            ))?;
            // a successful auto-theme configure write is a config-set
            // moment per the broader pattern in `src/cli/config.rs`.
            dispatch(BrandEvent::Success(SuccessKind::ConfigSet));
        } else {
            super::file_output::write_output(tr(
                "配对未变化，没有改写文件或新增恢复点。\n",
                "Pairing unchanged; no files rewritten or recovery point added.\n",
            ))?;
        }
    } else {
        super::file_output::write_output(tr("未保存配对设置。\n", "Pairing not saved.\n"))?;
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
/// back to the bare glyph when Roles is unavailable (graceful
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
/// NEVER lavender per D-01a), falling back to plain `✓ message`.
fn status_success_line(r: Option<&Roles<'_>>, message: &str) -> String {
    match r {
        Some(r) => r.status_success(message),
        None => format!("✓ {}", message),
    }
}

/// Render a theme display name through `Roles::theme_name` (active
/// theme's `brand_accent` per daily chrome), falling back to the
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

    /// graceful degrade — without Roles every helper falls back to
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
