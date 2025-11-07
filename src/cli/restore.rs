use crate::error::Result;

/// Handle `slate restore [backup-id]` command
pub fn handle(args: &[&str]) -> Result<()> {
    // will implement full restore flow

    if let Some(backup_id) = args.first() {
        println!("Restoring backup: {} — implemented in ", backup_id);
    } else {
        println!("Restore point selection — implemented in ");
    }

    Ok(())
}
