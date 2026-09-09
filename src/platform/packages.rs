use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::platform::capabilities::CapabilityReport;
pub(crate) mod apt;
pub(crate) mod homebrew;
mod routes;
pub(crate) use routes::{InstallContext, ToolInstallRoute};

/// Homebrew package kind, shared by platform APIs and the wizard catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrewKind {
    Formula,
    Cask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManagerBackend {
    Homebrew,
    Apt,
    Unsupported,
}

impl PackageManagerBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew",
            Self::Apt => "apt",
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

    if cfg!(target_os = "linux") && detection::command_path("apt-get").is_some() {
        return PackageManagerBackend::Apt;
    }

    PackageManagerBackend::Unsupported
}

fn capability_report_for_backend(backend: PackageManagerBackend) -> CapabilityReport {
    match backend {
        PackageManagerBackend::Homebrew => CapabilityReport::supported("homebrew"),
        PackageManagerBackend::Apt => CapabilityReport::supported("apt"),
        PackageManagerBackend::Unsupported => CapabilityReport::unsupported(
            "unsupported",
            "No supported package manager was found. Slate currently supports Homebrew on macOS and apt on Linux.",
        ),
    }
}

pub fn capability_report() -> CapabilityReport {
    capability_report_for_backend(detect_backend())
}

/// Formula-only package API. Wizard casks and Starship's user-local route use
/// their explicit policies; all native package installs share bounded capture.
pub fn install_tool_package(tool_id: &str, brew_package: &str, _env: &SlateEnv) -> Result<()> {
    match detect_backend() {
        PackageManagerBackend::Homebrew => install_with_homebrew(brew_package, BrewKind::Formula),
        PackageManagerBackend::Apt => apt::install(tool_id),
        PackageManagerBackend::Unsupported => Err(SlateError::Internal(
            "No supported package manager was found. Slate currently supports Homebrew on macOS and apt on Linux.".to_string(),
        )),
    }
}

pub(crate) fn install_with_homebrew(package: &str, kind: BrewKind) -> Result<()> {
    let brew = detection::homebrew_executable().ok_or_else(|| {
        SlateError::Internal(
            "Homebrew was not found. Install it first or add it to PATH.".to_string(),
        )
    })?;
    homebrew::install(&brew, package, kind, homebrew::TOOL_LIMITS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_labels() {
        assert_eq!(PackageManagerBackend::Homebrew.label(), "Homebrew");
        assert_eq!(PackageManagerBackend::Apt.label(), "apt");
    }

    #[test]
    fn test_apt_mappings_cover_core_packages() {
        assert_eq!(apt::package_name("bat"), Some("bat"));
        assert_eq!(apt::package_name("delta"), Some("git-delta"));
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
}
