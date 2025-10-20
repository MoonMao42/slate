use crate::error::{ThemeError, ThemeResult};
use atomic_write_file::AtomicWriteFile;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Information about a created backup
#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub tool: String,
    pub theme_name: String,
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub created_at: SystemTime,
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
}
