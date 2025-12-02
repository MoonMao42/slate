use crate::brand::language::Language;
use crate::error::Result;

/// Handle `slate restore [backup-id]` command
pub fn handle(args: &[&str]) -> Result<()> {
    // will implement full restore flow

    if let Some(backup_id) = args.first() {
        println!("{}", Language::restore_pending_backup(backup_id));
    } else {
        println!("{}", Language::RESTORE_PICKER_PENDING);
    }

    Ok(())
}
