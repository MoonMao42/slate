use crate::error::Result;

/// Handle `slate init [shell]` command
/// Init subcommand scaffolded alongside setup/set/status/list/restore
pub fn handle(args: &[&str]) -> Result<()> {
    // will implement shell integration

    if let Some(shell) = args.first() {
        println!("Initializing {} shell integration — implemented in ", shell);
    } else {
        println!("Shell type not specified — implemented in ");
    }

    Ok(())
}
