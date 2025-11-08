pub mod setup;
pub mod set;
pub mod status;
pub mod list;
pub mod restore;
pub mod init;
pub mod wizard_core;
pub mod font_detection;

use crate::error::Result;

/// Dispatch CLI commands based on parsed arguments
pub fn dispatch(command: &str, args: &[&str]) -> Result<()> {
    match command {
        "setup" => setup::handle(args),
        "set" => set::handle(args),
        "status" => status::handle(args),
        "list" => list::handle(args),
        "restore" => restore::handle(args),
        "init" => init::handle(args),
        _ => Err(crate::error::SlateError::Internal(
            format!("Unknown command: {}", command)
        )),
    }
}
