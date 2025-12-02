use crate::brand::language::Language;
use crate::error::Result;

/// Handle `slate status` command
pub fn handle(_args: &[&str]) -> Result<()> {
    // will implement full status display
    println!("{}", Language::STATUS_PENDING);
    Ok(())
}
