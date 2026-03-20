use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::brand::Language;
use crate::env::SlateEnv;
use crate::error::Result;

/// Handle `slate set` compatibility alias using structured clap arguments.
/// Routes to the noun-driven theme surface while preserving the current CLI:
/// 1. `slate set <theme>` → `slate theme <theme>` + dim tip
/// 2. `slate set --auto` → `slate theme --auto`
/// 3. `slate set` → theme picker + dim tip
pub fn handle(theme_name: Option<&str>, auto: bool) -> Result<()> {
    if auto {
        crate::cli::theme::handle_theme(None, true, false)?;
        return Ok(());
    }

    if let Some(theme_arg) = theme_name {
        crate::cli::theme::handle_theme(Some(theme_arg.to_string()), false, false)?;

        print_dim_tip();
        Ok(())
    } else {
        let env = SlateEnv::from_process()?;
        crate::cli::picker::launch_picker(&env)?;

        print_dim_tip();
        Ok(())
    }
}

/// Print a dim tip teaching users about the new `slate theme` surface.
/// Rendered through `Roles::path` so the byte contract matches the rest
/// of the surfaces (dim + italic). graceful degrade — when
/// the theme registry fails to boot we fall back to plain text.
fn print_dim_tip() {
    println!();
    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);
    println!("{}", format_dim_tip(roles.as_ref()));
}

/// Pure formatter for the `slate set` deprecation tip — takes an
/// optional `&Roles` so snapshot tests can drive it directly without a
/// live registry. Matches the graceful-degrade pattern used in
/// surfaces.
fn format_dim_tip(r: Option<&Roles<'_>>) -> String {
    match r {
        Some(r) => r.path(Language::SLATE_SET_DEPRECATION_TIP),
        None => format!("  {}", Language::SLATE_SET_DEPRECATION_TIP),
    }
}

/// Silent preview apply: updates only the live preview path without persisting theme/opacity state.
/// Called on every keystroke during picker navigation. Updates visual appearance
/// without committing the selection to ~/.config/slate/current and current-opacity.
/// This function:
/// 1. Does NOT write current/current-opacity files
/// 2. Updates terminal opacity/blur via adapters (Ghostty, Alacritty)
/// 3. Applies theme palette to adapters for visual preview
/// 4. Attempts Ghostty live reload with permission-aware behavior
/// 5. Produces NO stdout output (silent)
pub fn silent_preview_apply(
    env: &SlateEnv,
    theme_id: &str,
    opacity: crate::opacity::OpacityPreset,
) -> Result<()> {
    let registry = crate::theme::ThemeRegistry::new()?;
    let theme = registry.get(theme_id).ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id))
    })?;

    crate::cli::apply::preview_theme(env, theme, opacity)
}

/// Silent commit apply: persists theme/opacity state, then performs full apply.
/// Called on Enter key in picker. Commits the selection to persistent state,
/// then runs the full apply path including reload signals.
/// This function:
/// 1. Writes current file (theme ID)
/// 2. Writes current-opacity file (opacity preset)
/// 3. Applies theme palette to all adapters
/// 4. Updates opacity/blur configs
/// 5. Sends SIGUSR2 to Ghostty for hot-reload
/// 6. Produces NO stdout output (silent, Afterglow receipt printed by caller)
pub fn silent_commit_apply(
    env: &SlateEnv,
    theme_id: &str,
    opacity: crate::opacity::OpacityPreset,
) -> Result<()> {
    let registry = crate::theme::ThemeRegistry::new()?;

    let theme = registry.get(theme_id).ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id))
    })?;

    crate::cli::apply::ThemeApplyCoordinator::new(env).apply(theme)?;
    crate::cli::apply::apply_opacity(
        env,
        opacity,
        crate::cli::apply::OpacityApplyOptions {
            persist_state: true,
            reload_terminals: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::format_dim_tip;
    use crate::brand::render_context::{mock_context_with_mode, mock_theme, RenderMode};
    use crate::brand::roles::Roles;

    /// snapshot — the `slate set` deprecation tip rendered
    /// through `Roles::path` (dim + italic) in Basic mode. Byte-locked
    /// so future Language::SLATE_SET_DEPRECATION_TIP copy changes are
    /// visible in review.
    #[test]
    fn set_deprecation_tip_basic_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let out = format_dim_tip(Some(&r));
        insta::assert_snapshot!("set_deprecation_tip_basic", out);
    }

    /// graceful degrade — without Roles the tip falls back to
    /// plain text with 2-space indent (matches the legacy
    /// `Typography::explanation` indent contract so the tip doesn't
    /// jump positions when the registry is unreadable).
    #[test]
    fn set_deprecation_tip_falls_back_to_plain_when_roles_absent() {
        let out = format_dim_tip(None);
        assert!(!out.contains('\x1b'), "expected plain text, got: {out:?}");
        assert!(out.starts_with("  "), "expected 2-space indent: {out:?}");
    }
}
