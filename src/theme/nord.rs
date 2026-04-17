use super::{load_theme, ThemeVariant};
use crate::error::Result;

pub fn nord() -> Result<ThemeVariant> {
    load_theme("nord")
}
