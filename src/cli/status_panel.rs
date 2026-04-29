use crate::adapter::palette_renderer::PaletteRenderer;
use crate::adapter::registry::ToolRegistry;
use crate::brand::render_context::RenderContext;
use crate::brand::roles::Roles;
use crate::brand::Language;
use crate::config::ConfigManager;
use crate::detection::{TerminalFeatureSummary, TerminalProfile};
use crate::error::Result;
use crate::platform::capabilities::{detect_capabilities, CapabilityReport, CapabilitySnapshot};
use crate::theme::{Palette, ThemeRegistry, ThemeVariant};

/// Tool installation status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolStatus {
    Themed,       // ✓ Themed
    NotInstalled, // ✗ Not installed
}

const TOOL_STATUS_ITEMS: [(&str, &str); 12] = [
    ("ghostty", "ghostty"),
    ("alacritty", "alacritty"),
    ("kitty", "kitty"),
    ("starship", "starship"),
    ("bat", "bat"),
    ("delta", "delta"),
    ("eza", "eza"),
    ("lazygit", "lazygit"),
    ("fastfetch", "fastfetch"),
    ("zsh-syntax-highlighting", "zsh-highlight"),
    ("tmux", "tmux"),
    ("nerd-font", "nerd-font"),
];

fn get_auto_theme_status(config: &ConfigManager, terminal: &TerminalProfile) -> String {
    let enabled = config.is_auto_theme_enabled().unwrap_or(false);
    let running = crate::platform::dark_mode_notify::is_running().unwrap_or(false);
    let backend = crate::platform::desktop::detect_backend();
    let capability = crate::platform::desktop::capability_report();

    auto_theme_status_text(
        enabled,
        running,
        terminal,
        backend.label(),
        capability.reason.as_deref(),
    )
}

fn auto_theme_status_text(
    enabled: bool,
    running: bool,
    terminal: &TerminalProfile,
    backend_label: &str,
    backend_reason: Option<&str>,
) -> String {
    let status = match (enabled, running) {
        (true, true) => Language::STATUS_AUTO_WATCHER_RUNNING.to_string(),
        (true, false) if terminal.watcher_shell_autostart_supported() => {
            Language::STATUS_AUTO_WATCHER_IDLE_GHOSTTY.to_string()
        }
        (true, false) => Language::STATUS_AUTO_WATCHER_IDLE_OTHER.to_string(),
        (false, true) => Language::STATUS_AUTO_WATCHER_DRIFT.to_string(),
        (false, false) => Language::STATUS_AUTO_WATCHER_DISABLED.to_string(),
    };

    match backend_reason {
        Some(reason) => format!("{} — {} ({})", status, backend_label, reason),
        None => format!("{} — {}", status, backend_label),
    }
}

fn terminal_support_line(terminal: &TerminalProfile) -> String {
    format!(
        "{} — {}",
        terminal.compatibility_label(),
        terminal.short_limitations()
    )
}

