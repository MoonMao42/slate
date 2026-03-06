use crate::adapter::palette_renderer::PaletteRenderer;
use crate::adapter::registry::ToolRegistry;
use crate::brand::Language;
use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::detection::TerminalProfile;
use crate::error::Result;
use crate::platform::capabilities::{detect_capabilities, CapabilityReport, CapabilitySnapshot};
use crate::theme::{Palette, ThemeRegistry};

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

    // Print blank line above 
    println!();

    // Rounded panel header
    println!(
        " ╭─ {} slate status ─────────────────────────────────────────╮",
        Symbols::BRAND
    );

    // Section 1 - Core Vibe
    println!(" │");
    println!(" │  {} Core Vibe", Symbols::DIAMOND);
    print!(" │    ");
    print_color_blocks(&current_theme.palette);
    println!(" {}", current_theme.name);
    println!(" │    {}", current_theme.family);

    // Section 2 - Typography
    println!(" │");
    println!(" │  {} Typography", Symbols::DIAMOND);
    println!(" │    {}", current_font);

    // Section 3 - Background
    println!(" │");
    println!(" │  {} Background", Symbols::DIAMOND);
    println!(" │    Terminal  {}", terminal.display_name());
    println!(" │    Support   {}", terminal_support_line(&terminal));
    println!(" │    Reload   {}", terminal_features.reload);
    println!(" │    Preview  {}", terminal_features.live_preview);
    println!(" │    Font     {}", terminal_features.font_apply);
    println!(" │    Opacity  {}", current_opacity);

    // Section 4 - Shared platform capabilities
    println!(" │");
    println!(
        " │  {} {}",
        Symbols::DIAMOND,
        Language::STATUS_PLATFORM_CAPABILITIES
    );
    for (label, report) in capability_items(&capabilities) {
        println!(" │    {}", capability_row_text(label, report));
        if let Some(reason) = report.reason.as_deref() {
            println!(" │      {}", reason);
        }
    }

    // Section 5 - Toolkit (3-column grid)
    println!(" │");
    println!(" │  {} Toolkit", Symbols::DIAMOND);
    let adapter_status = get_adapter_statuses()?;
    for chunk in adapter_status.chunks(3) {
        print!(" │    ");
        for (tool, status) in chunk {
            let symbol = match status {
                ToolStatus::Themed => Symbols::SUCCESS,
                ToolStatus::NotInstalled => Symbols::FAILURE,
            };
            print!("{} {:<16}  ", symbol, tool);
        }
        println!();
    }

    // Section 6 - Auto Theme Watcher
    println!(" │");
    let auto_theme_status = get_auto_theme_status(&config, &terminal);
    println!(" │  {} Auto Theme Watcher", Symbols::DIAMOND);
    println!(" │    {}", auto_theme_status);

    // Panel footer
    println!(" ╰─────────────────────────────────────────────────────────────╯");

    // Print blank line below 
    println!();

    Ok(())
}

/// Render 4 color blocks (fg, bg, accent, error) per theme
fn print_color_blocks(palette: &Palette) {
    // 4 representative colors: foreground, background, accent (blue), error (red)
    let colors = vec![
        &palette.foreground,
        &palette.background,
        &palette.blue,
        &palette.red,
    ];

    for hex in colors {
        if let Ok((r, g, b)) = PaletteRenderer::hex_to_rgb(hex) {
            print!("\x1b[38;2;{};{};{}m████\x1b[0m", r, g, b);
        }
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
        auto_theme_status_text, capability_items, capability_row_text, terminal_support_line,
        TOOL_STATUS_ITEMS,
    };
    use crate::detection::TerminalProfile;
    use crate::platform::capabilities::{CapabilityReport, CapabilitySnapshot};

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
}
