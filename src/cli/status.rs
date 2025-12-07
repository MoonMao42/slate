use crate::error::Result;

/// Handle `slate status` command
pub fn handle(_args: &[&str]) -> Result<()> {
    super::status_panel::render()
}
