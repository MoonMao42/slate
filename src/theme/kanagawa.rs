use super::{load_theme, ThemeVariant};
use crate::error::Result;

pub fn kanagawa_dragon() -> Result<ThemeVariant> {
    load_theme("kanagawa-dragon")
}

pub fn kanagawa_wave() -> Result<ThemeVariant> {
    load_theme("kanagawa-wave")
}

pub fn kanagawa_lotus() -> Result<ThemeVariant> {
    load_theme("kanagawa-lotus")
}
