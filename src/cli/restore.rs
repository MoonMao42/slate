use crate::brand::language::Language;
use crate::config::{
    delete_restore_point, execute_restore, get_restore_point, is_baseline_restore_point,
    list_restore_points,
};
use crate::error::Result;
use cliclack::{confirm, select};
/// Handle `slate restore [ID] [--list] [--delete ID]` with structured clap arguments.
pub fn handle(restore_id: Option<&str>, list_mode: bool, delete_id: Option<&str>) -> Result<()> {
    if let Some(id) = delete_id {
        handle_delete(id)?;
        return Ok(());
    }

    if list_mode {
        handle_list()?;
        return Ok(());
    }

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

    // Baseline restore gets an extra warning
    let confirmed = if is_baseline_restore_point(&restore_point) {
        confirm("⚠ This will restore all config files to their state BEFORE slate was installed. Any manual changes you made since then will be lost. Continue?")
            .initial_value(false)
            .interact()?
    } else {
        confirm(format!(
            "Restore to {}? This will modify your configuration files.",
            restore_point.theme_name
        ))
        .interact()?
    };

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
                println!(
                    "{}",
                    Language::restore_receipt_failures(&result.display_tool, error)
                );
            }
        }
    }

    // Re-apply the restored theme so managed files match the restored state
    let env = crate::env::SlateEnv::from_process()?;
    let config = crate::config::ConfigManager::with_env(&env)?;
    if let Ok(Some(theme_id)) = config.get_current_theme() {
        let registry = crate::theme::ThemeRegistry::new()?;
        if let Some(theme) = registry.get(&theme_id) {
            println!("✓ Re-applying theme: {}", theme.name);
            // Apply without snapshotting again (we just restored)
            let report = crate::cli::theme_apply::ThemeApplyCoordinator::new(&env).apply(theme)?;
            crate::cli::theme_apply::log_apply_report(&report);
        }
    }

    // Sync watcher state: stop if auto-theme is now disabled, restart if enabled
    let _ = crate::platform::dark_mode_notify::stop();
    if config.is_auto_theme_enabled().unwrap_or(false) {
        let _ = crate::platform::dark_mode_notify::start(&config);
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
    ))
    .interact()?;

    if !confirmed {
        println!("Deletion cancelled.");
        return Ok(());
    }

    // Delete the restore point
    delete_restore_point(restore_id)?;
    println!("{}", Language::RESTORE_DELETED);

    Ok(())
}
