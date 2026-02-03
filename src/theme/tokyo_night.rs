use super::{load_theme, ThemeVariant};
use crate::error::Result;

pub fn tokyo_night_dark() -> Result<ThemeVariant> {
    load_theme("tokyo-night-dark")
}

pub fn tokyo_night_light() -> Result<ThemeVariant> {
    load_theme("tokyo-night-light")
}
