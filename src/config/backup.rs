use crate::error::{ThemeError, ThemeResult};
use atomic_write_file::AtomicWriteFile;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Information about a created backup
#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub tool: String,
    pub theme_name: String,
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub created_at: SystemTime,
}

/// Represents a single backup file with persisted metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RestoreEntry {
    pub tool_key: String, // e.g., "ghostty", "starship", "bat", "delta", "delta-gitconfig", "lazygit"
    pub display_tool: String, // e.g., "Ghostty", "Starship", "bat", "Delta", "Delta (.gitconfig)", "lazygit"
    pub original_path: PathBuf, // e.g., ~/.config/ghostty/config
    pub backup_path: PathBuf, // e.g., ~/.cache/slate/backups/restore_point_id/ghostty.backup
}

/// Represents a manifest-backed restore point with explicit directory structure
/// Storage: ~/.cache/slate/backups/<restore_point_id>/
/// manifest.toml (metadata)
/// ghostty.backup, starship.backup, bat.backup, delta.backup, delta-gitconfig.backup, lazygit.backup
#[derive(Debug, Clone)]
pub struct RestorePoint {
    pub id: String,             // e.g., "2026-04-09T10-00-00Z" (UUID-like, human-readable)
    pub theme_name: String,     // e.g., "Catppuccin Mocha"
    pub created_at: SystemTime, // When the slate set operation occurred
    pub entries: Vec<RestoreEntry>, // All backed-up files for this restore point
}

/// Explicit backup session created at the start of a set operation.
/// Groups all backups from a single set command under one restore_point_id.
/// Threaded through adapter trait to ensure consistent metadata persistence.
#[derive(Debug, Clone)]
pub struct BackupSession {
    pub restore_point_id: String, // e.g., "2026-04-09T10-00-00Z" (unique restore point ID)
    pub theme_name: String,       // e.g., "Catppuccin Mocha" (theme being applied)
    pub restore_point_dir: PathBuf, // Directory where this restore point's backups live
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RestoreManifestMetadata {
    id: String,
    theme_name: String,
    created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RestoreManifest {
    metadata: RestoreManifestMetadata,
    entries: Vec<RestoreEntry>,
}

static RESTORE_POINT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Get the backup directory path (~/.cache/slate/backups/)
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

    let backup_dir = cache_dir.join("slate").join("backups");

    // Create directory if missing
    fs::create_dir_all(&backup_dir).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to create backup directory: {}", e),
    })?;

    Ok(backup_dir)
}

/// Get a specific restore point directory path
fn restore_point_directory(restore_point_id: &str) -> ThemeResult<PathBuf> {
    let backup_dir = backup_directory()?;
    Ok(backup_dir.join(restore_point_id))
}

fn manifest_path(restore_point_dir: &Path) -> PathBuf {
    restore_point_dir.join("manifest.toml")
}

/// Generate a unique restore point ID (timestamp-based, human-readable)
fn generate_restore_point_id(now: SystemTime) -> String {
    let seq = RESTORE_POINT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{:04}",
        format_iso8601_timestamp(now),
        std::process::id(),
        seq % 10_000
    )
}

fn write_manifest_raw(manifest_path: &Path, manifest: &RestoreManifest) -> ThemeResult<()> {
    let content = toml::to_string_pretty(manifest).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to serialize manifest.toml: {}", e),
    })?;

    let mut file = AtomicWriteFile::open(manifest_path).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to open manifest.toml for writing: {}", e),
    })?;

    file.write_all(content.as_bytes())
        .map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to write manifest.toml: {}", e),
        })?;

    file.commit().map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to commit manifest.toml: {}", e),
    })
}

fn read_manifest_raw(manifest_path: &Path) -> ThemeResult<RestoreManifest> {
    let content = fs::read_to_string(manifest_path).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to read manifest.toml: {}", e),
    })?;

    toml::from_str(&content).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to parse manifest.toml: {}", e),
    })
}

fn manifest_to_restore_point(manifest: RestoreManifest) -> ThemeResult<RestorePoint> {
    Ok(RestorePoint {
        id: manifest.metadata.id,
        theme_name: manifest.metadata.theme_name,
        created_at: timestamp_from_string(&manifest.metadata.created_at)?,
        entries: manifest.entries,
    })
}

