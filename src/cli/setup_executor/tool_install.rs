use crate::cli::tool_selection::BrewKind;
use crate::env::SlateEnv;
use crate::error::Result;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolInstallMethod {
    Homebrew,
    UserLocal(PathBuf),
}

impl ToolInstallMethod {
    pub(crate) fn success_message(&self, label: &str) -> String {
        match self {
            Self::Homebrew => format!("✓ {} installed", label),
            Self::UserLocal(bin_dir) => {
                format!("✓ {} installed locally at {}", label, bin_dir.display())
            }
        }
    }
}

/// Install a tool via Homebrew, with a user-local Starship fallback for shared machines.
pub(crate) fn install_tool(
    tool_id: &str,
    package: &str,
    kind: BrewKind,
    env: &SlateEnv,
) -> Result<ToolInstallMethod> {
    match install_tool_via_platform(tool_id, package, kind, env) {
        Ok(()) => Ok(ToolInstallMethod::Homebrew),
        Err(err) if tool_id == "starship" && should_try_local_starship_fallback(&err) => {
            install_starship_locally(env)?;
            Ok(ToolInstallMethod::UserLocal(env.user_local_bin()))
        }
        Err(err) => Err(err),
    }
}

fn install_tool_via_platform(
    tool_id: &str,
    package: &str,
    kind: BrewKind,
    env: &SlateEnv,
) -> Result<()> {
    match crate::platform::packages::detect_backend() {
        crate::platform::packages::PackageManagerBackend::Homebrew => {
            install_tool_via_homebrew(package, kind)
        }
        crate::platform::packages::PackageManagerBackend::Apt => {
            crate::platform::packages::install_tool_package(tool_id, package, env)
        }
        crate::platform::packages::PackageManagerBackend::Unsupported => Err(
            crate::error::SlateError::Internal(
                "No supported package manager was found. Slate currently supports Homebrew on macOS and apt on Linux.".to_string(),
            ),
        ),
    }
}

fn install_tool_via_homebrew(package: &str, kind: BrewKind) -> Result<()> {
    let brew = crate::detection::homebrew_executable().ok_or_else(|| {
        crate::error::SlateError::Internal(
            "Homebrew was not found. Install it first or add it to PATH.".to_string(),
        )
    })?;
    let mut cmd = Command::new(brew);
    crate::detection::apply_normalized_path(&mut cmd);

    match kind {
        BrewKind::Formula => {
            cmd.arg("install").arg(package);
        }
        BrewKind::Cask => {
            cmd.arg("install").arg("--cask").arg(package);
        }
    }

    let output = cmd.output().map_err(|e| {
        crate::error::SlateError::Internal(format!("Failed to execute brew: {}", e))
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(crate::error::SlateError::Internal(classify_brew_error(
            package, &stderr,
        )))
    }
}

pub(crate) fn should_try_local_starship_fallback(err: &crate::error::SlateError) -> bool {
    let message = err.to_string().to_lowercase();
    message.contains("permission denied")
        || message.contains("not writable")
        || message.contains("homebrew was not found")
}

fn install_starship_locally(env: &SlateEnv) -> Result<()> {
    use std::fs;

    const STARSHIP_INSTALL_URL: &str = "https://starship.rs/install.sh";

    let local_bin = env.user_local_bin();
    fs::create_dir_all(&local_bin)?;

    let temp_dir = super::font_install::create_writable_temp_dir(env).map_err(|e| {
        crate::error::SlateError::Internal(format!(
            "Cannot create temporary directory for Starship installer: {}",
            e
        ))
    })?;
    let installer_path = temp_dir.path().join("starship_install.sh");

    let curl = crate::detection::command_path("curl").ok_or_else(|| {
        crate::error::SlateError::Internal(
            "curl was not found. Install curl, then rerun slate setup.".to_string(),
        )
    })?;
    let mut download = Command::new(curl);
    crate::detection::apply_normalized_path(&mut download);
    let download_output = download
        .arg("-fsSL")
        .arg("--connect-timeout")
        .arg("10")
        .arg("--max-time")
        .arg("60")
        .arg(STARSHIP_INSTALL_URL)
        .output()
        .map_err(|err| {
            crate::error::SlateError::Internal(format!(
                "Failed to download Starship installer: {}",
                err
            ))
        })?;

    if !download_output.status.success() {
        let stderr = String::from_utf8_lossy(&download_output.stderr);
        let stdout = String::from_utf8_lossy(&download_output.stdout);
        return Err(crate::error::SlateError::Internal(format!(
            "Starship local fallback download failed: {}",
            first_meaningful_command_line(&stderr, &stdout)
        )));
    }

    fs::write(&installer_path, &download_output.stdout)?;

    let mut install = Command::new("/bin/sh");
    crate::detection::apply_normalized_path(&mut install);
    let install_output = install
        .arg(&installer_path)
        .arg("-y")
        .arg("-b")
        .arg(&local_bin)
        .output()
        .map_err(|err| {
            crate::error::SlateError::Internal(format!(
                "Failed to execute Starship local installer: {}",
                err
            ))
        })?;

    if !install_output.status.success() {
        let stderr = String::from_utf8_lossy(&install_output.stderr);
        let stdout = String::from_utf8_lossy(&install_output.stdout);
        return Err(crate::error::SlateError::Internal(format!(
            "Starship local fallback failed: {}",
            first_meaningful_command_line(&stderr, &stdout)
        )));
    }

    let binary = local_bin.join("starship");
    if !binary.is_file() {
        return Err(crate::error::SlateError::Internal(format!(
            "Starship local fallback completed without creating {}",
            binary.display()
        )));
    }

    Ok(())
}

pub(super) fn first_meaningful_command_line(stderr: &str, stdout: &str) -> String {
    stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("==>"))
        .unwrap_or("unknown error")
        .to_string()
}

/// Classify brew stderr into a one-line guided message
pub(super) fn classify_brew_error(package: &str, stderr: &str) -> String {
    let lower = stderr.to_lowercase();
    if lower.contains("couldn't connect to server")
        || lower.contains("could not resolve host")
        || lower.contains("network is unreachable")
    {
        format!(
            "{} — network unreachable. Check your connection and retry: slate setup --only {}",
            package, package
        )
    } else if lower.contains("is not writable") || lower.contains("permission denied") {
        format!(
            "{} — permission denied. On a shared Homebrew install, ask the primary user or admin to install this package, then rerun slate setup.",
            package
        )
    } else if lower.contains("already installed") {
        format!("{} — already installed", package)
    } else {
        let first_line = stderr
            .lines()
            .find(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with("==>")
            })
            .unwrap_or(stderr.lines().next().unwrap_or("unknown error"));
        format!("{} — {}", package, first_line.trim())
    }
}
