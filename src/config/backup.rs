use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use atomic_write_file::AtomicWriteFile;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginalFileState {
    Present,
    Absent,
}

impl Default for OriginalFileState {
    fn default() -> Self {
        Self::Present
    }
}

/// Represents a single backup file with persisted metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RestoreEntry {
    pub tool_key: String, // e.g., "ghostty", "starship", "bat", "delta", "delta-gitconfig", "lazygit"
    pub display_tool: String, // e.g., "Ghostty", "Starship", "bat", "Delta", "Delta (.gitconfig)", "lazygit"
    pub original_path: PathBuf, // e.g., ~/.config/ghostty/config
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<PathBuf>, // e.g., ~/.cache/slate/backups/restore_point_id/ghostty.backup
    #[serde(default)]
    pub original_state: OriginalFileState,
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
    pub is_baseline: bool, // If true, this is the pre-slate baseline; protected from  overwrite
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
    #[serde(default)]
    is_baseline: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RestoreManifest {
    metadata: RestoreManifestMetadata,
    entries: Vec<RestoreEntry>,
}

static RESTORE_POINT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Get the backup directory path (~/.cache/slate/backups/)
pub fn backup_directory() -> Result<PathBuf> {
    let env = SlateEnv::from_process().map_err(|_| {
        SlateError::Internal("Cannot initialize SlateEnv to determine cache directory".to_string())
    })?;
    backup_directory_with_env(&env)
}

/// Get the backup directory path with injected SlateEnv (preferred for testing)
pub fn backup_directory_with_env(env: &SlateEnv) -> Result<PathBuf> {
    let backup_dir = env.slate_cache_dir().join("backups");

    // Create directory if missing
    fs::create_dir_all(&backup_dir).map_err(|e| {
        SlateError::BackupFailed(format!("Failed to create backup directory: {}", e))
    })?;

    Ok(backup_dir)
}

/// Get a specific restore point directory path
fn restore_point_directory(restore_point_id: &str) -> Result<PathBuf> {
    let backup_dir = backup_directory()?;
    resolve_restore_point_directory(&backup_dir, restore_point_id)
}

/// Get a specific restore point directory path with injected SlateEnv
fn restore_point_directory_with_env(env: &SlateEnv, restore_point_id: &str) -> Result<PathBuf> {
    let backup_dir = backup_directory_with_env(env)?;
    resolve_restore_point_directory(&backup_dir, restore_point_id)
}

fn validate_restore_point_id(restore_point_id: &str) -> Result<()> {
    if restore_point_id.is_empty()
        || restore_point_id.contains("..")
        || restore_point_id.contains('/')
        || restore_point_id.contains('\\')
    {
        return Err(SlateError::BackupFailed(format!(
            "Invalid restore point id: {}",
            restore_point_id
        )));
    }

    Ok(())
}

fn resolve_restore_point_directory(backup_dir: &Path, restore_point_id: &str) -> Result<PathBuf> {
    validate_restore_point_id(restore_point_id)?;

    let canonical_backup_dir = fs::canonicalize(backup_dir).map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to canonicalize backup directory {}: {}",
            backup_dir.display(),
            e
        ))
    })?;

    let restore_point_dir = backup_dir.join(restore_point_id);
    if restore_point_dir.exists() {
        let canonical_restore_point = fs::canonicalize(&restore_point_dir).map_err(|e| {
            SlateError::BackupFailed(format!(
                "Failed to canonicalize restore point {}: {}",
                restore_point_dir.display(),
                e
            ))
        })?;

        if !canonical_restore_point.starts_with(&canonical_backup_dir) {
            return Err(SlateError::BackupFailed(format!(
                "Restore point escapes backup directory: {}",
                restore_point_id
            )));
        }

        Ok(canonical_restore_point)
    } else {
        Ok(restore_point_dir)
    }
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

