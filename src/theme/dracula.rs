use super::{Theme, ThemeColors, ThemeFamily};
use std::collections::HashMap;

pub fn dracula() -> Theme {
    let mut overrides = HashMap::new();
    overrides.insert("ghostty".to_string(), "Dracula".to_string());
    overrides.insert("starship".to_string(), "dracula".to_string());
    overrides.insert("bat".to_string(), "Dracula".to_string());

    Theme {
        name: "dracula".to_string(),
        family: ThemeFamily::Dracula,
        colors: ThemeColors {
            foreground: "#f8f8f2".to_string(),
            background: "#282a36".to_string(),
            cursor: "#f8f8f2".to_string(),
            red: "#ff5555".to_string(),
            green: "#50fa7b".to_string(),
            yellow: "#f1fa8c".to_string(),
            blue: "#6272a4".to_string(),
            magenta: "#ff79c6".to_string(),
            cyan: "#8be9fd".to_string(),
            white: "#f8f8f2".to_string(),
            tool_overrides: overrides,
        },
    }
}
