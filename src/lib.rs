pub mod adapter;
pub mod cli;
pub mod config;
pub mod error;
pub mod theme;

pub use adapter::{ApplyThemeResult, BatAdapter, DeltaAdapter, GhosttyAdapter, LazygitAdapter, StarshipAdapter, ToolAdapter, ToolRegistry};
pub use cli::*;
pub use error::{ThemeError, ThemeResult};
pub use theme::{
    available_themes, get_theme, normalize_theme_name, parse_theme_input, Theme, ThemeColors,
    ThemeFamily,
};
