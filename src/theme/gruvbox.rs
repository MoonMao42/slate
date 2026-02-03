use super::{load_theme, ThemeVariant};
use crate::error::Result;

pub fn gruvbox_dark() -> Result<ThemeVariant> {
    load_theme("gruvbox-dark")
}

pub fn gruvbox_light() -> Result<ThemeVariant> {
    load_theme("gruvbox-light")
}
