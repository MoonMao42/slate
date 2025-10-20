use super::{Theme, ThemeColors, ThemeFamily};
use std::collections::HashMap;

pub fn tokyo_night_light() -> Theme {
    let mut overrides = HashMap::new();
    overrides.insert("ghostty".to_string(), "Tokyo Night Light".to_string());
    overrides.insert("starship".to_string(), "tokyo-night-light".to_string());
    overrides.insert("bat".to_string(), "Tokyo Night Light".to_string());

    Theme {
        name: "tokyo-night-light".to_string(),
        family: ThemeFamily::TokyoNight,
        colors: ThemeColors {
            foreground: "#3760bf".to_string(),
            background: "#f5f5f5".to_string(),
            cursor: "#0184bc".to_string(),
            red: "#d20f39".to_string(),
            green: "#2c8340".to_string(),
            yellow: "#9a6e00".to_string(),
            blue: "#0184bc".to_string(),
            magenta: "#7847bd".to_string(),
            cyan: "#00788c".to_string(),
            white: "#c4cccd".to_string(),
            tool_overrides: overrides,
        },
    }
}

pub fn tokyo_night_dark() -> Theme {
    let mut overrides = HashMap::new();
    overrides.insert("ghostty".to_string(), "Tokyo Night".to_string());
    overrides.insert("starship".to_string(), "tokyo-night".to_string());
    overrides.insert("bat".to_string(), "Tokyo Night".to_string());

    Theme {
        name: "tokyo-night-dark".to_string(),
        family: ThemeFamily::TokyoNight,
        colors: ThemeColors {
            foreground: "#c0caf5".to_string(),
            background: "#1a1b26".to_string(),
            cursor: "#7aa2f7".to_string(),
            red: "#f7768e".to_string(),
            green: "#9ece6a".to_string(),
            yellow: "#e0af68".to_string(),
            blue: "#7aa2f7".to_string(),
            magenta: "#bb9af7".to_string(),
            cyan: "#7dcfff".to_string(),
            white: "#a9b1d6".to_string(),
            tool_overrides: overrides,
        },
    }
}
