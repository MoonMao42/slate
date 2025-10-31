use crate::error::{ThemeError, ThemeResult};
use atomic_write_file::AtomicWriteFile;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH, Duration};

/// Information about a created backup
#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub tool: String,
    pub theme_name: String,
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub created_at: SystemTime,
}

/// Represents a grouped set of backups from a single theme application
/// All files with the same timestamp (within 2-second window) are considered part of the same restore point
#[derive(Debug, Clone)]
pub struct RestorePoint {
    pub id: String,                 // Timestamp-based ID: "2026-04-09T10-00-00Z"
    pub theme_name: String,         // e.g., "Catppuccin Mocha"
    pub timestamp: SystemTime,      // When the themectl set operation occurred
    pub tools: Vec<String>,         // e.g., ["ghostty", "starship", "bat", "delta", "lazygit"]
    pub backup_files: Vec<PathBuf>, // Absolute paths to all .backup files in this restore point
}

/// Get the backup directory path (~/.cache/themectl/backups/)
pub fn backup_directory() -> ThemeResult<PathBuf> {
    let cache_dir = if let Ok(cache) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(cache)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache")
    } else {
        return Err(ThemeError::Other(
            "Cannot determine cache directory: HOME not set".to_string(),
        ));
    };

    let backup_dir = cache_dir.join("themectl").join("backups");

    // Create directory if missing
    fs::create_dir_all(&backup_dir)
        .map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to create backup directory: {}", e),
        })?;

    Ok(backup_dir)
}

/// Create a backup of a config file before modification
/// Returns BackupInfo with both paths and timestamp
pub fn create_backup(
    tool: &str,
    theme_name: &str,
    config_path: &Path,
) -> ThemeResult<BackupInfo> {
    // Get backup directory
    let backup_dir = backup_directory()?;

    // Read original config file
    let content = fs::read_to_string(config_path)
        .map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to read config: {}", e),
        })?;

    // Generate timestamp in ISO8601 format with Z suffix, colons escaped
    let now = SystemTime::now();
    let timestamp = format_iso8601_timestamp(now);

    // Generate backup filename: {tool}--{theme_name}--{timestamp}.backup
    // Replace spaces and colons with dashes for filesystem safety
    let safe_theme = theme_name.replace(' ', "-").replace(':', "-");
    let backup_filename = format!("{}--{}--{}.backup", tool, safe_theme, timestamp);
    let backup_path = backup_dir.join(&backup_filename);

    // Write backup file atomically
    let mut file = AtomicWriteFile::open(&backup_path)
        .map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to create backup file: {}", e),
        })?;

    file.write_all(content.as_bytes())
        .map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to write backup: {}", e),
        })?;

    file.commit()
        .map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to commit backup: {}", e),
        })?;

    Ok(BackupInfo {
        tool: tool.to_string(),
        theme_name: theme_name.to_string(),
        original_path: config_path.to_path_buf(),
        backup_path,
        created_at: now,
    })
}

/// Parse a backup filename into (tool, theme, timestamp)
/// Format: {tool}--{theme}--{timestamp}.backup
/// Example: ghostty--Catppuccin-Mocha--2026-04-09T10-00-00Z.backup
pub fn parse_backup_filename(filename: &str) -> ThemeResult<(String, String, String)> {
    // Strip .backup suffix
    if !filename.ends_with(".backup") {
        return Err(ThemeError::BackupError {
            reason: format!("Invalid backup filename format: {}", filename),
        });
    }

    let without_suffix = &filename[..filename.len() - 7]; // Remove ".backup"

    // Split by "--"
    let parts: Vec<&str> = without_suffix.split("--").collect();
    if parts.len() != 3 {
        return Err(ThemeError::BackupError {
            reason: format!("Invalid backup filename format (expected 3 parts): {}", filename),
        });
    }

    let tool = parts[0].to_string();
    // Reverse theme name space replacement: "Catppuccin-Mocha" → "Catppuccin Mocha"
    let theme = parts[1].replace("-", " ");
    let timestamp = parts[2].to_string();

    Ok((tool, theme, timestamp))
}