fn write_manifest_raw(manifest_path: &Path, manifest: &RestoreManifest) -> Result<()> {
    let content = toml::to_string_pretty(manifest).map_err(|e| {
        SlateError::BackupFailed(format!("Failed to serialize manifest.toml: {}", e))
    })?;

    let mut file = AtomicWriteFile::open(manifest_path).map_err(|e| {
        SlateError::BackupFailed(format!("Failed to open manifest.toml for writing: {}", e))
    })?;

    file.write_all(content.as_bytes())
        .map_err(|e| SlateError::BackupFailed(format!("Failed to write manifest.toml: {}", e)))?;

    file.commit()
        .map_err(|e| SlateError::BackupFailed(format!("Failed to commit manifest.toml: {}", e)))
}

fn read_manifest_raw(manifest_path: &Path) -> Result<RestoreManifest> {
    let content = fs::read_to_string(manifest_path)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to read manifest.toml: {}", e)))?;

    toml::from_str(&content)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to parse manifest.toml: {}", e)))
}

fn manifest_to_restore_point(manifest: RestoreManifest) -> Result<RestorePoint> {
    Ok(RestorePoint {
        id: manifest.metadata.id,
        theme_name: manifest.metadata.theme_name,
        created_at: timestamp_from_string(&manifest.metadata.created_at)?,
        entries: manifest.entries,
        is_baseline: manifest.metadata.is_baseline,
    })
}

fn record_absent_entry(session: &BackupSession, entry: RestoreEntry) -> Result<RestoreEntry> {
    append_manifest_entry(session, &entry)?;
    Ok(entry)
}

fn append_manifest_entry(session: &BackupSession, entry: &RestoreEntry) -> Result<()> {
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

fn validate_restore_point_data(restore_point: &RestorePoint) -> Result<()> {
    if restore_point.entries.is_empty() {
        if restore_point.is_baseline {
            return Ok(());
        }
        return Err(SlateError::BackupFailed(format!(
            "No entries in restore point: {}",
            restore_point.id
        )));
    }

    let mut has_delta = false;
    let mut has_delta_gitconfig = false;

    for entry in &restore_point.entries {
        if entry.original_path.as_os_str().is_empty() {
            return Err(SlateError::BackupFailed(format!(
                "Restore entry '{}' is missing original_path metadata",
                entry.tool_key
            )));
        }

        if entry.original_state == OriginalFileState::Present {
            let Some(path) = entry.backup_path.as_ref() else {
                return Err(SlateError::BackupFailed(format!(
                    "Restore entry '{}' is missing backup_path metadata",
                    entry.tool_key
                )));
            };

            if !path.exists() {
                return Err(SlateError::BackupFailed(format!(
                    "Backup file not found: {}",
                    path.display()
                )));
            }

            let metadata = fs::metadata(path).map_err(|e| {
                SlateError::BackupFailed(format!(
                    "Cannot read backup file {}: {}",
                    path.display(),
                    e
                ))
            })?;

            if metadata.len() == 0 {
                return Err(SlateError::BackupFailed(format!(
                    "Backup file is empty: {}",
                    path.display()
                )));
            }
        }

        if entry.tool_key == "delta" {
            has_delta = true;
        } else if entry.tool_key == "delta-gitconfig" {
            has_delta_gitconfig = true;
        }
    }

    if has_delta != has_delta_gitconfig {
        return Err(SlateError::BackupFailed("Delta restore point is incomplete. Both delta and delta-gitconfig backups must exist together.".to_string(),));
    }

    Ok(())
}

/// Begin a new restore point session for a set operation.
/// Creates the restore_point_id directory and returns a BackupSession
/// that groups all backups from this set operation.
pub fn begin_restore_point(theme_name: &str) -> Result<BackupSession> {
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
                        is_baseline: false,
                    },
                    entries: Vec::new(),
                };
                write_manifest_raw(&manifest_path(&session.restore_point_dir), &manifest)?;
                return Ok(session);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(SlateError::BackupFailed(format!(
                    "Failed to create restore point directory: {}",
                    e
                )));
            }
        }
    }

    Err(SlateError::BackupFailed(
        "Failed to allocate a unique restore point ID".to_string(),
    ))
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
) -> Result<RestoreEntry> {
    // Read original config file
    let content = fs::read_to_string(config_path)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to read config: {}", e)))?;

    // Generate backup filename in the restore point directory
    // Format: {tool_key}.backup (simple, no timestamp needed)
    let backup_filename = format!("{}.backup", tool_key);
    let backup_path = session.restore_point_dir.join(&backup_filename);

    // Write backup file atomically
    let mut file = AtomicWriteFile::open(&backup_path)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to create backup file: {}", e)))?;

    file.write_all(content.as_bytes())
        .map_err(|e| SlateError::BackupFailed(format!("Failed to write backup: {}", e)))?;

    file.commit()
        .map_err(|e| SlateError::BackupFailed(format!("Failed to commit backup: {}", e)))?;

    let restore_entry = RestoreEntry {
        tool_key: tool_key.to_string(),
        display_tool: display_tool.to_string(),
        original_path: config_path.to_path_buf(),
        backup_path: Some(backup_path),
        original_state: OriginalFileState::Present,
    };

    append_manifest_entry(session, &restore_entry)?;

    Ok(restore_entry)
}