fn append_manifest_entry(session: &BackupSession, entry: &RestoreEntry) -> ThemeResult<()> {
    let manifest_path = manifest_path(&session.restore_point_dir);
    let mut manifest = read_manifest_raw(&manifest_path)?;

    if let Some(idx) = manifest
        .entries
        .iter()
        .position(|existing| existing.tool_key == entry.tool_key)
    {
        manifest.entries[idx] = entry.clone();
    } else {
        manifest.entries.push(entry.clone());
    }

    write_manifest_raw(&manifest_path, &manifest)
}

fn display_tool_name(entry: &RestoreEntry) -> String {
    match entry.tool_key.as_str() {
        "delta" | "delta-gitconfig" => "Delta".to_string(),
        _ => entry.display_tool.clone(),
    }
}

pub fn display_tools(entries: &[RestoreEntry]) -> Vec<String> {
    let mut tools = Vec::new();
    for entry in entries {
        let tool = display_tool_name(entry);
        if !tools.iter().any(|existing| existing == &tool) {
            tools.push(tool);
        }
    }
    tools
}

fn validate_restore_point_data(restore_point: &RestorePoint) -> ThemeResult<()> {
    if restore_point.entries.is_empty() {
        return Err(ThemeError::BackupError {
            reason: format!("No entries in restore point: {}", restore_point.id),
        });
    }

    let mut has_delta = false;
    let mut has_delta_gitconfig = false;

    for entry in &restore_point.entries {
        if entry.original_path.as_os_str().is_empty() {
            return Err(ThemeError::BackupError {
                reason: format!(
                    "Restore entry '{}' is missing original_path metadata",
                    entry.tool_key
                ),
            });
        }

        let path = &entry.backup_path;
        if !path.exists() {
            return Err(ThemeError::BackupError {
                reason: format!("Backup file not found: {}", path.display()),
            });
        }

        let metadata = fs::metadata(path).map_err(|e| ThemeError::BackupError {
            reason: format!("Cannot read backup file {}: {}", path.display(), e),
        })?;

        if metadata.len() == 0 {
            return Err(ThemeError::BackupError {
                reason: format!("Backup file is empty: {}", path.display()),
            });
        }

        if entry.tool_key == "delta" {
            has_delta = true;
        } else if entry.tool_key == "delta-gitconfig" {
            has_delta_gitconfig = true;
        }
    }

    if has_delta != has_delta_gitconfig {
        return Err(ThemeError::BackupError {
            reason: "Delta restore point is incomplete. Both delta and delta-gitconfig backups must exist together.".to_string(),
        });
    }

    Ok(())
}

/// Begin a new restore point session for a set operation.
/// Creates the restore_point_id directory and returns a BackupSession
/// that groups all backups from this set operation.
pub fn begin_restore_point(theme_name: &str) -> ThemeResult<BackupSession> {
    let created_at = SystemTime::now();

    for _ in 0..32 {
        let restore_point_id = generate_restore_point_id(created_at);
        let restore_point_dir = restore_point_directory(&restore_point_id)?;

        match fs::create_dir(&restore_point_dir) {
            Ok(()) => {
                let session = BackupSession {
                    restore_point_id,
                    theme_name: theme_name.to_string(),
                    restore_point_dir,
                };
                let manifest = RestoreManifest {
                    metadata: RestoreManifestMetadata {
                        id: session.restore_point_id.clone(),
                        theme_name: session.theme_name.clone(),
                        created_at: format_iso8601_timestamp(created_at),
                    },
                    entries: Vec::new(),
                };
                write_manifest_raw(&manifest_path(&session.restore_point_dir), &manifest)?;
                return Ok(session);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(ThemeError::BackupError {
                    reason: format!("Failed to create restore point directory: {}", e),
                });
            }
        }
    }

    Err(ThemeError::BackupError {
        reason: "Failed to allocate a unique restore point ID".to_string(),
    })
}

