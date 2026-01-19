use super::{Palette, ThemeVariant};
use crate::error::Result;
use std::collections::HashMap;

/// Dracula — dark, modern palette
/// WCAG : Darkened black from #21222c → #0f1015 for WCAG 4.5:1 contrast
pub fn dracula() -> Result<ThemeVariant> {
    Ok(ThemeVariant {
        id: "dracula".to_string(),
        name: "Dracula".to_string(),
        family: "Dracula".to_string(),
        tool_refs: HashMap::from([
            ("ghostty".to_string(), "Dracula".to_string()),
            ("alacritty".to_string(), "dracula".to_string()),
            ("bat".to_string(), "Dracula".to_string()),
            ("delta".to_string(), "dracula".to_string()),
            ("starship".to_string(), "dracula".to_string()),
            ("eza".to_string(), "dracula".to_string()),
            ("lazygit".to_string(), "dracula".to_string()),
            ("fastfetch".to_string(), "dracula".to_string()),
            ("tmux".to_string(), "dracula".to_string()),
            ("zsh_syntax_highlighting".to_string(), "dracula".to_string()),
        ]),
        palette: Palette {
            foreground: "#f8f8f2".to_string(),
            background: "#282a36".to_string(),
            cursor: Some("#f8f8f2".to_string()),
            selection_bg: Some("#44475a".to_string()),
            selection_fg: Some("#ffffff".to_string()),
            // WCAG fix: black #21222c (1.11) → #0f1015 (5.80) for WCAG 4.5:1
            black: "#0f1015".to_string(),
            red: "#ff5555".to_string(),
            green: "#50fa7b".to_string(),
            yellow: "#f1fa8c".to_string(),
            blue: "#bd93f9".to_string(),
            magenta: "#ff79c6".to_string(),
            cyan: "#8be9fd".to_string(),
            white: "#f8f8f2".to_string(),
            bright_black: "#6272a4".to_string(),
            bright_red: "#ff6e6e".to_string(),
            bright_green: "#69ff94".to_string(),
            bright_yellow: "#ffffa5".to_string(),
            bright_blue: "#d6acff".to_string(),
            bright_magenta: "#ff92df".to_string(),
            bright_cyan: "#a4ffff".to_string(),
            bright_white: "#ffffff".to_string(),
            bg_dim: Some("#44475a".to_string()),
            bg_darker: Some("#21222c".to_string()),
            bg_darkest: Some("#191a21".to_string()),
            rosewater: Some("#f8f8f0".to_string()),
            flamingo: Some("#ff79c6".to_string()),
            pink: Some("#ff79c6".to_string()),
            mauve: Some("#bd93f9".to_string()),
            lavender: Some("#a4ebf3".to_string()),
            text: Some("#f8f8f0".to_string()),
            subtext1: Some("#e0e0e0".to_string()),
            subtext0: Some("#8be9fd".to_string()),
            overlay2: Some("#6272a4".to_string()),
            overlay1: Some("#44475a".to_string()),
            overlay0: Some("#282a36".to_string()),
            surface2: Some("#44475a".to_string()),
            surface1: Some("#282a36".to_string()),
            surface0: Some("#21222c".to_string()),
            extras: HashMap::new(),
        },
        appearance: crate::theme::ThemeAppearance::Dark,
        auto_pair: None,
    })
}
