use std::io;
use thiserror::Error;

/// Result type alias for themectl operations
pub type ThemeResult<T> = Result<T, ThemeError>;

/// Comprehensive error type for all failure scenarios
#[derive(Error, Debug)]
pub enum ThemeError {
    /// Tool binary not found in PATH
    #[error("Tool '{0}' not found in PATH")]
    ToolNotFound(String),

    /// Tool is installed but has no config file
    #[error("Config file not found for tool '{0}'")]
    ConfigNotFound(String),

    /// TOML parsing or format error
    #[error("Invalid TOML in {path}: {reason}")]
    InvalidToml { path: String, reason: String },

    /// Unknown theme name with available options context
    #[error("Theme '{0}' not found. Available themes: {1}")]
    ThemeNotFound(String, String),

    /// Atomic write operation failed
    #[error("Failed to write config at {path}: {reason}")]
    WriteError { path: String, reason: String },

    /// Backup creation failed
    #[error("Failed to create backup: {reason}")]
    BackupError { reason: String },

    /// Symlink canonicalization failed
    #[error("Failed to resolve symlink at {path}")]
    SymlinkError { path: String },

    /// Partial failure when applying theme to multiple tools
    #[error("Partial failure: {0} tool(s) failed to apply theme")]
    PartialFailure(usize),

    /// No supported tools were detected on the system
    #[error("No supported tools detected. Install one of: ghostty, starship, bat")]
    NoToolsDetected,

    /// Standard I/O errors with context
    #[error(transparent)]
    Io(#[from] io::Error),

    /// Catch-all for other errors
    #[error("Error: {0}")]
    Other(String),
}

impl From<toml::de::Error> for ThemeError {
    fn from(err: toml::de::Error) -> Self {
        ThemeError::InvalidToml {
            path: "config".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<toml_edit::de::Error> for ThemeError {
    fn from(err: toml_edit::de::Error) -> Self {
        ThemeError::InvalidToml {
            path: "config".to_string(),
            reason: err.to_string(),
        }
    }
}
