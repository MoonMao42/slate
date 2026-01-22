use crate::brand::language::Language;
use crate::config::{execute_restore, get_restore_point, is_baseline_restore_point, list_restore_points, delete_restore_point};
use crate::error::Result;
use cliclack::{confirm, select};

/// Handle `slate restore [ID] [--list] [--delete ID]` command
pub fn handle(args: &[&str]) -> Result<()> {
    // Parse arguments for --list, --delete
    let mut restore_id: Option<&str> = None;
    let mut list_mode = false;
    let mut delete_id: Option<&str> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--list" => list_mode = true,
            "--delete" => {
                // Next arg is the ID to delete
                if i + 1 < args.len() {
                    delete_id = Some(args[i + 1]);
                    i += 1;
                }
            }
            id if !id.starts_with('-') => {
                restore_id = Some(id);
            }
            _ => {}
        }
        i += 1;
    }

    // Handle --delete flow
    if let Some(id) = delete_id {
        handle_delete(id)?;
        return Ok(());
    }

    // Handle --list flow
    if list_mode {
        handle_list()?;
        return Ok(());
    }

    // Handle direct restore with ID or picker
    if let Some(id) = restore_id {
        handle_restore_direct(id)?;
    } else {
        handle_restore_picker()?;
    }

    Ok(())
}

/// Handle list restore points
fn handle_list() -> Result<()> {
    let restore_points = list_restore_points()?;

    if restore_points.is_empty() {
        println!("{}", Language::RESTORE_NO_POINTS);
        return Ok(());
    }

    println!("{}", Language::RESTORE_LIST_HEADER);
    for point in restore_points {
        // Skip pre-restore snapshots in list
        if point.theme_name.starts_with("pre-restore-snapshot") {
            continue;
        }
        let summary =
            Language::restore_point_summary(&point.id, &point.theme_name, point.entries.len());
        println!("{}", summary);
    }

    Ok(())
}

/// Handle direct restore with explicit ID
fn handle_restore_direct(restore_id: &str) -> Result<()> {
    // Validate the restore point exists
    let restore_point = get_restore_point(restore_id)?;

    // Prevent restoring to baseline
    if is_baseline_restore_point(&restore_point) {
        println!("✗ Cannot restore to baseline. This is a protected snapshot.");
        return Ok(());
    }

    // Confirm restore
    let confirmed =
        confirm(format!("Restore to {}? This will modify your configuration files.", restore_point.theme_name))
            .interact()?;

    if !confirmed {
        println!("Restore cancelled.");
        return Ok(());
    }

    // Execute restore
    let receipt = execute_restore(restore_id)?;

    // Print receipt
    println!();
    println!("{}", Language::restore_receipt_header(&receipt.theme_name));
    println!(
        "{}",
        Language::restore_receipt_detail(receipt.success_count(), receipt.failure_count())
    );

    // Print failures if any
    if !receipt.failed_results().is_empty() {
        println!("\nPartially failed:");
        for result in receipt.failed_results() {
            if let Some(error) = &result.error {
                println!("{}", Language::restore_receipt_failures(&result.display_tool, error));
            }
        }
    }

    Ok(())
}

/// Handle interactive restore point picker
fn handle_restore_picker() -> Result<()> {
    let restore_points = list_restore_points()?;

    if restore_points.is_empty() {
        println!("{}", Language::RESTORE_NO_POINTS);
        return Ok(());
    }

    // Filter out pre-restore snapshots from user-facing picker
    let user_visible: Vec<_> = restore_points
        .iter()
        .filter(|p| !p.theme_name.starts_with("pre-restore-snapshot"))
        .collect();

    if user_visible.is_empty() {
        println!("{}", Language::RESTORE_NO_POINTS);
        return Ok(());
    }

    println!("{}", Language::RESTORE_HEADER);
    println!();

    // Build selection options with formatted labels
    // We need to own the strings to keep them alive during the items() call
    let options: Vec<(String, String, String)> = user_visible
        .iter()
        .map(|p| {
            (
                p.id.clone(),
                p.theme_name.clone(),
                format!("{} files", p.entries.len()),
            )
        })
        .collect();

    // Convert to borrowed slices for cliclack
    // Note: select().interact() returns the FIRST element of the tuple, not the index
    let select_items: Vec<(&str, &str, &str)> = options
        .iter()
        .map(|(id, theme, count_label)| (id.as_str(), theme.as_str(), count_label.as_str()))
        .collect();

    let selected_id = select("Choose restore point:")
        .items(&select_items)
        .interact()?;

    // selected_id is already the restore point ID from the first tuple element
    handle_restore_direct(selected_id)?;

    Ok(())
}

/// Handle deleting a restore point
fn handle_delete(restore_id: &str) -> Result<()> {
    // Validate the restore point exists first
    let restore_point = get_restore_point(restore_id)?;

    // Prevent deleting baseline
    if is_baseline_restore_point(&restore_point) {
        println!("✗ Cannot delete baseline. This is a protected snapshot.");
        return Ok(());
    }

    // Confirm deletion
    let confirmed = confirm(format!(
        "Delete restore point {}? This cannot be undone.",
        restore_point.id
    )).interact()?;

    if !confirmed {
        println!("Deletion cancelled.");
        return Ok(());
    }

    // Delete the restore point
    delete_restore_point(restore_id)?;
    println!("{}", Language::RESTORE_DELETED);

    Ok(())
}
