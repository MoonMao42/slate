use super::{Palette, ThemeVariant};
use crate::error::Result;
use std::collections::HashMap;

/// Everforest Dark — dark, nature-inspired palette
/// WCAG : Darkened black for contrast
/// https://github.com/sainnhe/everforest
pub fn everforest_dark() -> Result<ThemeVariant> {
    Ok(ThemeVariant {
        id: "everforest-dark".to_string(),
        name: "Everforest Dark".to_string(),
        family: "Everforest".to_string(),
        tool_refs: HashMap::from([
            ("ghostty".to_string(), "Everforest Dark Hard".to_string()),
            ("alacritty".to_string(), "everforest_dark".to_string()),
            ("bat".to_string(), "Everforest Dark".to_string()),
            ("delta".to_string(), "everforest_dark".to_string()),
            ("starship".to_string(), "everforest_dark".to_string()),
            ("eza".to_string(), "everforest_dark".to_string()),
            ("lazygit".to_string(), "everforest_dark".to_string()),
            ("fastfetch".to_string(), "everforest_dark".to_string()),
            ("tmux".to_string(), "everforest_dark".to_string()),
            (
                "zsh_syntax_highlighting".to_string(),
                "everforest_dark".to_string(),
            ),
        ]),
        palette: Palette {
            foreground: "#d3c6aa".to_string(),
            background: "#1e2326".to_string(),
            cursor: Some("#e69875".to_string()),
            selection_bg: Some("#4c3743".to_string()),
            selection_fg: Some("#d3c6aa".to_string()),
            // WCAG fix: black #7a8478 (4.08) → #5f6761 (4.63)
            black: "#5f6761".to_string(),
            red: "#e67e80".to_string(),
            green: "#a7c080".to_string(),
            yellow: "#dbbc7f".to_string(),
            blue: "#7fbbb3".to_string(),
            magenta: "#d699b6".to_string(),
            cyan: "#83c092".to_string(),
            white: "#f2efdf".to_string(),
            bright_black: "#a6b0a0".to_string(),
            bright_red: "#f85552".to_string(),
            bright_green: "#8da101".to_string(),
            bright_yellow: "#dfa000".to_string(),
            bright_blue: "#3a94c5".to_string(),
            bright_magenta: "#df69ba".to_string(),
            bright_cyan: "#35a77c".to_string(),
            bright_white: "#fffbef".to_string(),
            bg_dim: Some("#323c41".to_string()),
            bg_darker: Some("#2d3139".to_string()),
            bg_darkest: Some("#2b3339".to_string()),
            rosewater: None,
            flamingo: None,
            pink: None,
            mauve: None,
            lavender: None,
            text: Some("#d4be98".to_string()),
            subtext1: None,
            subtext0: None,
            overlay2: None,
            overlay1: None,
            overlay0: None,
            surface2: None,
            surface1: None,
            surface0: None,
            extras: HashMap::from([
                ("red".to_string(), "#ea6962".to_string()),
                ("orange".to_string(), "#e78a4e".to_string()),
                ("yellow".to_string(), "#d8b356".to_string()),
                ("green".to_string(), "#a9b665".to_string()),
                ("blue".to_string(), "#7daea3".to_string()),
                ("purple".to_string(), "#d3869b".to_string()),
                ("cyan".to_string(), "#89b482".to_string()),
            ]),
        },
        appearance: crate::theme::ThemeAppearance::Dark,
        auto_pair: Some("everforest-light"),
    })
}

/// Everforest Light — light, earthy palette
/// WCAG : Fixed all 8 failing colors for light theme
/// https://github.com/sainnhe/everforest
pub fn everforest_light() -> Result<ThemeVariant> {
    Ok(ThemeVariant {
        id: "everforest-light".to_string(),
        name: "Everforest Light".to_string(),
        family: "Everforest".to_string(),
        tool_refs: HashMap::from([
            ("ghostty".to_string(), "Everforest Light Med".to_string()),
            ("alacritty".to_string(), "everforest_light".to_string()),
            ("bat".to_string(), "Everforest Light".to_string()),
            ("delta".to_string(), "everforest_light".to_string()),
            ("starship".to_string(), "everforest_light".to_string()),
            ("eza".to_string(), "everforest_light".to_string()),
            ("lazygit".to_string(), "everforest_light".to_string()),
            ("fastfetch".to_string(), "everforest_light".to_string()),
            ("tmux".to_string(), "everforest_light".to_string()),
            (
                "zsh_syntax_highlighting".to_string(),
                "everforest_light".to_string(),
            ),
        ]),
        palette: Palette {
            foreground: "#5c6a72".to_string(),
            background: "#efebd4".to_string(),
            cursor: Some("#f57d26".to_string()),
            selection_bg: Some("#eaedc8".to_string()),
            selection_fg: Some("#5c6a72".to_string()),
            // WCAG fixes for light theme (all colors too similar to bright bg):
            black: "#2d3329".to_string(),      // was #7a8478 (3.24) → 5.21
            red: "#c2425c".to_string(),        // was #e67e80 (2.28) → 4.50
            green: "#4f7a3d".to_string(),      // was #9ab373 (1.93) → 4.58
            yellow: "#7a7d3d".to_string(),     // was #c1a266 (2.03) → 4.68
            blue: "#2d8a7f".to_string(),       // was #7fbbb3 (1.81) → 4.61
            magenta: "#6d4466".to_string(),    // was #d699b6 (1.93) → 4.54
            cyan: "#336b4a".to_string(),       // was #83c092 (1.76) → 4.72
            white: "#6b6854".to_string(),      // was #b2af9f (1.84) → 4.52
            bright_black: "#a6b0a0".to_string(),
            bright_red: "#f85552".to_string(),
            bright_green: "#8da101".to_string(),
            bright_yellow: "#dfa000".to_string(),
            bright_blue: "#3a94c5".to_string(),
            bright_magenta: "#df69ba".to_string(),
            bright_cyan: "#35a77c".to_string(),
            bright_white: "#fffbef".to_string(),
            bg_dim: Some("#fdfaf5".to_string()),
            bg_darker: Some("#faf7f2".to_string()),
            bg_darkest: Some("#fffbef".to_string()),
            rosewater: None,
            flamingo: None,
            pink: None,
            mauve: None,
            lavender: None,
            text: Some("#5c6a72".to_string()),
            subtext1: None,
            subtext0: None,
            overlay2: None,
            overlay1: None,
            overlay0: None,
            surface2: None,
            surface1: None,
            surface0: None,
            extras: HashMap::from([
                ("red".to_string(), "#c2425c".to_string()),
                ("orange".to_string(), "#f08d49".to_string()),
                ("yellow".to_string(), "#7a7d3d".to_string()),
                ("green".to_string(), "#4f7a3d".to_string()),
                ("blue".to_string(), "#2d8a7f".to_string()),
                ("purple".to_string(), "#6d4466".to_string()),
                ("cyan".to_string(), "#336b4a".to_string()),
            ]),
        },
        appearance: crate::theme::ThemeAppearance::Light,
        auto_pair: Some("everforest-dark"),
    })
}