/// Create a backup of a config file within a restore session (manifest-backed).
/// Returns RestoreEntry with persisted original_path and backup_path.
/// This is the session-aware variant that should be used when a BackupSession exists.
/// Parameters:
/// - tool_key: Internal identifier for the backup (e.g., "ghostty", "delta", "delta-gitconfig")
/// - display_tool: User-facing tool name (e.g., "Ghostty", "Delta", "Delta (.gitconfig)")
/// - session: The BackupSession that groups this backup with others
/// - config_path: Path to the config file being backed up
pub fn create_backup_with_session(
    tool_key: &str,
    display_tool: &str,
    session: &BackupSession,
    config_path: &Path,
) -> ThemeResult<RestoreEntry> {
    // Read original config file
    let content = fs::read_to_string(config_path).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to read config: {}", e),
    })?;

    // Generate backup filename in the restore point directory
    // Format: {tool_key}.backup (simple, no timestamp needed)
    let backup_filename = format!("{}.backup", tool_key);
    let backup_path = session.restore_point_dir.join(&backup_filename);

    // Write backup file atomically
    let mut file = AtomicWriteFile::open(&backup_path).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to create backup file: {}", e),
    })?;

    file.write_all(content.as_bytes())
        .map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to write backup: {}", e),
        })?;

    file.commit().map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to commit backup: {}", e),
    })?;

    let restore_entry = RestoreEntry {
        tool_key: tool_key.to_string(),
        display_tool: display_tool.to_string(),
        original_path: config_path.to_path_buf(),
        backup_path,
    };

    append_manifest_entry(session, &restore_entry)?;

    Ok(restore_entry)
}

/// Create a backup of a config file before modification
/// Returns BackupInfo with both paths and timestamp
pub fn create_backup(tool: &str, theme_name: &str, config_path: &Path) -> ThemeResult<BackupInfo> {
    // Get backup directory
    let backup_dir = backup_directory()?;

    // Read original config file
    let content = fs::read_to_string(config_path).map_err(|e| ThemeError::BackupError {
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
    let mut file = AtomicWriteFile::open(&backup_path).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to create backup file: {}", e),
    })?;

    file.write_all(content.as_bytes())
        .map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to write backup: {}", e),
        })?;

    file.commit().map_err(|e| ThemeError::BackupError {
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
            reason: format!(
                "Invalid backup filename format (expected 3 parts): {}",
                filename
            ),
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
    if chars[4] != '-'
        || chars[7] != '-'
        || chars[10] != 'T'
        || chars[13] != '-'
        || chars[16] != '-'
    {
        return Err(ThemeError::BackupError {
            reason: format!("Invalid timestamp format: {}", ts_str),
        });
    }

    // Parse: YYYY-MM-DDTHH-MM-SS
    let year: u64 = ts_str[0..4].parse().map_err(|_| ThemeError::BackupError {
        reason: format!("Invalid year in timestamp: {}", ts_str),
    })?;
    let month: u64 = ts_str[5..7].parse().map_err(|_| ThemeError::BackupError {
        reason: format!("Invalid month in timestamp: {}", ts_str),
    })?;
    let day: u64 = ts_str[8..10].parse().map_err(|_| ThemeError::BackupError {
        reason: format!("Invalid day in timestamp: {}", ts_str),
    })?;
    let hour: u64 = ts_str[11..13]
        .parse()
        .map_err(|_| ThemeError::BackupError {
            reason: format!("Invalid hour in timestamp: {}", ts_str),
        })?;
    let minute: u64 = ts_str[14..16]
        .parse()
        .map_err(|_| ThemeError::BackupError {
            reason: format!("Invalid minute in timestamp: {}", ts_str),
        })?;
    let second: u64 = ts_str[17..19]
        .parse()
        .map_err(|_| ThemeError::BackupError {
            reason: format!("Invalid second in timestamp: {}", ts_str),
        })?;

    // Convert to Unix timestamp
    let days_since_epoch =
        days_from_unix_epoch(year, month, day).ok_or_else(|| ThemeError::BackupError {
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
/// Scans ~/.cache/slate/backups/ for restore_point_id directories with manifest.toml
/// Returns Vec<RestorePoint> sorted by creation time descending (newest first)
pub fn list_restore_points() -> ThemeResult<Vec<RestorePoint>> {
    let backup_dir = backup_directory()?;

    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&backup_dir).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to read backup directory: {}", e),
    })?;

    let mut restore_points = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to read backup directory entry: {}", e),
        })?;

        let path = entry.path();

        // Only process directories
        if !path.is_dir() {
            continue;
        }

        // Get the directory name as restore_point_id
        let _restore_point_id = match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        // Check if manifest.toml exists
        let manifest_path = path.join("manifest.toml");
        if !manifest_path.exists() {
            // Not a valid restore point directory (no manifest)
            continue;
        }

        // Try to read and parse the manifest
        match read_manifest(&manifest_path) {
            Ok(restore_point) => {
                if validate_restore_point_data(&restore_point).is_ok() {
                    restore_points.push(restore_point);
                }
            }
            Err(_) => {
                // Skip invalid manifests
                continue;
            }
        }
    }

    // Sort by created_at descending (newest first)
    restore_points.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(restore_points)
}

