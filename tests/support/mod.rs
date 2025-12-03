use slate_cli::env::SlateEnv;
use tempfile::TempDir;

/// Create an isolated test environment using a temporary directory
/// Returns (TempDir, SlateEnv) — keep TempDir alive for test lifetime
pub fn create_test_env() -> (TempDir, SlateEnv) {
    let tempdir = TempDir::new().expect("Failed to create temp directory");
    let env = SlateEnv::with_home(tempdir.path().to_path_buf());
    (tempdir, env)
}

/// Helper: Verify a file exists in test environment
pub fn assert_test_file_exists(env: &SlateEnv, filename: &str) {
    let path = env.managed_file(filename);
    assert!(
        path.exists(),
        "Expected file {} to exist",
        path.display()
    );
}

/// Helper: Read test environment config file
pub fn read_test_file(env: &SlateEnv, filename: &str) -> String {
    let path = env.managed_file(filename);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Failed to read test file {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_env_creates_tempdir() {
        let (tempdir, env) = create_test_env();
        assert!(tempdir.path().exists());
        assert_eq!(env.home(), tempdir.path());
    }

    #[test]
    fn test_test_env_isolated_from_real_home() {
        let (tempdir, env) = create_test_env();
        // Verify the test env is using a tempdir, not the real home
        assert!(env.home().to_string_lossy().contains("tmp"));
        // And it's definitely not the real home directory
        assert!(!env.home().to_string_lossy().contains(std::env::var("HOME").unwrap_or_default()));
    }

    #[test]
    fn test_managed_file_path_in_test_env() {
        let (_tempdir, env) = create_test_env();
        let file = env.managed_file("test.conf");
        // Should construct path without touching filesystem
        assert!(file.ends_with("slate/test.conf"));
    }
}
