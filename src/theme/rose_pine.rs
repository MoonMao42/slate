use super::{load_theme, ThemeVariant};
use crate::error::Result;

pub fn rose_pine_main() -> Result<ThemeVariant> {
    load_theme("rose-pine-main")
}

pub fn rose_pine_moon() -> Result<ThemeVariant> {
    load_theme("rose-pine-moon")
}

pub fn rose_pine_dawn() -> Result<ThemeVariant> {
    load_theme("rose-pine-dawn")
}