/// Read and parse a manifest.toml file
fn read_manifest(manifest_path: &Path) -> ThemeResult<RestorePoint> {
    let restore_point = manifest_to_restore_point(read_manifest_raw(manifest_path)?)?;
    if restore_point.entries.is_empty() {
        return Err(ThemeError::BackupError {
            reason: "manifest.toml has empty entries array".to_string(),
        });
    }
    Ok(restore_point)
}

/// Validate a restore point before any restore operation
/// Checks:
/// - Restore point directory exists
/// - manifest.toml exists and is valid
/// - At least one backup file exists
/// - All backup files are readable and non-empty
/// - If delta file exists, delta-gitconfig must also exist 
pub fn validate_restore_point(restore_point_id: &str) -> ThemeResult<()> {
    let restore_point = get_restore_point(restore_point_id)?;
    validate_restore_point_data(&restore_point)
}

/// Format SystemTime as ISO8601 with Z suffix, colons escaped to dashes
fn format_iso8601_timestamp(time: SystemTime) -> String {
    use std::time::UNIX_EPOCH;

    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();

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
        assert!(path.to_string_lossy().contains("slate"));
        assert!(path.to_string_lossy().contains("backups"));
    }

    #[test]
    fn test_iso8601_timestamp_format() {
        let timestamp = format_iso8601_timestamp(SystemTime::UNIX_EPOCH);
        assert!(timestamp.starts_with("1970-"));
        assert!(timestamp.ends_with("Z"));
        assert!(!timestamp.contains(":"));
        assert_eq!(timestamp.len(), 20);
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
        let result = list_restore_points();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_restore_point_nonexistent() {
        let result = validate_restore_point("2026-04-09T10-00-00Z");
        assert!(result.is_err());
    }

    #[test]
    fn test_restore_entry_serialization() {
        let entry = RestoreEntry {
            tool_key: "ghostty".to_string(),
            display_tool: "Ghostty".to_string(),
            original_path: PathBuf::from("~/.config/ghostty/config"),
            backup_path: PathBuf::from(
                "~/.cache/slate/backups/2026-04-09T10-00-00Z/ghostty.backup",
            ),
        };
        let serialized = toml::to_string(&entry);
        assert!(serialized.is_ok());
    }

    #[test]
    fn test_restore_point_structure() {
        let restore_point = RestorePoint {
            id: "2026-04-09T10-00-00Z".to_string(),
            theme_name: "Catppuccin Mocha".to_string(),
            created_at: SystemTime::now(),
            entries: vec![],
        };
        assert_eq!(restore_point.id, "2026-04-09T10-00-00Z");
        assert_eq!(restore_point.theme_name, "Catppuccin Mocha");
        assert!(restore_point.entries.is_empty());
    }

    #[test]
    fn test_display_tools_groups_delta_pair_under_one_label() {
        let entries = vec![
            RestoreEntry {
                tool_key: "delta".to_string(),
                display_tool: "Delta".to_string(),
                original_path: PathBuf::from("/tmp/delta"),
                backup_path: PathBuf::from("/tmp/delta.backup"),
            },
            RestoreEntry {
                tool_key: "delta-gitconfig".to_string(),
                display_tool: "Delta (.gitconfig)".to_string(),
                original_path: PathBuf::from("/tmp/gitconfig"),
                backup_path: PathBuf::from("/tmp/delta-gitconfig.backup"),
            },
        ];

        assert_eq!(display_tools(&entries), vec!["Delta".to_string()]);
    }

    #[test]
    fn test_validate_restore_point_rejects_incomplete_delta_pair() {
        let temp_dir = tempfile::tempdir().unwrap();
        let backup_path = temp_dir.path().join("delta-gitconfig.backup");
        fs::write(
            &backup_path,
            "[include]\n\tpath = \"/tmp/delta/config.gitconfig\"\n",
        )
        .unwrap();

        let restore_point = RestorePoint {
            id: "restore-point".to_string(),
            theme_name: "catppuccin-mocha".to_string(),
            created_at: SystemTime::now(),
            entries: vec![RestoreEntry {
                tool_key: "delta-gitconfig".to_string(),
                display_tool: "Delta".to_string(),
                original_path: PathBuf::from("/tmp/gitconfig"),
                backup_path,
            }],
        };

        let error = validate_restore_point_data(&restore_point).unwrap_err();
        assert!(error
            .to_string()
            .contains("Delta restore point is incomplete"));
    }
}

