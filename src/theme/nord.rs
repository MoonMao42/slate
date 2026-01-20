use super::{Palette, ThemeVariant};
use crate::error::Result;
use std::collections::HashMap;

/// Nord — arctic, north-bluish palette
/// WCAG : Fixed black and red for WCAG 4.5:1 compliance
pub fn nord() -> Result<ThemeVariant> {
    Ok(ThemeVariant {
        id: "nord".to_string(),
        name: "Nord".to_string(),
        family: "Nord".to_string(),
        tool_refs: HashMap::from([
            ("ghostty".to_string(), "Nord".to_string()),
            ("alacritty".to_string(), "nord".to_string()),
            ("bat".to_string(), "Nord".to_string()),
            ("delta".to_string(), "nord".to_string()),
            ("starship".to_string(), "nord".to_string()),
            ("eza".to_string(), "nord".to_string()),
            ("lazygit".to_string(), "nord".to_string()),
            ("fastfetch".to_string(), "nord".to_string()),
            ("tmux".to_string(), "nord".to_string()),
            ("zsh_syntax_highlighting".to_string(), "nord".to_string()),
        ]),
        palette: Palette {
            foreground: "#d8dee9".to_string(),
            background: "#2e3440".to_string(),
            cursor: Some("#d8dee9".to_string()),
            selection_bg: Some("#434c5e".to_string()),
            selection_fg: Some("#eceff4".to_string()),
            // WCAG fixes: black #596377 → #eceff4 (flip to light), red #d86b6d → #d60000
            black: "#eceff4".to_string(),      // was #596377 (2.07) → 10.84 (flipped to light)
            red: "#ff7777".to_string(),        // WCAG : was #d86b6d (3.71) → #ff7777 (4.85)
            green: "#a3be8c".to_string(),
            yellow: "#ebcb8b".to_string(),
            blue: "#81a1c1".to_string(),
            magenta: "#d68ae0".to_string(),
            cyan: "#88c0d0".to_string(),
            white: "#e5e9f0".to_string(),
            bright_black: "#4c566a".to_string(),
            bright_red: "#bf616a".to_string(),
            bright_green: "#a3be8c".to_string(),
            bright_yellow: "#ebcb8b".to_string(),
            bright_blue: "#81a1c1".to_string(),
            bright_magenta: "#d68ae0".to_string(),
            bright_cyan: "#8fbcbb".to_string(),
            bright_white: "#eceff4".to_string(),
            bg_dim: Some("#373e4c".to_string()),
            bg_darker: Some("#2e3440".to_string()),
            bg_darkest: Some("#1e2227".to_string()),
            rosewater: Some("#d8dee9".to_string()),
            flamingo: Some("#bf616a".to_string()),
            pink: Some("#d08770".to_string()),
            mauve: Some("#b48ead".to_string()),
            lavender: Some("#5e81ac".to_string()),
            text: Some("#d8dee9".to_string()),
            subtext1: Some("#d0d8e0".to_string()),
            subtext0: Some("#c8cfd8".to_string()),
            overlay2: Some("#a3b0c0".to_string()),
            overlay1: Some("#788ca0".to_string()),
            overlay0: Some("#505860".to_string()),
            surface2: Some("#434c5e".to_string()),
            surface1: Some("#3b4252".to_string()),
            surface0: Some("#2e3440".to_string()),
            extras: HashMap::new(),
        },
        appearance: crate::theme::ThemeAppearance::Dark,
        auto_pair: Some("nord"),
    })
}