/// Create a backup of a config file before modification
/// Returns BackupInfo with both paths and timestamp
pub fn create_backup(tool: &str, theme_name: &str, config_path: &Path) -> Result<BackupInfo> {
    // Get backup directory
    let backup_dir = backup_directory()?;

    // Read original config file
    let content = fs::read_to_string(config_path)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to read config: {}", e)))?;

    // Generate timestamp in ISO8601 format with Z suffix, colons escaped
    let now = SystemTime::now();
    let timestamp = format_iso8601_timestamp(now);

    // Generate backup filename: {tool}--{theme_name}--{timestamp}.backup
    // Replace spaces and colons with dashes for filesystem safety
    let safe_theme = theme_name.replace([' ', ':'], "-");
    let backup_filename = format!("{}--{}--{}.backup", tool, safe_theme, timestamp);
    let backup_path = backup_dir.join(&backup_filename);

    // Write backup file atomically
    let mut file = AtomicWriteFile::open(&backup_path)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to create backup file: {}", e)))?;

    file.write_all(content.as_bytes())
        .map_err(|e| SlateError::BackupFailed(format!("Failed to write backup: {}", e)))?;

    file.commit()
        .map_err(|e| SlateError::BackupFailed(format!("Failed to commit backup: {}", e)))?;

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
pub fn parse_backup_filename(filename: &str) -> Result<(String, String, String)> {
    // Strip .backup suffix
    if !filename.ends_with(".backup") {
        return Err(SlateError::BackupFailed(format!(
            "Invalid backup filename format: {}",
            filename
        )));
    }

    let without_suffix = &filename[..filename.len() - 7]; // Remove ".backup"

    // Split by "--"
    let parts: Vec<&str> = without_suffix.split("--").collect();
    if parts.len() != 3 {
        return Err(SlateError::BackupFailed(format!(
            "Invalid backup filename format (expected 3 parts): {}",
            filename
        )));
    }

    let tool = parts[0].to_string();
    // Reverse theme name space replacement: "Catppuccin-Mocha" → "Catppuccin Mocha"
    let theme = parts[1].replace("-", " ");
    let timestamp = parts[2].to_string();

    Ok((tool, theme, timestamp))
}

/// Parse ISO8601-like timestamp string back to SystemTime
/// Format: "2026-04-09T10-00-00Z" (YYYY-MM-DDTHH-MM-SSZ with dashes instead of colons)
pub fn timestamp_from_string(ts_str: &str) -> Result<SystemTime> {
    // Expected format: 2026-04-09T10-00-00Z (20 chars)
    if ts_str.len() != 20 || !ts_str.ends_with('Z') {
        return Err(SlateError::BackupFailed(format!(
            "Invalid timestamp format: {}",
            ts_str
        )));
    }

    // Check format pattern: YYYY-MM-DDTHH-MM-SSZ
    let chars: Vec<char> = ts_str.chars().collect();
    if chars[4] != '-'
        || chars[7] != '-'
        || chars[10] != 'T'
        || chars[13] != '-'
        || chars[16] != '-'
    {
        return Err(SlateError::BackupFailed(format!(
            "Invalid timestamp format: {}",
            ts_str
        )));
    }

    // Parse: YYYY-MM-DDTHH-MM-SS
    let year: u64 = ts_str[0..4]
        .parse()
        .map_err(|_| SlateError::BackupFailed(format!("Invalid year in timestamp: {}", ts_str)))?;
    let month: u64 = ts_str[5..7]
        .parse()
        .map_err(|_| SlateError::BackupFailed(format!("Invalid month in timestamp: {}", ts_str)))?;
    let day: u64 = ts_str[8..10]
        .parse()
        .map_err(|_| SlateError::BackupFailed(format!("Invalid day in timestamp: {}", ts_str)))?;
    let hour: u64 = ts_str[11..13]
        .parse()
        .map_err(|_| SlateError::BackupFailed(format!("Invalid hour in timestamp: {}", ts_str)))?;
    let minute: u64 = ts_str[14..16].parse().map_err(|_| {
        SlateError::BackupFailed(format!("Invalid minute in timestamp: {}", ts_str))
    })?;
    let second: u64 = ts_str[17..19].parse().map_err(|_| {
        SlateError::BackupFailed(format!("Invalid second in timestamp: {}", ts_str))
    })?;

    // Convert to Unix timestamp
    let days_since_epoch = days_from_unix_epoch(year, month, day).ok_or_else(|| {
        SlateError::BackupFailed(format!("Invalid date in timestamp: {}", ts_str))
    })?;

    let total_seconds = days_since_epoch * 86400 + hour * 3600 + minute * 60 + second;

    Ok(UNIX_EPOCH + Duration::from_secs(total_seconds))
}

/// Calculate days since Unix epoch (1970-01-01) for a given date
fn days_from_unix_epoch(year: u64, month: u64, day: u64) -> Option<u64> {
    if !(1..=12).contains(&month) || day < 1 {
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
        days += days_in_month[m as usize - 1];
    }

    // Add remaining days
    days += day - 1;

    Some(days)
}

/// List all restore points in the backup directory
/// Scans ~/.cache/slate/backups/ for restore_point_id directories with manifest.toml
/// Returns Vec<RestorePoint> sorted by creation time descending (newest first)
pub fn list_restore_points() -> Result<Vec<RestorePoint>> {
    let env = SlateEnv::from_process().map_err(|_| {
        SlateError::Internal("Cannot initialize SlateEnv to list restore points".to_string())
    })?;
    list_restore_points_with_env(&env)
}

/// List all restore points using an injected SlateEnv.
pub fn list_restore_points_with_env(env: &SlateEnv) -> Result<Vec<RestorePoint>> {
    let backup_dir = backup_directory_with_env(env)?;

    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&backup_dir)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to read backup directory: {}", e)))?;

    let mut restore_points = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| {
            SlateError::BackupFailed(format!("Failed to read backup directory entry: {}", e))
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
fn read_manifest(manifest_path: &Path) -> Result<RestorePoint> {
    let restore_point = manifest_to_restore_point(read_manifest_raw(manifest_path)?)?;
    if restore_point.entries.is_empty() && !restore_point.is_baseline {
        return Err(SlateError::BackupFailed(
            "manifest.toml has empty entries array".to_string(),
        ));
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
pub fn validate_restore_point(restore_point_id: &str) -> Result<()> {
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

struct SnapshotTarget {
    tool_key: &'static str,
    display_tool: &'static str,
    path: PathBuf,
}

fn baseline_snapshot_targets(env: &SlateEnv) -> Vec<SnapshotTarget> {
    let ghostty_path = crate::adapter::GhosttyAdapter
        .integration_config_path_with_env(env)
        .unwrap_or_else(|_| env.xdg_config_home().join("ghostty/config.ghostty"));
    let alacritty_path = env.xdg_config_home().join("alacritty/alacritty.toml");
    let starship_path = std::env::var("STARSHIP_CONFIG")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| env.xdg_config_home().join("starship.toml"));

    vec![
        SnapshotTarget {
            tool_key: "zshrc",
            display_tool: "Zsh",
            path: env.zshrc_path(),
        },
        SnapshotTarget {
            tool_key: "gitconfig",
            display_tool: "Git",
            path: env.home().join(".gitconfig"),
        },
        SnapshotTarget {
            tool_key: "tmux",
            display_tool: "tmux",
            path: env.home().join(".tmux.conf"),
        },
        SnapshotTarget {
            tool_key: "ghostty",
            display_tool: "Ghostty",
            path: ghostty_path,
        },
        SnapshotTarget {
            tool_key: "alacritty",
            display_tool: "Alacritty",
            path: alacritty_path,
        },
        SnapshotTarget {
            tool_key: "starship",
            display_tool: "Starship",
            path: starship_path,
        },
        SnapshotTarget {
            tool_key: "slate-current",
            display_tool: "Slate current theme",
            path: env.managed_file("current"),
        },
        SnapshotTarget {
            tool_key: "slate-current-font",
            display_tool: "Slate current font",
            path: env.managed_file("current-font"),
        },
        SnapshotTarget {
            tool_key: "slate-current-opacity",
            display_tool: "Slate current opacity",
            path: env.managed_file("current-opacity"),
        },
        SnapshotTarget {
            tool_key: "slate-config",
            display_tool: "Slate config",
            path: env.managed_file("config.toml"),
        },
        SnapshotTarget {
            tool_key: "slate-auto",
            display_tool: "Slate auto theme",
            path: env.managed_file("auto.toml"),
        },
        SnapshotTarget {
            tool_key: "slate-fastfetch",
            display_tool: "Slate fastfetch autorun",
            path: env.managed_file("autorun-fastfetch"),
        },
    ]
}

/// Create a baseline restore point before any slate mutations
/// Special variant of begin_restore_point that marks the restore point as baseline
/// This should be called BEFORE first setup to capture pre-slate state
pub fn begin_restore_point_baseline(home: &Path) -> Result<RestorePoint> {
    let env = SlateEnv::with_home(home.to_path_buf());
    begin_restore_point_baseline_with_env(&env)
}

pub fn begin_restore_point_baseline_with_env(env: &SlateEnv) -> Result<RestorePoint> {
    let created_at = SystemTime::now();
    let backup_dir = backup_directory_with_env(env)?;

    for _ in 0..32 {
        let restore_point_id = generate_restore_point_id(created_at);
        let restore_point_dir = resolve_restore_point_directory(&backup_dir, &restore_point_id)?;

        match fs::create_dir(&restore_point_dir) {
            Ok(()) => {
                let session = BackupSession {
                    restore_point_id,
                    theme_name: "baseline-pre-slate".to_string(),
                    restore_point_dir,
                };
                let manifest = RestoreManifest {
                    metadata: RestoreManifestMetadata {
                        id: session.restore_point_id.clone(),
                        theme_name: session.theme_name.clone(),
                        created_at: format_iso8601_timestamp(created_at),
                        is_baseline: true,
                    },
                    entries: Vec::new(),
                };
                write_manifest_raw(&manifest_path(&session.restore_point_dir), &manifest)?;

                for target in baseline_snapshot_targets(env) {
                    if target.path.exists() {
                        let _ = create_backup_with_session(
                            target.tool_key,
                            target.display_tool,
                            &session,
                            &target.path,
                        )?;
                    } else {
                        let absent_entry = RestoreEntry {
                            tool_key: target.tool_key.to_string(),
                            display_tool: target.display_tool.to_string(),
                            original_path: target.path,
                            backup_path: None,
                            original_state: OriginalFileState::Absent,
                        };
                        let _ = record_absent_entry(&session, absent_entry)?;
                    }
                }

                return get_restore_point_with_env(env, &session.restore_point_id);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(SlateError::BackupFailed(format!(
                    "Failed to create baseline restore point directory: {}",
                    e
                )));
            }
        }
    }

    Err(SlateError::BackupFailed(
        "Failed to allocate a unique baseline restore point ID".to_string(),
    ))
}

/// Check if a restore point is the baseline restore point
/// Used by reset to protect baseline from accidental overwrite
pub fn is_baseline_restore_point(restore_point: &RestorePoint) -> bool {
    restore_point.is_baseline
}

/// Get a specific restore point by ID
pub fn get_restore_point(restore_point_id: &str) -> Result<RestorePoint> {
    let env = SlateEnv::from_process().map_err(|_| {
        SlateError::Internal("Cannot initialize SlateEnv to load restore point".to_string())
    })?;
    get_restore_point_with_env(&env, restore_point_id)
}

/// Get a specific restore point by ID using an injected SlateEnv.
pub fn get_restore_point_with_env(env: &SlateEnv, restore_point_id: &str) -> Result<RestorePoint> {
    let restore_point_dir = restore_point_directory_with_env(env, restore_point_id)?;
    let manifest_path = manifest_path(&restore_point_dir);

    if !manifest_path.exists() {
        return Err(SlateError::BackupFailed(format!(
            "Restore point not found: {}",
            restore_point_id
        )));
    }

    let restore_point = read_manifest(&manifest_path)?;
    if restore_point.id != restore_point_id {
        return Err(SlateError::BackupFailed(format!(
            "Restore point metadata mismatch: expected {}, found {}",
            restore_point_id, restore_point.id
        )));
    }

    Ok(restore_point)
}

/// Restore a specific restore point by writing backed-up files back to their original paths.
/// If the original file was absent when snapshotted, remove any slate-created file instead.
fn restore_entry(entry: &RestoreEntry, content: Option<&str>) -> Result<()> {
    if entry.original_state == OriginalFileState::Absent {
        match fs::remove_file(&entry.original_path) {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => {
                return Err(SlateError::BackupFailed(format!(
                    "Failed to remove restored file {}: {}",
                    entry.original_path.display(),
                    err
                )))
            }
        }
    }

    let original_path = &entry.original_path;
    let content = content.unwrap_or_default();

    // Write content atomically
    let mut file = AtomicWriteFile::open(original_path).map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to open config for writing {}: {}",
            original_path.display(),
            e
        ))
    })?;

    file.write_all(content.as_bytes()).map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to write restored content to {}: {}",
            original_path.display(),
            e
        ))
    })?;

    file.commit().map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to commit restored file {}: {}",
            original_path.display(),
            e
        ))
    })
}

