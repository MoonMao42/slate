use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::platform::capabilities::CapabilityReport;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManagerBackend {
    Homebrew,
    Apt,
    Yay,
    Pacman,
    Unsupported,
}

impl PackageManagerBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew",
            Self::Apt => "apt",
            Self::Yay => "yay",
            Self::Pacman => "pacman",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn is_supported(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

pub fn detect_backend() -> PackageManagerBackend {
    if cfg!(target_os = "macos") {
        if detection::homebrew_executable().is_some() {
            return PackageManagerBackend::Homebrew;
        }
        return PackageManagerBackend::Unsupported;
    }

    if cfg!(target_os = "linux") {
        if detection::command_path("apt-get").is_some() {
            return PackageManagerBackend::Apt;
        }
        // Prefer yay over pacman — yay covers both official repos and AUR.
        if detection::command_path("yay").is_some() {
            return PackageManagerBackend::Yay;
        }
        if detection::command_path("pacman").is_some() {
            return PackageManagerBackend::Pacman;
        }
    }

    PackageManagerBackend::Unsupported
}

fn capability_report_for_backend(backend: PackageManagerBackend) -> CapabilityReport {
    match backend {
        PackageManagerBackend::Homebrew => CapabilityReport::supported("homebrew"),
        PackageManagerBackend::Apt => CapabilityReport::supported("apt"),
        PackageManagerBackend::Yay => CapabilityReport::supported("yay"),
        PackageManagerBackend::Pacman => CapabilityReport::supported("pacman"),
        PackageManagerBackend::Unsupported => CapabilityReport::unsupported(
            "unsupported",
            "No supported package manager was found. Slate currently supports Homebrew on macOS, apt on Debian/Ubuntu, and pacman/yay on Arch Linux.",
        ),
    }
}

pub fn capability_report() -> CapabilityReport {
    capability_report_for_backend(detect_backend())
}

pub fn install_tool_package(tool_id: &str, brew_package: &str, env: &SlateEnv) -> Result<()> {
    match detect_backend() {
        PackageManagerBackend::Homebrew => install_with_homebrew(brew_package),
        PackageManagerBackend::Apt => install_with_apt(tool_id, env),
        PackageManagerBackend::Yay => install_with_yay(tool_id),
        PackageManagerBackend::Pacman => install_with_pacman(tool_id),
        PackageManagerBackend::Unsupported => Err(SlateError::Internal(
            "No supported package manager was found. Slate currently supports Homebrew on macOS, apt on Debian/Ubuntu, and pacman/yay on Arch Linux.".to_string(),
        )),
    }
}

fn install_with_homebrew(package: &str) -> Result<()> {
    let brew = detection::homebrew_executable().ok_or_else(|| {
        SlateError::Internal(
            "Homebrew was not found. Install it first or add it to PATH.".to_string(),
        )
    })?;

    let mut cmd = Command::new(brew);
    detection::apply_normalized_path(&mut cmd);
    let output = cmd
        .arg("install")
        .arg(package)
        .output()
        .map_err(|e| SlateError::Internal(format!("Failed to execute brew: {}", e)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(SlateError::Internal(stderr.trim().to_string()))
    }
}

fn install_with_apt(tool_id: &str, _env: &SlateEnv) -> Result<()> {
    if tool_id == "starship" {
        return Err(SlateError::Internal(
            "starship uses Slate's user-local installer path on Linux.".to_string(),
        ));
    }

    let package = apt_package_name(tool_id).ok_or_else(|| {
        SlateError::Internal(format!(
            "Slate does not have an apt package mapping for '{}'. Install it manually, then rerun slate setup.",
            tool_id
        ))
    })?;

    let apt_get = detection::command_path("apt-get").ok_or_else(|| {
        SlateError::Internal(
            "apt-get was not found. Install apt or use a supported Linux distro.".to_string(),
        )
    })?;

    let mut cmd = Command::new("sudo");
    detection::apply_normalized_path(&mut cmd);
    let output = cmd
        .arg(apt_get)
        .args(["install", "-y", package])
        .output()
        .map_err(|e| SlateError::Internal(format!("Failed to execute apt-get: {}", e)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(SlateError::Internal(stderr.trim().to_string()))
    }
}

fn apt_package_name(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        "bat" => Some("bat"),
        "delta" => Some("git-delta"),
        "eza" => Some("eza"),
        "lazygit" => Some("lazygit"),
        "fastfetch" => Some("fastfetch"),
        "zsh-syntax-highlighting" => Some("zsh-syntax-highlighting"),
        _ => None,
    }
}

fn install_with_yay(tool_id: &str) -> Result<()> {
    if tool_id == "starship" {
        return Err(SlateError::Internal(
            "starship uses Slate's user-local installer path on Linux.".to_string(),
        ));
    }

    let package = pacman_package_name(tool_id).ok_or_else(|| {
        SlateError::Internal(format!(
            "Slate does not have a pacman/yay package mapping for '{}'. Install it manually, then rerun slate setup.",
            tool_id
        ))
    })?;

    let yay = detection::command_path("yay").ok_or_else(|| {
        SlateError::Internal("yay was not found in PATH.".to_string())
    })?;

    let mut cmd = Command::new(yay);
    detection::apply_normalized_path(&mut cmd);
    let output = cmd
        .args(["--noconfirm", "-S", package])
        .output()
        .map_err(|e| SlateError::Internal(format!("Failed to execute yay: {}", e)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(SlateError::Internal(stderr.trim().to_string()))
    }
}

fn install_with_pacman(tool_id: &str) -> Result<()> {
    if tool_id == "starship" {
        return Err(SlateError::Internal(
            "starship uses Slate's user-local installer path on Linux.".to_string(),
        ));
    }

    let package = pacman_package_name(tool_id).ok_or_else(|| {
        SlateError::Internal(format!(
            "Slate does not have a pacman package mapping for '{}'. Install it manually, then rerun slate setup.",
            tool_id
        ))
    })?;

    let pacman = detection::command_path("pacman").ok_or_else(|| {
        SlateError::Internal("pacman was not found in PATH.".to_string())
    })?;

    let mut cmd = Command::new("sudo");
    detection::apply_normalized_path(&mut cmd);
    let output = cmd
        .arg(pacman)
        .args(["--noconfirm", "-S", package])
        .output()
        .map_err(|e| SlateError::Internal(format!("Failed to execute pacman: {}", e)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(SlateError::Internal(stderr.trim().to_string()))
    }
}

fn pacman_package_name(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        "bat" => Some("bat"),
        "delta" => Some("git-delta"),
        "eza" => Some("eza"),
        "lazygit" => Some("lazygit"),
        "fastfetch" => Some("fastfetch"),
        "zsh-syntax-highlighting" => Some("zsh-syntax-highlighting"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_labels() {
        assert_eq!(PackageManagerBackend::Homebrew.label(), "Homebrew");
        assert_eq!(PackageManagerBackend::Apt.label(), "apt");
        assert_eq!(PackageManagerBackend::Yay.label(), "yay");
        assert_eq!(PackageManagerBackend::Pacman.label(), "pacman");
        assert_eq!(PackageManagerBackend::Unsupported.label(), "unsupported");
    }

    #[test]
    fn test_backend_is_supported() {
        assert!(PackageManagerBackend::Apt.is_supported());
        assert!(PackageManagerBackend::Yay.is_supported());
        assert!(PackageManagerBackend::Pacman.is_supported());
        assert!(!PackageManagerBackend::Unsupported.is_supported());
    }

    #[test]
    fn test_apt_mappings_cover_core_packages() {
        assert_eq!(apt_package_name("bat"), Some("bat"));
        assert_eq!(apt_package_name("delta"), Some("git-delta"));
    }

    #[test]
    fn test_pacman_mappings_cover_core_packages() {
        assert_eq!(pacman_package_name("bat"), Some("bat"));
        assert_eq!(pacman_package_name("delta"), Some("git-delta"));
        assert_eq!(pacman_package_name("eza"), Some("eza"));
        assert_eq!(pacman_package_name("lazygit"), Some("lazygit"));
        assert_eq!(pacman_package_name("fastfetch"), Some("fastfetch"));
        assert_eq!(pacman_package_name("unknown-tool"), None);
    }

    #[test]
    fn test_capability_report_for_apt_reports_supported() {
        let report = capability_report_for_backend(PackageManagerBackend::Apt);

        assert_eq!(
            report.level,
            crate::platform::capabilities::SupportLevel::Supported
        );
        assert_eq!(report.backend, "apt");
        assert!(report.reason.is_none());
    }

    #[test]
    fn test_capability_report_for_yay_reports_supported() {
        let report = capability_report_for_backend(PackageManagerBackend::Yay);

        assert_eq!(
            report.level,
            crate::platform::capabilities::SupportLevel::Supported
        );
        assert_eq!(report.backend, "yay");
        assert!(report.reason.is_none());
    }

    #[test]
    fn test_capability_report_for_pacman_reports_supported() {
        let report = capability_report_for_backend(PackageManagerBackend::Pacman);

        assert_eq!(
            report.level,
            crate::platform::capabilities::SupportLevel::Supported
        );
        assert_eq!(report.backend, "pacman");
        assert!(report.reason.is_none());
    }
}
