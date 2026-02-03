use super::{load_theme, ThemeVariant};
use crate::error::Result;

pub fn catppuccin_latte() -> Result<ThemeVariant> {
    load_theme("catppuccin-latte")
}

pub fn catppuccin_frappe() -> Result<ThemeVariant> {
    load_theme("catppuccin-frappe")
}

pub fn catppuccin_macchiato() -> Result<ThemeVariant> {
    load_theme("catppuccin-macchiato")
}

pub fn catppuccin_mocha() -> Result<ThemeVariant> {
    load_theme("catppuccin-mocha")
}