fn capability_items(snapshot: &CapabilitySnapshot) -> [(&'static str, &CapabilityReport); 8] {
    [
        ("OS", &snapshot.os),
        ("Arch", &snapshot.arch),
        ("Shell", &snapshot.shell),
        ("Package Manager", &snapshot.package_manager),
        ("Desktop Appearance", &snapshot.desktop_appearance),
        ("Share Capture", &snapshot.share_capture),
        ("Font Platform", &snapshot.font_platform),
        ("Terminal", &snapshot.terminal),
    ]
}

fn capability_row_text(name: &str, report: &CapabilityReport) -> String {
    format!("{}  {} · {}", name, report.level.label(), report.backend)
}

/// Render the status dashboard
pub fn render() -> Result<()> {
    let config = ConfigManager::new()?;
    let registry = ThemeRegistry::new()?;

    // Get current state
    let current_theme = config
        .get_current_theme()?
        .and_then(|id| registry.get(&id).cloned())
        .unwrap_or_else(|| registry.get("catppuccin-mocha").unwrap().clone());
    let current_font = config
        .get_current_font()?
        .unwrap_or_else(|| "Not configured".to_string());
    let current_opacity = config
        .get_current_opacity_preset()
        .map(|p| p.to_string())
        .unwrap_or_else(|_| "Solid".to_string());
    let terminal = TerminalProfile::detect();
    let terminal_features = terminal.feature_summary();
    let capabilities = detect_capabilities();
    let adapter_status = get_adapter_statuses()?;
    let auto_theme_status = get_auto_theme_status(&config, &terminal);

    // graceful degrade: fall back to plain chrome when the registry
    // cannot resolve an active theme. Roles methods accept `Option<&Roles>`
    // via the `format_status_panel` seam below.
    let ctx = RenderContext::from_active_theme().ok();
    let roles = ctx.as_ref().map(Roles::new);

    print!(
        "{}",
        format_status_panel(
            roles.as_ref(),
            &current_theme,
            &current_font,
            &current_opacity,
            &terminal,
            &terminal_features,
            &capabilities,
            &adapter_status,
            &auto_theme_status,
        )
    );

    Ok(())
}

/// Pure formatter for the `slate status` panel body.
/// Takes `Option<&Roles>` so snapshot tests can inject a MockTheme-backed
/// `Roles` without a live registry , and so the graceful
/// degrade path can render plain chrome when the theme registry is
/// unreadable. Returns the full multi-line panel as a single `String`
/// (leading and trailing blank lines included) so callers can `print!`
/// it in one shot and snapshot tests can byte-lock the layout.
/// The 9-argument surface mirrors the panel's data dependencies 1:1
/// rolling them into a `StatusPanelView` struct would add a layer of
/// indirection without any reuse benefit (single call site in
/// `render()`, three snapshot tests).
#[allow(clippy::too_many_arguments)]
fn format_status_panel(
    r: Option<&Roles<'_>>,
    theme: &ThemeVariant,
    font: &str,
    opacity: &str,
    terminal: &TerminalProfile,
    features: &TerminalFeatureSummary,
    capabilities: &CapabilitySnapshot,
    adapters: &[(String, ToolStatus)],
    auto_theme_status: &str,
) -> String {
    let mut out = String::with_capacity(2048);

    // Leading blank line.
    out.push('\n');

    // Rounded panel header — the ✦ logo glyph is a brand anchor
    // rendered through `Roles::brand` so it carries the fixed
    // `BRAND_LAVENDER_FIXED` byte sequence in truecolor mode.
    out.push_str(&format!(
        " ╭─ {} slate status ─────────────────────────────────────────╮\n",
        brand_glyph(r, "✦")
    ));

    // Section 1 — Core Vibe
    out.push_str(" │\n");
    out.push_str(&format!(" │  {}\n", section_heading(r, "Core Vibe")));
    out.push_str(" │    ");
    out.push_str(&render_color_blocks(&theme.palette));
    out.push_str(&format!(" {}\n", theme_name(r, &theme.name)));
    out.push_str(&format!(" │    {}\n", dim_text(r, &theme.family)));

    // Section 2 — Typography
    out.push_str(" │\n");
    out.push_str(&format!(" │  {}\n", section_heading(r, "Typography")));
    out.push_str(&format!(" │    {}\n", dim_text(r, font)));

    // Section 3 — Background
    out.push_str(" │\n");
    out.push_str(&format!(" │  {}\n", section_heading(r, "Background")));
    out.push_str(&format!(
        " │    Terminal  {}\n",
        dim_text(r, terminal.display_name())
    ));
    out.push_str(&format!(
        " │    Support   {}\n",
        dim_text(r, &terminal_support_line(terminal))
    ));
    out.push_str(&format!(
        " │    Reload   {}\n",
        code_text(r, &features.reload)
    ));
    out.push_str(&format!(
        " │    Preview  {}\n",
        code_text(r, &features.live_preview)
    ));
    out.push_str(&format!(
        " │    Font     {}\n",
        code_text(r, &features.font_apply)
    ));
    out.push_str(&format!(" │    Opacity  {}\n", code_text(r, opacity)));

    // Section 4 — Shared platform capabilities
    out.push_str(" │\n");
    out.push_str(&format!(
        " │  {}\n",
        section_heading(r, Language::STATUS_PLATFORM_CAPABILITIES)
    ));
    for (label, report) in capability_items(capabilities) {
        out.push_str(&format!(
            " │    {}\n",
            dim_text(r, &capability_row_text(label, report))
        ));
        if let Some(reason) = report.reason.as_deref() {
            out.push_str(&format!(" │      {}\n", dim_text(r, reason)));
        }
    }

    // Section 5 — Toolkit (3-column grid)
    out.push_str(" │\n");
    out.push_str(&format!(" │  {}\n", section_heading(r, "Toolkit")));
    for chunk in adapters.chunks(3) {
        out.push_str(" │    ");
        for (tool, status) in chunk {
            out.push_str(&tool_status_cell(r, tool, *status));
            out.push_str("  ");
        }
        out.push('\n');
    }

    // Section 6 — Auto Theme Watcher
    out.push_str(" │\n");
    out.push_str(&format!(
        " │  {}\n",
        section_heading(r, "Auto Theme Watcher")
    ));
    out.push_str(&format!(" │    {}\n", dim_text(r, auto_theme_status)));

    // Panel footer + trailing blank line.
    out.push_str(" ╰─────────────────────────────────────────────────────────────╯\n");
    out.push('\n');

    out
}

/// Brand-anchor glyph (`✦`) — lavender-bytes under truecolor, bold under
/// Basic, plain otherwise. brand anchors never depend on the active
/// theme's `brand_accent`.
fn brand_glyph(r: Option<&Roles<'_>>, glyph: &str) -> String {
    match r {
        Some(r) => r.brand(glyph),
        None => glyph.to_string(),
    }
}

/// Section header — `◆ title` with brand-lavender `◆` via
/// `Roles::heading` (/ Sketch 003 canon).
fn section_heading(r: Option<&Roles<'_>>, title: &str) -> String {
    match r {
        Some(r) => r.heading(title),
        None => format!("◆ {title}"),
    }
}

/// Active-theme accent for the theme display name (daily chrome).
fn theme_name(r: Option<&Roles<'_>>, name: &str) -> String {
    match r {
        Some(r) => r.theme_name(name),
        None => name.to_string(),
    }
}

/// Dim + italic — used for paths, identifiers, secondary labels.
fn dim_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.path(text),
        None => text.to_string(),
    }
}