/// Delete a specific restore point by ID (removes entire restore_point_id directory)
pub fn delete_restore_point(restore_point_id: &str) -> Result<usize> {
    let restore_point_dir = restore_point_directory(restore_point_id)?;

    if !restore_point_dir.exists() {
        return Err(SlateError::BackupFailed(format!(
            "Restore point directory not found: {}",
            restore_point_id
        )));
    }

    // Count files before deletion
    let entries = fs::read_dir(&restore_point_dir).map_err(|e| {
        SlateError::BackupFailed(format!("Failed to read restore point directory: {}", e))
    })?;

    let file_count: usize = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .count();

    // Remove entire directory
    fs::remove_dir_all(&restore_point_dir).map_err(|e| {
        SlateError::BackupFailed(format!(
            "Failed to delete restore point {}: {}",
            restore_point_id, e
        ))
    })?;

    Ok(file_count)
}

/// Delete all restore points (removes all restore_point_id directories)
pub fn clear_all_restore_points() -> Result<usize> {
    let backup_dir = backup_directory()?;
    if !backup_dir.exists() {
        return Ok(0);
    }

    let entries = fs::read_dir(&backup_dir)
        .map_err(|e| SlateError::BackupFailed(format!("Failed to read backup directory: {}", e)))?;

    let mut deleted_items = 0;
    for entry in entries {
        let entry = entry.map_err(|e| {
            SlateError::BackupFailed(format!("Failed to read backup directory entry: {}", e))
        })?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| {
                SlateError::BackupFailed(format!(
                    "Failed to delete backup directory {}: {}",
                    path.display(),
                    e
                ))
            })?;
            deleted_items += 1;
        } else if path.is_file() {
            fs::remove_file(&path).map_err(|e| {
                SlateError::BackupFailed(format!(
                    "Failed to delete backup file {}: {}",
                    path.display(),
                    e
                ))
            })?;
            deleted_items += 1;
        }
    }

    Ok(deleted_items)
}