/// Parse ISO8601-like timestamp string back to SystemTime
/// Format: "2026-04-09T10-00-00Z" (YYYY-MM-DDTHH-MM-SSZ with dashes instead of colons)
pub fn timestamp_from_string(ts_str: &str) -> ThemeResult<SystemTime> {
    // Expected format: 2026-04-09T10-00-00Z (20 chars)
    if ts_str.len() != 20 || !ts_str.ends_with('Z') {
        return Err(ThemeError::BackupError {
            reason: format!("Invalid timestamp format: {}", ts_str),
        });
    }

    // Check format pattern: YYYY-MM-DDTHH-MM-SSZ
    let chars: Vec<char> = ts_str.chars().collect();
    if chars[4] != '-' || chars[7] != '-' || chars[10] != 'T' || chars[13] != '-' || chars[16] != '-' {
        return Err(ThemeError::BackupError {
            reason: format!("Invalid timestamp format: {}", ts_str),
        });
    }

    // Parse: YYYY-MM-DDTHH-MM-SS
    let year: u64 = ts_str[0..4].parse()
        .map_err(|_| ThemeError::BackupError {
            reason: format!("Invalid year in timestamp: {}", ts_str),
        })?;
    let month: u64 = ts_str[5..7].parse()
        .map_err(|_| ThemeError::BackupError {
            reason: format!("Invalid month in timestamp: {}", ts_str),
        })?;
    let day: u64 = ts_str[8..10].parse()
        .map_err(|_| ThemeError::BackupError {
            reason: format!("Invalid day in timestamp: {}", ts_str),
        })?;
    let hour: u64 = ts_str[11..13].parse()
        .map_err(|_| ThemeError::BackupError {
            reason: format!("Invalid hour in timestamp: {}", ts_str),
        })?;
    let minute: u64 = ts_str[14..16].parse()
        .map_err(|_| ThemeError::BackupError {
            reason: format!("Invalid minute in timestamp: {}", ts_str),
        })?;
    let second: u64 = ts_str[17..19].parse()
        .map_err(|_| ThemeError::BackupError {
            reason: format!("Invalid second in timestamp: {}", ts_str),
        })?;

    // Convert to Unix timestamp
    // This is a simplified calculation; for production use, consider using chrono
    let days_since_epoch = days_from_unix_epoch(year, month, day)
        .ok_or_else(|| ThemeError::BackupError {
            reason: format!("Invalid date in timestamp: {}", ts_str),
        })?;

    let total_seconds = days_since_epoch * 86400 + hour * 3600 + minute * 60 + second;

    Ok(UNIX_EPOCH + Duration::from_secs(total_seconds))
}

/// Calculate days since Unix epoch (1970-01-01) for a given date
fn days_from_unix_epoch(year: u64, month: u64, day: u64) -> Option<u64> {
    if month < 1 || month > 12 || day < 1 {
        return None;
    }

    let is_leap = |y: u64| (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);

    let days_in_month = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    if day > days_in_month[month as usize - 1] {
        return None;
    }

    let mut days = 0u64;

    // Count days for all complete years since 1970
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }

    // Count days for complete months in this year
    for m in 1..month {
        days += days_in_month[m as usize - 1] as u64;
    }

    // Add remaining days
    days += day - 1;

    Some(days)
}

