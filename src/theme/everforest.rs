use super::{load_theme, ThemeVariant};
use crate::error::Result;

pub fn everforest_dark() -> Result<ThemeVariant> {
    load_theme("everforest-dark")
}

pub fn everforest_light() -> Result<ThemeVariant> {
    load_theme("everforest-light")
}
