use crate::error::Result;
use crate::brand::language::Language;

/// Handle `slate list` command
pub fn handle(_args: &[&str]) -> Result<()> {
    // will implement full theme listing
    println!("{}", Language::LIST_HEADER);
    println!("{}", Language::LIST_PENDING);
    Ok(())
}