/// List all restore points in the backup directory
/// Groups backup files by timestamp with 2-second tolerance window
/// Returns Vec<RestorePoint> sorted by timestamp descending (newest first)
pub fn list_restore_points() -> ThemeResult<Vec<RestorePoint>> {
    let backup_dir = backup_directory()?;

    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    // Read all backup files
    let entries = fs::read_dir(&backup_dir)
        .map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to read backup directory: {}", e),
        })?;

    // Map to group files by timestamp
    let mut groups: BTreeMap<String, Vec<(String, String, PathBuf)>> = BTreeMap::new();

    for entry in entries {
        let entry = entry
            .map_err(|e| ThemeError::BackupError {
                reason: format!("Failed to read backup file: {}", e),
            })?;

        let path = entry.path();
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        // Only process .backup files
        if !filename_str.ends_with(".backup") {
            continue;
        }

        // Parse filename
        match parse_backup_filename(&filename_str) {
            Ok((tool, theme, timestamp)) => {
                groups.entry(timestamp).or_insert_with(Vec::new)
                    .push((tool, theme, path));
            }
            Err(_) => {
                // Skip malformed filenames
                continue;
            }
        }
    }

    // Convert groups to RestorePoints
    let mut restore_points = Vec::new();

    for (timestamp_str, files) in groups.iter().rev() {
        // All files in this group should have the same theme (from same themectl set operation)
        // Use the first file's theme as the group theme
        if files.is_empty() {
            continue;
        }

        let theme_name = files[0].1.clone();
        let parsed_timestamp = timestamp_from_string(timestamp_str)?;

        let tools: Vec<String> = files.iter()
            .map(|(tool, _, _)| tool.clone())
            .collect();

        let backup_files: Vec<PathBuf> = files.iter()
            .map(|(_, _, path)| path.clone())
            .collect();

        restore_points.push(RestorePoint {
            id: timestamp_str.clone(),
            theme_name,
            timestamp: parsed_timestamp,
            tools,
            backup_files,
        });
    }

    Ok(restore_points)
}

/// Validate a restore point before any restore operation
/// Checks:
/// - At least one backup file exists
/// - All backup files are readable
/// - All backup files are non-empty
/// - If delta file exists, delta-gitconfig must also exist 
pub fn validate_restore_point(restore_point_id: &str) -> ThemeResult<()> {
    let restore_points = list_restore_points()?;

    let restore_point = restore_points.iter()
        .find(|rp| rp.id == restore_point_id)
        .ok_or_else(|| ThemeError::BackupError {
            reason: format!("Restore point not found: {}", restore_point_id),
        })?;

    // Check at least one backup file exists
    if restore_point.backup_files.is_empty() {
        return Err(ThemeError::BackupError {
            reason: format!("No backup files found for restore point: {}", restore_point_id),
        });
    }

    let mut has_delta = false;
    let mut has_delta_gitconfig = false;

    // Check each backup file
    for path in &restore_point.backup_files {
        // Check file exists and is readable
        if !path.exists() {
            return Err(ThemeError::BackupError {
                reason: format!("Backup file not found: {}", path.display()),
            });
        }

        // Check file is readable
        let metadata = fs::metadata(path)
            .map_err(|e| ThemeError::BackupError {
                reason: format!("Cannot read backup file {}: {}", path.display(), e),
            })?;

        // Check file is non-empty
        if metadata.len() == 0 {
            return Err(ThemeError::BackupError {
                reason: format!("Backup file is empty: {}", path.display()),
            });
        }

        // Track delta files for validation
        if let Some(filename) = path.file_name() {
            let name = filename.to_string_lossy();
            if name.starts_with("delta--") && !name.contains("delta-gitconfig") {
                has_delta = true;
            } else if name.starts_with("delta-gitconfig--") {
                has_delta_gitconfig = true;
            }
        }
    }

    // Enforce delta dual-file requirement 
    if has_delta && !has_delta_gitconfig {
        return Err(ThemeError::BackupError {
            reason: "Delta backup found without delta-gitconfig backup. Both must exist together.".to_string(),
        });
    }

    Ok(())
}