/// Neutral-surface code chip — used for enum-like capability tokens
/// (opacity preset, reload/preview/font-apply summaries).
fn code_text(r: Option<&Roles<'_>>, text: &str) -> String {
    match r {
        Some(r) => r.code(text),
        None => format!("`{text}`"),
    }
}

/// Toolkit cell: `{glyph} {tool:<16}`. Success → theme.green `✓`,
/// not-installed → theme.red `✗`. Both emit via the severity roles so
/// D-01a's "severity never lavender" invariant is honored.
/// Passing a 16-char left-justified label through `Roles::status_*` keeps
/// the theme-colored envelope wrapping the whole cell — the subsequent
/// two-space gap between cells is appended by the caller to preserve the
/// pre-migration 3-column grid layout.
fn tool_status_cell(r: Option<&Roles<'_>>, tool: &str, status: ToolStatus) -> String {
    let cell = format!("{tool:<16}");
    match (r, status) {
        (Some(r), ToolStatus::Themed) => r.status_success(&cell),
        (Some(r), ToolStatus::NotInstalled) => r.status_error(&cell),
        (None, ToolStatus::Themed) => format!("✓ {cell}"),
        (None, ToolStatus::NotInstalled) => format!("✗ {cell}"),
    }
}

/// Render 4 color blocks (fg, bg, accent, error) per theme.
/// Thin wrapper around the allowlisted swatch helper so the `render_*`
/// call site stays test-friendly (returns a `String` instead of printing
/// directly).
fn render_color_blocks(palette: &Palette) -> String {
    let colors = [
        &palette.foreground,
        &palette.background,
        &palette.blue,
        &palette.red,
    ];
    let mut out = String::with_capacity(64);
    for hex in colors {
        out.push_str(&swatch_cell(hex));
    }
    out
}

