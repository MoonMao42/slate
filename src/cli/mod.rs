pub mod setup;
pub mod set;
pub mod status;
pub mod list;
pub mod restore;
pub mod wizard_core;
pub mod font_detection;
pub mod tool_selection;
pub mod preset_selection;
pub mod font_selection;
pub mod theme_selection;
pub mod preflight;
pub mod failure_handler;
pub mod setup_executor;

use crate::error::Result;

/// Dispatch CLI commands based on parsed arguments
/// Note: setup handler is now called directly from main.rs with structured arguments
pub fn dispatch(command: &str, args: &[&str]) -> Result<()> {
    match command {
        "set" => set::handle(args),
        "status" => status::handle(args),
        "list" => list::handle(args),
        "restore" => restore::handle(args),
        _ => Err(crate::error::SlateError::Internal(
            format!("Unknown command: {}", command)
        )),
    }
}
