use super::{load_theme, ThemeVariant};
use crate::error::Result;

pub fn dracula() -> Result<ThemeVariant> {
    load_theme("dracula")
}
