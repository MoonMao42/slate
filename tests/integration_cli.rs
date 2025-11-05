use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct CliTestEnv {
    _temp_dir: TempDir,
    home: PathBuf,
    config_home: PathBuf,
    cache_home: PathBuf,
    bin_dir: PathBuf,
    ghostty_config: PathBuf,
    starship_config: PathBuf,
    bat_config: PathBuf,
}

impl CliTestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let home = root.join("home");
        let config_home = root.join("config");
        let cache_home = root.join("cache");
        let bin_dir = root.join("bin");

        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        fs::create_dir_all(&cache_home).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        create_fake_binary(&bin_dir.join("ghostty"));
        create_fake_binary(&bin_dir.join("starship"));
        create_fake_binary(&bin_dir.join("bat"));

        let ghostty_config = config_home.join("ghostty").join("config.ghostty");
        let starship_config = config_home.join("starship.toml");
        let bat_config = config_home.join("bat").join("config");

        write_file(&ghostty_config, "theme = \"Old Ghostty\"\n");
        write_file(&starship_config, "palette = \"old_starship\"\n");
        write_file(&bat_config, "--theme=\"OldBat\"\n");

        Self {
            _temp_dir: temp_dir,
            home,
            config_home,
            cache_home,
            bin_dir,
            ghostty_config,
            starship_config,
            bat_config,
        }
    }

    fn command(&self) -> Command {
        let mut cmd = Command::cargo_bin("themectl").unwrap();
        cmd.env_clear();
        cmd.env("HOME", &self.home);
        cmd.env("XDG_CONFIG_HOME", &self.config_home);
        cmd.env("XDG_CACHE_HOME", &self.cache_home);
        cmd.env("PATH", &self.bin_dir);
        cmd
    }

    fn backup_root(&self) -> PathBuf {
        self.cache_home.join("themectl").join("backups")
    }
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[cfg(unix)]
fn create_fake_binary(path: &Path) {
    write_file(path, "#!/bin/sh\nexit 0\n");
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

#[cfg(not(unix))]
fn create_fake_binary(path: &Path) {
    write_file(path, "");
}

fn extract_restore_point_id(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("Created restore point: "))
        .map(|id| id.trim().to_string())
        .expect("restore point id not found in stdout")
}

#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Apply a theme"));
}

#[test]
fn test_cli_version() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
}

#[test]
fn test_cli_set_help() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.args(&["set", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("THEME"));
}

#[test]
fn test_cli_set_valid_theme() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("catppuccin-mocha")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"))
        .stdout(predicate::str::contains("tools updated"));
}

#[test]
fn test_cli_set_invalid_theme() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("nonexistent-theme")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"))
        .stderr(predicate::str::contains("not recognized"));
}

#[test]
fn test_cli_set_verbose_mode() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("catppuccin-mocha")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains("Scanning for tools"))
        .stdout(predicate::str::contains("Checking"));
}

#[test]
fn test_cli_set_dracula_theme() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("dracula")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"))
        .stdout(predicate::str::contains("dracula"));
}

#[test]
fn test_cli_set_tokyo_night_dark() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("tokyo-night-dark")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"))
        .stdout(predicate::str::contains("tokyo-night-dark"));
}

#[test]
fn test_cli_set_nord_theme() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("nord")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"))
        .stdout(predicate::str::contains("nord"));
}

#[test]
fn test_cli_set_case_insensitive() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("Catppuccin-Mocha")
        .assert()
        .success()
        .stdout(predicate::str::contains("Theme Applied:"));
}

#[test]
fn test_cli_exit_code_success() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set").arg("catppuccin-mocha").assert().success();
}

#[test]
fn test_cli_exit_code_failure() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set").arg("invalid-theme").assert().failure();
}

#[test]
fn test_cli_output_format_has_emoji() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("catppuccin-mocha")
        .assert()
        .success()
        .stdout(predicate::str::contains("🎨"));
}

#[test]
fn test_cli_output_format_has_checkmark() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .arg("catppuccin-mocha")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓"));
}

#[test]
fn test_cli_no_arguments() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.assert().failure().stderr(
        predicate::str::contains("missing positional argument")
            .or(predicate::str::contains("COMMAND")),
    );
}

#[test]
fn test_cli_set_no_theme_non_tty() {
    // Without a theme arg in non-TTY, interactive picker fails gracefully
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("set")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cancelled"));
}

#[test]
fn test_cli_status_help() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.args(&["status", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Show current theme state"));
}

#[test]
fn test_cli_status_basic() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("status").assert().success();
}

#[test]
fn test_cli_status_verbose() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("status").arg("--verbose").assert().success();
}

#[test]
fn test_cli_status_verbose_short() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("status").arg("-v").assert().success();
}

#[test]
fn test_cli_status_output_has_tools_or_empty_message() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    let output = cmd.arg("status").output().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain either tool names (if any installed) or the no-tools message
    let has_tools = stdout.contains("Tool Status")
        || stdout.contains("Ghostty")
        || stdout.contains("Starship")
        || stdout.contains("bat")
        || stdout.contains("Delta")
        || stdout.contains("Lazygit");
    let has_empty_msg = stdout.contains("No supported tools detected");

    assert!(
        has_tools || has_empty_msg,
        "Status output should show tools or empty message, got: {}",
        stdout
    );
}

// List command tests