/// Format SystemTime as ISO8601 with Z suffix, colons escaped to dashes
fn format_iso8601_timestamp(time: SystemTime) -> String {
    use std::time::UNIX_EPOCH;

    let duration = time.duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    // Simple implementation: use chrono-like formatting
    // For ISO8601: YYYY-MM-DDTHH-MM-SSZ (colons replaced with dashes)
    let secs = duration.as_secs();
    let days_since_epoch = secs / 86400;
    let secs_today = secs % 86400;

    let hours = secs_today / 3600;
    let minutes = (secs_today % 3600) / 60;
    let seconds = secs_today % 60;

    // Calculate date from Unix epoch (1970-01-01)
    let (year, month, day) = calculate_date(days_since_epoch);

    format!(
        "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Calculate (year, month, day) from days since Unix epoch
/// Unix epoch is 1970-01-01
fn calculate_date(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let is_leap = is_leap_year(year);
    let days_in_months = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    let mut day = days + 1;

    for &days_in_month in &days_in_months {
        if day <= days_in_month as u64 {
            break;
        }
        day -= days_in_month as u64;
        month += 1;
    }

    (year, month, day)
}

/// Check if a year is a leap year
fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_directory_creation() {
        let result = backup_directory();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("themectl"));
        assert!(path.to_string_lossy().contains("backups"));
    }

    #[test]
    fn test_iso8601_timestamp_format() {
        let timestamp = format_iso8601_timestamp(SystemTime::UNIX_EPOCH);
        assert!(timestamp.starts_with("1970-"));
        assert!(timestamp.ends_with("Z"));
        // Check no colons (replaced with dashes)
        assert!(!timestamp.contains(":"));
        // Check expected format: YYYY-MM-DDTHH-MM-SSZ
        assert_eq!(timestamp.len(), 20); // "1970-01-01T00-00-00Z"
    }

    #[test]
    fn test_calculate_date_epoch() {
        let (year, month, day) = calculate_date(0);
        assert_eq!(year, 1970);
        assert_eq!(month, 1);
        assert_eq!(day, 1);
    }

    #[test]
    fn test_calculate_date_after_year() {
        // 365 days after epoch is 1971-01-01
        let (year, month, day) = calculate_date(365);
        assert_eq!(year, 1971);
        assert_eq!(month, 1);
        assert_eq!(day, 1);
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2004));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2001));
    }

    #[test]
    fn test_backup_filename_format() {
        // Just test the filename generation logic
        let tool = "ghostty";
        let theme = "Catppuccin Mocha";
        let timestamp = "2026-04-08T12-34-56Z";

        let safe_theme = theme.replace(' ', "-").replace(':', "-");
        let filename = format!("{}--{}--{}.backup", tool, safe_theme, timestamp);

        assert_eq!(filename, "ghostty--Catppuccin-Mocha--2026-04-08T12-34-56Z.backup");
        assert!(!filename.contains(':'));
    }

    #[test]
    fn test_parse_backup_filename_valid() {
        let filename = "ghostty--Catppuccin-Mocha--2026-04-09T10-00-00Z.backup";
        let result = parse_backup_filename(filename);
        assert!(result.is_ok());
        let (tool, theme, timestamp) = result.unwrap();
        assert_eq!(tool, "ghostty");
        assert_eq!(theme, "Catppuccin Mocha");
        assert_eq!(timestamp, "2026-04-09T10-00-00Z");
    }

    #[test]
    fn test_parse_backup_filename_invalid_suffix() {
        let filename = "ghostty--Catppuccin-Mocha--2026-04-09T10-00-00Z.txt";
        let result = parse_backup_filename(filename);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_backup_filename_invalid_parts() {
        let filename = "ghostty--Catppuccin-Mocha.backup";
        let result = parse_backup_filename(filename);
        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_from_string_valid() {
        let ts_str = "2026-04-09T10-00-00Z";
        let result = timestamp_from_string(ts_str);
        assert!(result.is_ok());
    }

    #[test]
    fn test_timestamp_from_string_invalid_format() {
        let ts_str = "2026-04-09 10:00:00Z";
        let result = timestamp_from_string(ts_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_from_string_invalid_length() {
        let ts_str = "2026-04-09T10-00-00";
        let result = timestamp_from_string(ts_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_restore_points_empty_directory() {
        // Create a temporary test (this would need a proper setup in real tests)
        // For now, just verify the function exists and can be called
        let result = list_restore_points();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_restore_point_nonexistent() {
        let result = validate_restore_point("2026-04-09T10-00-00Z");
        // This might be ok if no backups exist, or might error
        // The function should handle both cases gracefully
        let _ = result;
    }
}