// SWATCH-RENDERER: intentionally raw ANSI (renders palette colors, not role text)
// This helper is the sole allowlisted styling ANSI site in
// `src/cli/status_panel.rs`. It renders a 4-glyph palette swatch — the
// bytes ARE the rendered theme colors, so they cannot flow through the
// Roles API (which is for text roles, not color previews). The
// SWATCH-RENDERER marker above triggers `count_style_ansi_in`'s
// function-scoped allowlist in `src/brand/migration.rs`; the scanner
// skips everything until the fn body's closing brace lands.
// NOTE: do not mention literal brace characters in this marker-adjacent
// docstring — the scanner counts brace occurrences on subsequent lines
// to find the end of the swatch fn and cannot distinguish comments
// from code (it is deliberately a line-based filter, not a parser).
fn swatch_cell(hex: &str) -> String {
    match PaletteRenderer::hex_to_rgb(hex) {
        Ok((r, g, b)) => format!("\x1b[38;2;{};{};{}m████\x1b[0m", r, g, b),
        Err(_) => String::new(),
    }
}

/// Get installation status for all adapters
fn get_adapter_statuses() -> Result<Vec<(String, ToolStatus)>> {
    let registry = ToolRegistry::default();
    let mut statuses = vec![];

    for (tool_key, display_name) in TOOL_STATUS_ITEMS {
        let status = if let Some(adapter) = registry.get_adapter(tool_key) {
            if adapter.is_installed().unwrap_or(false) {
                ToolStatus::Themed
            } else {
                ToolStatus::NotInstalled
            }
        } else {
            ToolStatus::NotInstalled
        };
        statuses.push((display_name.to_string(), status));
    }

    Ok(statuses)
}

#[cfg(test)]
mod tests {
    use super::{
        auto_theme_status_text, capability_items, capability_row_text, format_status_panel,
        terminal_support_line, ToolStatus, TOOL_STATUS_ITEMS,
    };
    use crate::brand::render_context::{
        mock_context, mock_context_with_mode, mock_theme, RenderMode,
    };
    use crate::brand::roles::Roles;
    use crate::detection::{TerminalFeatureSummary, TerminalProfile};
    use crate::platform::capabilities::{CapabilityReport, CapabilitySnapshot};

    fn fixed_capabilities() -> CapabilitySnapshot {
        CapabilitySnapshot {
            os: CapabilityReport::supported("macos"),
            arch: CapabilityReport::supported("aarch64"),
            shell: CapabilityReport::supported("zsh"),
            terminal: CapabilityReport::supported("ghostty"),
            desktop_appearance: CapabilityReport::supported("macOS defaults"),
            share_capture: CapabilityReport::supported("screencapture"),
            font_platform: CapabilityReport::supported("fontconfig"),
            package_manager: CapabilityReport::supported("homebrew"),
        }
    }

    fn fixed_features() -> TerminalFeatureSummary {
        TerminalFeatureSummary {
            reload: "SIGUSR2".to_string(),
            live_preview: "supported".to_string(),
            font_apply: "config".to_string(),
        }
    }

    fn fixed_adapters() -> Vec<(String, ToolStatus)> {
        vec![
            ("ghostty".to_string(), ToolStatus::Themed),
            ("alacritty".to_string(), ToolStatus::NotInstalled),
            ("starship".to_string(), ToolStatus::Themed),
        ]
    }

    #[test]
    fn test_tool_status_items_use_registered_zsh_key() {
        assert!(TOOL_STATUS_ITEMS
            .iter()
            .any(|(key, label)| *key == "zsh-syntax-highlighting" && *label == "zsh-highlight"));
    }

