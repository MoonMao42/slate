use super::{Theme, ThemeColors, ThemeFamily};
use std::collections::HashMap;

pub fn catppuccin_latte() -> Theme {
    let mut overrides = HashMap::new();
    overrides.insert("ghostty".to_string(), "Catppuccin Latte".to_string());
    overrides.insert("starship".to_string(), "catppuccin_latte".to_string());
    overrides.insert("bat".to_string(), "Catppuccin Latte".to_string());

    Theme {
        name: "catppuccin-latte".to_string(),
        family: ThemeFamily::Catppuccin,
        colors: ThemeColors {
            foreground: "#4c4f69".to_string(),
            background: "#eff1f5".to_string(),
            cursor: "#dc8a78".to_string(),
            red: "#d20f39".to_string(),
            green: "#40a02b".to_string(),
            yellow: "#df8e1d".to_string(),
            blue: "#1e66f5".to_string(),
            magenta: "#ea76cb".to_string(),
            cyan: "#04a5e5".to_string(),
            white: "#acb0be".to_string(),
            tool_overrides: overrides,
        },
    }
}

pub fn catppuccin_frappe() -> Theme {
    let mut overrides = HashMap::new();
    overrides.insert("ghostty".to_string(), "Catppuccin Frappe".to_string());
    overrides.insert("starship".to_string(), "catppuccin_frappe".to_string());
    overrides.insert("bat".to_string(), "Catppuccin Frappe".to_string());

    Theme {
        name: "catppuccin-frappe".to_string(),
        family: ThemeFamily::Catppuccin,
        colors: ThemeColors {
            foreground: "#c6d0f5".to_string(),
            background: "#302d41".to_string(),
            cursor: "#f5a97f".to_string(),
            red: "#e64553".to_string(),
            green: "#a6d189".to_string(),
            yellow: "#e5c890".to_string(),
            blue: "#8caaee".to_string(),
            magenta: "#f4b8e4".to_string(),
            cyan: "#81c8be".to_string(),
            white: "#b5bfe2".to_string(),
            tool_overrides: overrides,
        },
    }
}

pub fn catppuccin_macchiato() -> Theme {
    let mut overrides = HashMap::new();
    overrides.insert("ghostty".to_string(), "Catppuccin Macchiato".to_string());
    overrides.insert("starship".to_string(), "catppuccin_macchiato".to_string());
    overrides.insert("bat".to_string(), "Catppuccin Macchiato".to_string());

    Theme {
        name: "catppuccin-macchiato".to_string(),
        family: ThemeFamily::Catppuccin,
        colors: ThemeColors {
            foreground: "#cad1f5".to_string(),
            background: "#24273a".to_string(),
            cursor: "#f5a97f".to_string(),
            red: "#ed8796".to_string(),
            green: "#a6da95".to_string(),
            yellow: "#eed49f".to_string(),
            blue: "#8aadf4".to_string(),
            magenta: "#f5bde6".to_string(),
            cyan: "#8bd5ca".to_string(),
            white: "#b8c0e0".to_string(),
            tool_overrides: overrides,
        },
    }
}

pub fn catppuccin_mocha() -> Theme {
    let mut overrides = HashMap::new();
    overrides.insert("ghostty".to_string(), "Catppuccin Mocha".to_string());
    overrides.insert("starship".to_string(), "catppuccin_mocha".to_string());
    overrides.insert("bat".to_string(), "Catppuccin Mocha".to_string());

    Theme {
        name: "catppuccin-mocha".to_string(),
        family: ThemeFamily::Catppuccin,
        colors: ThemeColors {
            foreground: "#cdd6f4".to_string(),
            background: "#1e1e2e".to_string(),
            cursor: "#f5a97f".to_string(),
            red: "#f38ba8".to_string(),
            green: "#a6e3a1".to_string(),
            yellow: "#f9e2af".to_string(),
            blue: "#89b4fa".to_string(),
            magenta: "#f5c2de".to_string(),
            cyan: "#94e2d5".to_string(),
            white: "#bac2de".to_string(),
            tool_overrides: overrides,
        },
    }
}
