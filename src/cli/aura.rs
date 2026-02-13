use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeRegistry;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

const QUOTES: &[(&str, &str)] = &[
    (
        "Good design is as little design as possible.",
        "Dieter Rams",
    ),
    (
        "Simplicity is the ultimate sophistication.",
        "Leonardo da Vinci",
    ),
    (
        "The details are not the details. They make the design.",
        "Charles Eames",
    ),
    (
        "Design is not just what it looks like. Design is how it works.",
        "Steve Jobs",
    ),
    ("Less, but better.", "Dieter Rams"),
    (
        "Any sufficiently advanced technology is indistinguishable from magic.",
        "Arthur C. Clarke",
    ),
    ("The best interface is no interface.", "Golden Krishna"),
    (
        "Make it work, make it right, make it fast.",
        "Kent Beck",
    ),
    (
        "Perfection is achieved when there is nothing left to take away.",
        "Antoine de Saint-Exupery",
    ),
    (
        "We shape our tools, and thereafter our tools shape us.",
        "Marshall McLuhan",
    ),
];

/// Parse a hex color string (#RRGGBB) into (r, g, b) tuple.
fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Hidden easter egg: display a themed quote.
pub fn handle() -> Result<()> {
    let mut stdout = io::stdout();

    // Try to load current theme colors for styling
    let (accent_color, subtext_color) = load_theme_colors();

    // Pick a random quote based on current time
    let index = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize
        % QUOTES.len();
    let (quote, author) = QUOTES[index];

    // Clear screen and position cursor
    crossterm::execute!(
        stdout,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    )?;

    // Render with vertical padding and themed colors
    let accent_start = format_color_start(&accent_color);
    let subtext_start = format_color_start(&subtext_color);
    let reset = "\x1b[0m";

    write!(
        stdout,
        "\n\n  {accent_start}\"{quote}\"{reset}\n\n  {subtext_start}-- {author}{reset}\n\n"
    )?;
    stdout.flush()?;

    Ok(())
}

/// Load accent and subtext colors from the current theme.
/// Falls back to sensible defaults if theme loading fails.
fn load_theme_colors() -> (String, String) {
    let default_accent = "#89b4fa".to_string(); // Catppuccin Mocha blue
    let default_subtext = "#a6adc8".to_string(); // Catppuccin Mocha subtext0

    let Ok(env) = SlateEnv::from_process() else {
        return (default_accent, default_subtext);
    };
    let Ok(config) = ConfigManager::with_env(&env) else {
        return (default_accent, default_subtext);
    };
    let Ok(Some(theme_id)) = config.get_current_theme() else {
        return (default_accent, default_subtext);
    };

    let Ok(registry) = ThemeRegistry::new() else {
        return (default_accent, default_subtext);
    };
    let Some(theme) = registry.get(&theme_id) else {
        return (default_accent, default_subtext);
    };

    let accent = theme.palette.cyan.clone();
    let subtext = theme
        .palette
        .subtext0
        .clone()
        .or_else(|| theme.palette.subtext1.clone())
        .unwrap_or_else(|| theme.palette.bright_black.clone());

    (accent, subtext)
}

/// Format an ANSI 24-bit color escape sequence from a hex color.
fn format_color_start(hex: &str) -> String {
    match parse_hex_color(hex) {
        Some((r, g, b)) => format!("\x1b[38;2;{r};{g};{b}m"),
        None => String::new(),
    }
}
