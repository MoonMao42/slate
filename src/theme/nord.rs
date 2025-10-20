use super::{Theme, ThemeColors, ThemeFamily};
use std::collections::HashMap;

pub fn nord() -> Theme {
    let mut overrides = HashMap::new();
    overrides.insert("ghostty".to_string(), "Nord".to_string());
    overrides.insert("starship".to_string(), "nord".to_string());
    overrides.insert("bat".to_string(), "Nord".to_string());

    Theme {
        name: "nord".to_string(),
        family: ThemeFamily::Nord,
        colors: ThemeColors {
            foreground: "#d8dee9".to_string(),
            background: "#2e3440".to_string(),
            cursor: "#88c0d0".to_string(),
            red: "#bf616a".to_string(),
            green: "#a3be8c".to_string(),
            yellow: "#ebcb8b".to_string(),
            blue: "#5e81ac".to_string(),
            magenta: "#b48ead".to_string(),
            cyan: "#88c0d0".to_string(),
            white: "#eceff4".to_string(),
            tool_overrides: overrides,
        },
    }
}