    #[test]
    fn test_terminal_support_line_for_terminal_app() {
        let terminal = TerminalProfile::from_env_vars(Some("Apple_Terminal"), None);
        let line = terminal_support_line(&terminal);
        assert!(line.contains("supported with limits"));
        assert!(line.contains("manual font pick"));
    }

    #[test]
    fn test_auto_theme_status_uses_terminal_specific_idle_copy() {
        let ghostty = TerminalProfile::from_env_vars(Some("ghostty"), None);
        let terminal_app = TerminalProfile::from_env_vars(Some("Apple_Terminal"), None);

        assert_eq!(
            auto_theme_status_text(true, false, &ghostty, "XDG desktop portal", None),
            "enabled, waiting for the next Ghostty shell — XDG desktop portal"
        );
        assert_eq!(
            auto_theme_status_text(
                true,
                false,
                &terminal_app,
                "GNOME gsettings",
                Some("XDG desktop portal was unavailable, so Slate fell back to GNOME gsettings.")
            ),
            "enabled, but not running — re-enable to restart it — GNOME gsettings (XDG desktop portal was unavailable, so Slate fell back to GNOME gsettings.)"
        );
    }

    #[test]
    fn test_capability_row_text_includes_level_and_backend() {
        let row = capability_row_text(
            "Package Manager",
            &CapabilityReport::best_effort("apt", "validated baseline still landing"),
        );

        assert!(row.contains("Package Manager"));
        assert!(row.contains("best effort"));
        assert!(row.contains("apt"));
    }

    #[test]
    fn test_capability_items_include_font_platform() {
        let snapshot = CapabilitySnapshot {
            os: CapabilityReport::supported("macos"),
            arch: CapabilityReport::supported("aarch64"),
            shell: CapabilityReport::supported("zsh"),
            terminal: CapabilityReport::best_effort("unknown-terminal", "shell/tool theme only"),
            desktop_appearance: CapabilityReport::best_effort(
                "gnome-gsettings",
                "XDG desktop portal was unavailable, so Slate fell back to GNOME gsettings.",
            ),
            share_capture: CapabilityReport::unsupported(
                "unsupported",
                "Share URI export is still available.",
            ),
            font_platform: CapabilityReport::supported("fontconfig"),
            package_manager: CapabilityReport::supported("homebrew"),
        };

        let labels = capability_items(&snapshot)
            .iter()
            .map(|(label, _)| *label)
            .collect::<Vec<_>>();

        assert!(labels.contains(&"Font Platform"));
        assert!(labels.contains(&"Desktop Appearance"));
    }

    /// Task 1 Test 1 — full `slate status` panel snapshot under MockTheme
    /// in Basic mode (byte-stable across CI and workstations because
    /// Basic mode omits truecolor bytes that would vary per theme).
    #[test]
    fn status_panel_full_basic_snapshot() {
        let theme = mock_theme();
        let ctx = mock_context_with_mode(&theme, RenderMode::Basic);
        let r = Roles::new(&ctx);
        let terminal = TerminalProfile::from_env_vars(Some("ghostty"), None);
        let features = fixed_features();
        let capabilities = fixed_capabilities();
        let adapters = fixed_adapters();

        let out = format_status_panel(
            Some(&r),
            &theme,
            "JetBrains Mono",
            "Solid",
            &terminal,
            &features,
            &capabilities,
            &adapters,
            "enabled and running — macOS defaults",
        );

        insta::assert_snapshot!("status_panel_full_basic", out);
    }