/// Get a specific restore point by ID
pub fn get_restore_point(restore_point_id: &str) -> ThemeResult<RestorePoint> {
    let restore_point_dir = restore_point_directory(restore_point_id)?;
    let manifest_path = manifest_path(&restore_point_dir);

    if !manifest_path.exists() {
        return Err(ThemeError::BackupError {
            reason: format!("Restore point not found: {}", restore_point_id),
        });
    }

    let restore_point = read_manifest(&manifest_path)?;
    if restore_point.id != restore_point_id {
        return Err(ThemeError::BackupError {
            reason: format!(
                "Restore point metadata mismatch: expected {}, found {}",
                restore_point_id, restore_point.id
            ),
        });
    }

    Ok(restore_point)
}

/// Restore a specific restore point by writing backed-up files back to their original paths
pub fn restore_restore_point(
    restore_point_id: &str,
) -> ThemeResult<crate::adapter::ApplyThemeResult> {
    let restore_point = get_restore_point(restore_point_id)?;
    validate_restore_point_data(&restore_point)?;

    let mut states: HashMap<String, Option<String>> = HashMap::new();
    let mut ordered_tools = display_tools(&restore_point.entries);

    for entry in &restore_point.entries {
        let backup_content =
            fs::read_to_string(&entry.backup_path).map_err(|e| ThemeError::BackupError {
                reason: format!(
                    "Failed to read backup file {}: {}",
                    entry.backup_path.display(),
                    e
                ),
            })?;

        let state = states.entry(display_tool_name(entry)).or_insert(None);
        match restore_entry(&entry.original_path, &backup_content) {
            Ok(_) => {}
            Err(e) => *state = Some(e.to_string()),
        }
    }

    ordered_tools.sort();

    let mut result = crate::adapter::ApplyThemeResult::default();
    for tool in ordered_tools {
        match states.remove(&tool) {
            Some(Some(error)) => result.failed.push((tool, error)),
            _ => result.successful.push(tool),
        }
    }

    Ok(result)
}

/// Helper to restore a single entry to its persisted original_path
fn restore_entry(original_path: &Path, content: &str) -> ThemeResult<()> {
    // Write content atomically
    let mut file = AtomicWriteFile::open(original_path).map_err(|e| ThemeError::BackupError {
        reason: format!(
            "Failed to open config for writing {}: {}",
            original_path.display(),
            e
        ),
    })?;

    file.write_all(content.as_bytes())
        .map_err(|e| ThemeError::BackupError {
            reason: format!(
                "Failed to write restored content to {}: {}",
                original_path.display(),
                e
            ),
        })?;

    file.commit().map_err(|e| ThemeError::BackupError {
        reason: format!(
            "Failed to commit restored file {}: {}",
            original_path.display(),
            e
        ),
    })
}

/// Delete a specific restore point by ID (removes entire restore_point_id directory)
pub fn delete_restore_point(restore_point_id: &str) -> ThemeResult<usize> {
    let restore_point_dir = restore_point_directory(restore_point_id)?;

    if !restore_point_dir.exists() {
        return Err(ThemeError::BackupError {
            reason: format!("Restore point directory not found: {}", restore_point_id),
        });
    }

    // Count files before deletion
    let entries = fs::read_dir(&restore_point_dir).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to read restore point directory: {}", e),
    })?;

    let file_count: usize = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .count();

    // Remove entire directory
    fs::remove_dir_all(&restore_point_dir).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to delete restore point {}: {}", restore_point_id, e),
    })?;

    Ok(file_count)
}

/// Delete all restore points (removes all restore_point_id directories)
pub fn clear_all_restore_points() -> ThemeResult<usize> {
    let backup_dir = backup_directory()?;
    if !backup_dir.exists() {
        return Ok(0);
    }

    let entries = fs::read_dir(&backup_dir).map_err(|e| ThemeError::BackupError {
        reason: format!("Failed to read backup directory: {}", e),
    })?;

    let mut deleted_items = 0;
    for entry in entries {
        let entry = entry.map_err(|e| ThemeError::BackupError {
            reason: format!("Failed to read backup directory entry: {}", e),
        })?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| ThemeError::BackupError {
                reason: format!(
                    "Failed to delete backup directory {}: {}",
                    path.display(),
                    e
                ),
            })?;
            deleted_items += 1;
        } else if path.is_file() {
            fs::remove_file(&path).map_err(|e| ThemeError::BackupError {
                reason: format!("Failed to delete backup file {}: {}", path.display(), e),
            })?;
            deleted_items += 1;
        }
    }

    Ok(deleted_items)
}
