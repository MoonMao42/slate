use thiserror::Error;

/// Primary error type for slate operations.
/// Use #[error(...)] for Display impl, never hand-written impl Display.
/// color-eyre adds context/colors at call sites.
#[derive(Error, Debug)]
pub enum SlateError {
    #[error("Home directory not found. Please set $HOME environment variable.")]
    MissingHomeDir,

    #[error("Config file not found at {0}. Run 'slate setup' to initialize it.")]
    ConfigNotFound(String),

    #[error("Failed to read config from {0}: {1}")]
    ConfigReadError(String, String),

    #[error("Failed to parse config from {0}: {1}")]
    ConfigParseError(String, String),

    #[error("Failed to write config to {0}: {1}")]
    ConfigWriteError(String, String),

    #[error("Tool {0} is not installed. Run 'slate setup' to configure it.")]
    ToolNotInstalled(String),

    #[error("Theme '{0}' not found. Run 'slate list' to see available themes.")]
    ThemeNotFound(String),

    #[error("Invalid theme data: {0}")]
    InvalidThemeData(String),

    #[error("Adapter for {0} is not registered.")]
    AdapterNotFound(String),

    #[error("Failed to apply theme to {0}: {1}")]
    ApplyThemeFailed(String, String),

    #[error("Failed to reload {0}: {1}")]
    ReloadFailed(String, String),

    #[error("Backup operation failed: {0}")]
    BackupFailed(String),

    #[error("Restore operation failed: {0}")]
    RestoreFailed(String),

    #[error("Launchd operation failed: {0}")]
    LaunchdError(String),

    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParseError(#[from] toml_edit::TomlError),

    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("User cancelled operation")]
    UserCancelled,

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for slate operations
pub type Result<T> = std::result::Result<T, SlateError>;

/// Install the color-eyre error handler.
/// Call this once at the start of main().
/// color_eyre::install() must be called.
pub fn install_error_handler() -> color_eyre::Result<()> {
    color_eyre::install()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SlateError::ToolNotInstalled("ghostty".to_string());
        let msg = err.to_string();
        assert!(msg.contains("ghostty"));
        assert!(msg.contains("not installed"));
    }

    #[test]
    fn test_error_with_context() {
        let err = SlateError::ConfigNotFound("/path/to/config".to_string());
        assert!(err.to_string().contains("/path/to/config"));
    }

    #[test]
    fn test_result_type() {
        let ok_result: Result<i32> = Ok(42);
        assert!(ok_result.is_ok());

        let err_result: Result<i32> = Err(SlateError::UserCancelled);
        assert!(err_result.is_err());
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let slate_err: SlateError = io_err.into();
        let msg = slate_err.to_string();
        assert!(msg.contains("IO error"));
    }
}