/// Result of a single file restoration attempt
#[derive(Debug, Clone)]
pub struct RestoreFileResult {
    pub tool_key: String,
    pub display_tool: String,
    pub original_path: PathBuf,
    pub success: bool,
    pub error: Option<String>,
}

/// Aggregate receipt for a complete restore operation
/// Records successes, skips, and failures without pretending atomicity
#[derive(Debug, Clone)]
pub struct RestoreReceipt {
    pub restore_point_id: String,
    pub theme_name: String,
    pub results: Vec<RestoreFileResult>,
}

impl RestoreReceipt {
    /// Count successful restorations
    pub fn success_count(&self) -> usize {
        self.results.iter().filter(|r| r.success).count()
    }

    /// Count failed restorations
    pub fn failure_count(&self) -> usize {
        self.results.iter().filter(|r| !r.success).count()
    }

    /// Check if entire restore operation was successful
    pub fn is_fully_successful(&self) -> bool {
        self.failure_count() == 0 && !self.results.is_empty()
    }

    /// Get failed results for error reporting
    pub fn failed_results(&self) -> Vec<&RestoreFileResult> {
        self.results.iter().filter(|r| !r.success).collect()
    }
}

/// Create a pre-restore snapshot before any restore point is applied
/// This captures the current user-facing files so users can undo if restore goes wrong
pub fn create_pre_restore_snapshot(current_restore_point_id: &str) -> Result<RestorePoint> {
    let env = SlateEnv::from_process().map_err(|_| {
        SlateError::Internal("Cannot initialize SlateEnv to create pre-restore snapshot".to_string())
    })?;
    create_pre_restore_snapshot_with_env(&env, current_restore_point_id)
}

