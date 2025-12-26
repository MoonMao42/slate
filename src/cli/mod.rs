pub mod auto_theme;
pub mod clean;
pub mod config;
pub mod failure_handler;
pub mod font;
pub mod font_detection;
pub mod font_selection;
pub mod hub;
pub mod list;
pub mod picker;
pub mod preflight;
pub mod preset_selection;
pub mod restore;
pub mod set;
pub mod setup;
pub mod setup_executor;
pub mod status;
pub mod status_panel;
pub mod theme;
pub mod theme_selection;
pub mod tool_selection;
pub mod wizard_core;

use crate::error::Result;

/// Dispatch CLI commands based on parsed arguments
/// Note: setup handler is now called directly from main.rs with structured arguments
pub fn dispatch(command: &str, args: &[&str]) -> Result<()> {
    match command {
        "set" => set::handle(args),
        "status" => status::handle(args),
        "list" => list::handle(args),
        "restore" => restore::handle(args),
        _ => Err(crate::error::SlateError::Internal(format!(
            "Unknown command: {}",
            command
        ))),
    }
}
