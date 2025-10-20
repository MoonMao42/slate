pub mod adapter;
pub mod error;
pub mod theme;

pub use adapter::{ApplyThemeResult, ToolAdapter, ToolRegistry};
pub use error::{ThemeError, ThemeResult};
pub use theme::{
    available_themes, get_theme, normalize_theme_name, parse_theme_input, Theme, ThemeColors,
    ThemeFamily,
};