pub fn create_pre_restore_snapshot_with_env(
    env: &SlateEnv,
    current_restore_point_id: &str,
) -> Result<RestorePoint> {
    let created_at = SystemTime::now();
    let backup_dir = backup_directory_with_env(env)?;

    for _ in 0..32 {
        let restore_point_id = generate_restore_point_id(created_at);
        let restore_point_dir = resolve_restore_point_directory(&backup_dir, &restore_point_id)?;

        match fs::create_dir(&restore_point_dir) {
            Ok(()) => {
                let session = BackupSession {
                    restore_point_id,
                    theme_name: format!("pre-restore-snapshot-for-{}", current_restore_point_id),
                    restore_point_dir,
                };
                let manifest = RestoreManifest {
                    metadata: RestoreManifestMetadata {
                        id: session.restore_point_id.clone(),
                        theme_name: session.theme_name.clone(),
                        created_at: format_iso8601_timestamp(created_at),
                        is_baseline: false,
                    },
                    entries: Vec::new(),
                };
                write_manifest_raw(&manifest_path(&session.restore_point_dir), &manifest)?;

                for target in baseline_snapshot_targets(env) {
                    if target.path.exists() {
                        let _ = create_backup_with_session(
                            target.tool_key,
                            target.display_tool,
                            &session,
                            &target.path,
                        )?;
                    } else {
                        let absent_entry = RestoreEntry {
                            tool_key: target.tool_key.to_string(),
                            display_tool: target.display_tool.to_string(),
                            original_path: target.path,
                            backup_path: None,
                            original_state: OriginalFileState::Absent,
                        };
                        let _ = record_absent_entry(&session, absent_entry)?;
                    }
                }

                return get_restore_point_with_env(env, &session.restore_point_id);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(SlateError::BackupFailed(format!(
                    "Failed to create pre-restore snapshot directory: {}",
                    e
                )));
            }
        }
    }

    Err(SlateError::BackupFailed(
        "Failed to allocate a unique pre-restore snapshot ID".to_string(),
    ))
}