    /// Task 1 Test 2 — the palette swatch survives migration.
    /// `render()` pipes `render_color_blocks(&palette)` into the Core
    /// Vibe row; the emitted bytes MUST still contain the ESC CSI
    /// `38;2;` SGR prefix so the 4-glyph theme-color preview is
    /// rendered as colored bytes (swatch, not role text).
    /// Built from a byte slice rather than a source-level ANSI literal
    /// so the Wave-3 grep gate `no_raw_ansi_in_wave_3_files` stays
    /// authoritative — same pattern as
    /// `theme_switch_envelope_uses_green_not_lavender` guard.
    #[test]
    fn status_panel_preserves_palette_swatch() {
        let theme = mock_theme();
        let ctx = mock_context(&theme);
        let r = Roles::new(&ctx);
        let terminal = TerminalProfile::from_env_vars(Some("ghostty"), None);
        let features = fixed_features();
        let capabilities = fixed_capabilities();
        let adapters = fixed_adapters();

        let out = format_status_panel(
            Some(&r),
            &theme,
            "JetBrains Mono",
            "Solid",
            &terminal,
            &features,
            &capabilities,
            &adapters,
            "enabled and running — macOS defaults",
        );

        // `ESC [ 3 8 ; 2 ;` — the truecolor-fg SGR prefix, built byte-
        // by-byte so this source line does not itself count as a raw
        // styling ANSI escape in the grep-gate scan.
        let truecolor_fg_prefix: [u8; 6] = [0x1b, b'[', b'3', b'8', b';', b'2'];
        assert!(
            out.as_bytes()
                .windows(truecolor_fg_prefix.len())
                .any(|w| w == truecolor_fg_prefix),
            "palette swatch must survive role migration — expected truecolor-fg SGR prefix, got: {out:?}"
        );
    }

    /// Task 1 Test 3 — invariant: the `◆ Core Vibe` section heading
    /// renders through `Roles::heading`, which locks the brand-anchor
    /// glyph to `BRAND_LAVENDER_FIXED` (`#7287fd` → `38;2;114;135;253`)
    /// under truecolor mode regardless of the active theme's
    /// `brand_accent`. Guards against accidental future drift back to
    /// per-theme section headings.
    #[test]
    fn status_panel_core_vibe_heading_carries_brand_lavender() {
        let theme = mock_theme();
        let ctx = mock_context(&theme);
        let r = Roles::new(&ctx);
        let terminal = TerminalProfile::from_env_vars(Some("ghostty"), None);
        let features = fixed_features();
        let capabilities = fixed_capabilities();
        let adapters = fixed_adapters();

        let out = format_status_panel(
            Some(&r),
            &theme,
            "JetBrains Mono",
            "Solid",
            &terminal,
            &features,
            &capabilities,
            &adapters,
            "enabled and running — macOS defaults",
        );

        assert!(
            out.contains("38;2;114;135;253"),
            "◆ Core Vibe heading must carry the brand-lavender triple #7287fd (114;135;253), got: {out:?}"
        );
    }

    /// graceful degrade — when Roles is unavailable (e.g. the theme
    /// registry failed to boot), the panel still renders readable plain
    /// chrome: no ANSI bytes outside the palette swatch, section
    /// headings still say `◆ Core Vibe` etc., toolkit cells still use
    /// `✓ ` / `✗ ` glyphs.
    #[test]
    fn status_panel_falls_back_to_plain_when_roles_absent() {
        let theme = mock_theme();
        let terminal = TerminalProfile::from_env_vars(Some("ghostty"), None);
        let features = fixed_features();
        let capabilities = fixed_capabilities();
        let adapters = fixed_adapters();

        let out = format_status_panel(
            None,
            &theme,
            "JetBrains Mono",
            "Solid",
            &terminal,
            &features,
            &capabilities,
            &adapters,
            "disabled — macOS defaults",
        );

        // The only ESC/CSI bytes allowed in the fallback path are the
        // ones produced by the palette swatch row.
        for line in out.lines() {
            if line.contains("████") {
                continue; // swatch row — expected to carry raw ANSI
            }
            assert!(
                !line.contains('\x1b'),
                "fallback path must be plain text outside the swatch row, got: {line:?}"
            );
        }
        assert!(out.contains("◆ Core Vibe"));
        assert!(out.contains("◆ Typography"));
        assert!(out.contains("✓ ghostty"));
        assert!(out.contains("✗ alacritty"));
    }
}
