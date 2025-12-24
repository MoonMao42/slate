use crate::adapter::palette_renderer::PaletteRenderer;
use crate::adapter::registry::ToolRegistry;
use crate::config::ConfigManager;
use crate::design::symbols::Symbols;
use crate::error::Result;
use crate::theme::{Palette, ThemeRegistry};
use crate::brand::Language;

/// Tool installation status 
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolStatus {
    Themed,       // ✓ Themed
    Paused,       // ○ Paused ()
    NotInstalled, // ✗ Not installed
}

/// Check if auto-theme launchd agent is loaded 
/// Run `launchctl list sh.slate.auto-theme` and check exit code
/// Return format: "[loaded]" (exit 0) or "[not installed]" (non-zero)
fn get_agent_status() -> String {
    match std::process::Command::new("launchctl")
        .args(&["list", "sh.slate.auto-theme"])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                Language::STATUS_AUTO_AGENT_LOADED.to_string()
            } else {
                Language::STATUS_AUTO_AGENT_NOT_INSTALLED.to_string()
            }
        }
        Err(_) => Language::STATUS_AUTO_AGENT_NOT_INSTALLED.to_string(),
    }
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
    let terminal = detect_terminal();

    // Print blank line above 
    println!();

    // Rounded panel header
    println!(
        "{}╭─ {} slate status ─────────────────────────────────────────╮",
        " ",
        Symbols::BRAND
    );

    // Section 1 - Core Vibe
    println!("{}│", " ");
    println!("{}│  {} Core Vibe", " ", Symbols::DIAMOND);
    print!("{}│    ", " ");
    print_color_blocks(&current_theme.palette);
    println!(" {}", current_theme.name);
    println!("{}│    {}", " ", current_theme.family);

    // Section 2 - Typography
    println!("{}│", " ");
    println!("{}│  {} Typography", " ", Symbols::DIAMOND);
    println!("{}│    {}", " ", current_font);

    // Section 3 - Background
    println!("{}│", " ");
    println!("{}│  {} Background", " ", Symbols::DIAMOND);
    println!("{}│    {}  {}", " ", "Terminal", terminal);
    println!("{}│    {}  {}", " ", "Opacity", current_opacity);

    // Section 4 - Toolkit (3-column grid)
    println!("{}│", " ");
    println!("{}│  {} Toolkit", " ", Symbols::DIAMOND);
    let adapter_status = get_adapter_statuses()?;
    for chunk in adapter_status.chunks(3) {
        print!("{}│    ", " ");
        for (tool, status) in chunk {
            let symbol = match status {
                ToolStatus::Themed => Symbols::SUCCESS,
                ToolStatus::Paused => Symbols::PENDING,
                ToolStatus::NotInstalled => Symbols::FAILURE,
            };
            print!("{} {:<16}  ", symbol, tool);
        }
        println!();
    }

    // Section 5 - Auto Theme Agent
    println!("{}│", " ");
    let agent_status = get_agent_status();
    println!("{}│  {} Auto Theme Agent", " ", Symbols::DIAMOND);
    println!("{}│    {}", " ", agent_status);

    // Panel footer
    println!(
        "{}╰─────────────────────────────────────────────────────────────╯",
        " "
    );

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

    let tools = vec![
        "ghostty",
        "alacritty",
        "starship",
        "bat",
        "delta",
        "eza",
        "lazygit",
        "fastfetch",
        "zsh-highlight",
        "tmux",
        "nerd-font",
    ];

    for tool in tools {
        let status = if let Some(adapter) = registry.get_adapter(tool) {
            if adapter.is_installed().unwrap_or(false) {
                ToolStatus::Themed
            } else {
                ToolStatus::NotInstalled
            }
        } else {
            ToolStatus::NotInstalled
        };
        statuses.push((tool.to_string(), status));
    }

    Ok(statuses)
}

/// Detect terminal from environment variables
fn detect_terminal() -> String {
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        term_program
    } else if let Ok(term) = std::env::var("TERM") {
        term
    } else {
        "Unknown".to_string()
    }
}