/// Execute a restore operation for a specific restore point
/// Loads the restore point, validates it, restores entries one by one,
/// and returns an aggregate receipt with per-file success/failure tracking
pub fn execute_restore(restore_point_id: &str) -> Result<RestoreReceipt> {
    // Validate restore point exists and is readable
    let restore_point = get_restore_point(restore_point_id)?;

    // Prevent restoring to baseline (safety gate)
    if restore_point.is_baseline {
        return Err(SlateError::RestoreFailed(
            "Cannot restore to baseline restore point. This is a protected snapshot.".to_string(),
        ));
    }

    // Validate restore point data before starting restoration
    validate_restore_point_data(&restore_point)?;

    // Create pre-restore snapshot for user safety
    let _pre_restore = create_pre_restore_snapshot(restore_point_id)?;

    // Restore entries one by one, continuing on individual failures
    let mut results = Vec::new();

    for entry in &restore_point.entries {
        let result = restore_single_entry(entry);
        results.push(result);
    }

    Ok(RestoreReceipt {
        restore_point_id: restore_point.id.clone(),
        theme_name: restore_point.theme_name.clone(),
        results,
    })
}

/// Restore a single entry with error handling for reporting
fn restore_single_entry(entry: &RestoreEntry) -> RestoreFileResult {
    // Read backup content if it exists
    let backup_content = if entry.original_state == OriginalFileState::Present {
        match entry.backup_path.as_ref() {
            Some(path) => match fs::read_to_string(path) {
                Ok(content) => Some(content),
                Err(e) => {
                    return RestoreFileResult {
                        tool_key: entry.tool_key.clone(),
                        display_tool: entry.display_tool.clone(),
                        original_path: entry.original_path.clone(),
                        success: false,
                        error: Some(format!("Failed to read backup file: {}", e)),
                    };
                }
            },
            None => {
                return RestoreFileResult {
                    tool_key: entry.tool_key.clone(),
                    display_tool: entry.display_tool.clone(),
                    original_path: entry.original_path.clone(),
                    success: false,
                    error: Some("Backup path not found in manifest".to_string()),
                };
            }
        }
    } else {
        None
    };

    // Attempt restore
    match restore_entry(entry, backup_content.as_deref()) {
        Ok(()) => RestoreFileResult {
            tool_key: entry.tool_key.clone(),
            display_tool: entry.display_tool.clone(),
            original_path: entry.original_path.clone(),
            success: true,
            error: None,
        },
        Err(e) => RestoreFileResult {
            tool_key: entry.tool_key.clone(),
            display_tool: entry.display_tool.clone(),
            original_path: entry.original_path.clone(),
            success: false,
            error: Some(e.to_string()),
        },
    }
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
            backup_path: Some(PathBuf::from(
                "~/.cache/slate/backups/2026-04-09T10-00-00Z/ghostty.backup",
            )),
            original_state: OriginalFileState::Present,
        };

        let json = serde_json::to_string(&entry);
        assert!(json.is_ok());

        let parsed: RestoreEntry = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(parsed.tool_key, "ghostty");
        assert_eq!(parsed.original_state, OriginalFileState::Present);
    }
}