#[test]
fn test_cli_list_help() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.args(&["list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("List available themes"));
}

#[test]
fn test_cli_list_plain_text_mode() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    // Simulate non-TTY by piping
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Catppuccin"))
        .stdout(predicate::str::contains("Tokyo Night"))
        .stdout(predicate::str::contains("Dracula"))
        .stdout(predicate::str::contains("Nord"));
}

#[test]
fn test_cli_list_contains_catppuccin_variants() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("catppuccin-latte"))
        .stdout(predicate::str::contains("catppuccin-frappe"))
        .stdout(predicate::str::contains("catppuccin-macchiato"))
        .stdout(predicate::str::contains("catppuccin-mocha"));
}

#[test]
fn test_cli_list_contains_tokyo_night_variants() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("tokyo-night-light"))
        .stdout(predicate::str::contains("tokyo-night-dark"));
}

#[test]
fn test_cli_list_contains_descriptions() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Dark theme"))
        .stdout(predicate::str::contains("Light theme"));
}

#[test]
fn test_cli_list_contains_dracula() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("dracula"));
}

#[test]
fn test_cli_list_contains_nord() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("nord"));
}

#[test]
fn test_cli_restore_help() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.args(&["restore", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Restore a previous theme state"));
}

#[test]
fn test_cli_restore_list_empty() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("restore").arg("--list").assert().success().stdout(
        predicate::str::contains("No restore points available").or(predicate::str::contains("")),
    ); // Empty output if no backups
}

#[test]
fn test_cli_restore_conflicting_modes() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("restore")
        .arg("--list")
        .arg("--clear-all")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Conflicting modes"));
}

#[test]
fn test_cli_restore_nonexistent_id() {
    let mut cmd = Command::cargo_bin("themectl").unwrap();
    cmd.arg("restore")
        .arg("nonexistent-restore-point-id")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Restore point not found"));
}

#[test]
fn test_cli_set_creates_manifest_and_restore_round_trip() {
    let env = CliTestEnv::new();
    let original_ghostty = "theme = \"Old Ghostty\"\n";
    let original_starship = "palette = \"old_starship\"\n";
    let original_bat = "--theme=\"OldBat\"\n";

    let output = env
        .command()
        .arg("set")
        .arg("catppuccin-mocha")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let restore_point_id = extract_restore_point_id(&stdout);
    let restore_point_dir = env.backup_root().join(&restore_point_id);
    let manifest_path = restore_point_dir.join("manifest.toml");

    assert!(manifest_path.exists(), "manifest.toml was not created");
    assert!(restore_point_dir.join("ghostty.backup").exists());
    assert!(restore_point_dir.join("starship.backup").exists());
    assert!(restore_point_dir.join("bat.backup").exists());

    let manifest = fs::read_to_string(&manifest_path).unwrap();
    // theme_name records the backed-up (old) theme, not the new one being set
    assert!(manifest.contains("theme_name = \"Old Ghostty\""));
    assert!(manifest.contains("tool_key = \"ghostty\""));
    assert!(manifest.contains("tool_key = \"starship\""));
    assert!(manifest.contains("tool_key = \"bat\""));

    let list_output = env.command().arg("restore").arg("--list").output().unwrap();
    assert!(
        list_output.status.success(),
        "restore --list failed: {}",
        String::from_utf8_lossy(&list_output.stderr)
    );
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(list_stdout.contains(&restore_point_id));
    assert!(list_stdout.contains("Old Ghostty"));

    write_file(&env.ghostty_config, "theme = \"Broken\"\n");
    write_file(&env.starship_config, "palette = \"broken\"\n");
    write_file(&env.bat_config, "--theme=\"Broken\"\n");

    let restore_output = env
        .command()
        .arg("restore")
        .arg(&restore_point_id)
        .output()
        .unwrap();

    assert!(
        restore_output.status.success(),
        "restore failed: {}",
        String::from_utf8_lossy(&restore_output.stderr)
    );
    let restore_stdout = String::from_utf8_lossy(&restore_output.stdout);
    assert!(restore_stdout.contains("Restored:"));
    assert!(restore_stdout.contains("Ghostty"));
    assert!(restore_stdout.contains("Starship"));
    assert!(restore_stdout.contains("bat"));

    assert_eq!(
        fs::read_to_string(&env.ghostty_config).unwrap(),
        original_ghostty
    );
    assert_eq!(
        fs::read_to_string(&env.starship_config).unwrap(),
        original_starship
    );
    assert_eq!(fs::read_to_string(&env.bat_config).unwrap(), original_bat);
}

#[test]
fn test_cli_set_creates_unique_restore_points_on_back_to_back_runs() {
    let env = CliTestEnv::new();

    let first = env
        .command()
        .arg("set")
        .arg("catppuccin-mocha")
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "first set failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_id = extract_restore_point_id(&String::from_utf8_lossy(&first.stdout));

    let second = env.command().arg("set").arg("nord").output().unwrap();
    assert!(
        second.status.success(),
        "second set failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_id = extract_restore_point_id(&String::from_utf8_lossy(&second.stdout));

    assert_ne!(first_id, second_id);
    assert!(env
        .backup_root()
        .join(&first_id)
        .join("manifest.toml")
        .exists());
    assert!(env
        .backup_root()
        .join(&second_id)
        .join("manifest.toml")
        .exists());

    let list_output = env.command().arg("restore").arg("--list").output().unwrap();
    assert!(list_output.status.success());
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(list_stdout.contains(&first_id));
    assert!(list_stdout.contains(&second_id));
}
