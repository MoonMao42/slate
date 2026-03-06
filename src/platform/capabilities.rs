use crate::detection::{TerminalKind, TerminalProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportLevel {
    Supported,
    BestEffort,
    Unsupported,
    MissingDependency,
}

impl SupportLevel {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::BestEffort => "best effort",
            Self::Unsupported => "unsupported",
            Self::MissingDependency => "missing dependency",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityReport {
    pub level: SupportLevel,
    pub backend: &'static str,
    pub reason: Option<String>,
}

impl CapabilityReport {
    pub fn supported(backend: &'static str) -> Self {
        Self {
            level: SupportLevel::Supported,
            backend,
            reason: None,
        }
    }

    pub fn best_effort(backend: &'static str, reason: impl Into<String>) -> Self {
        Self {
            level: SupportLevel::BestEffort,
            backend,
            reason: Some(reason.into()),
        }
    }

    pub fn unsupported(backend: &'static str, reason: impl Into<String>) -> Self {
        Self {
            level: SupportLevel::Unsupported,
            backend,
            reason: Some(reason.into()),
        }
    }

    pub fn missing_dependency(backend: &'static str, reason: impl Into<String>) -> Self {
        Self {
            level: SupportLevel::MissingDependency,
            backend,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilitySnapshot {
    pub os: CapabilityReport,
    pub arch: CapabilityReport,
    pub shell: CapabilityReport,
    pub terminal: CapabilityReport,
    pub desktop_appearance: CapabilityReport,
    pub share_capture: CapabilityReport,
    pub font_platform: CapabilityReport,
    pub package_manager: CapabilityReport,
}

pub fn detect_capabilities() -> CapabilitySnapshot {
    CapabilitySnapshot {
        os: os_capability_report(),
        arch: arch_capability_report(),
        shell: crate::platform::shell::capability_report(),
        terminal: terminal_capability_report(&TerminalProfile::detect()),
        desktop_appearance: crate::platform::desktop::capability_report(),
        share_capture: crate::platform::share::capability_report(),
        font_platform: crate::platform::fonts::capability_report(),
        package_manager: crate::platform::packages::capability_report(),
    }
}

fn os_capability_report() -> CapabilityReport {
    if cfg!(target_os = "macos") {
        CapabilityReport::supported("macos")
    } else if cfg!(target_os = "linux") {
        CapabilityReport::supported("linux")
    } else {
        CapabilityReport::unsupported(
            "unsupported",
            "Slate v2.1 officially supports macOS and Linux only.",
        )
    }
}

fn arch_capability_report() -> CapabilityReport {
    if cfg!(target_arch = "x86_64") {
        CapabilityReport::supported("x86_64")
    } else if cfg!(target_arch = "aarch64") {
        CapabilityReport::supported("aarch64")
    } else {
        CapabilityReport::unsupported(
            "unsupported",
            "Slate v2.1 officially supports x86_64 and aarch64 targets only.",
        )
    }
}

pub fn terminal_capability_report(profile: &TerminalProfile) -> CapabilityReport {
    match profile.kind() {
        TerminalKind::Ghostty => CapabilityReport::supported("ghostty"),
        TerminalKind::Kitty => CapabilityReport::supported("kitty"),
        TerminalKind::Alacritty => {
            CapabilityReport::best_effort("alacritty", profile.compatibility_summary())
        }
        TerminalKind::TerminalApp => {
            CapabilityReport::best_effort("terminal-app", profile.compatibility_summary())
        }
        TerminalKind::Unknown => {
            CapabilityReport::best_effort("unknown-terminal", profile.compatibility_summary())
        }
    }
}

// Later portal/D-Bus listeners must stay behind backend seams rather than forcing shared CLI
// handlers to become async during.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::TerminalProfile;

    #[test]
    fn test_support_level_labels_are_stable() {
        assert_eq!(SupportLevel::Supported.label(), "supported");
        assert_eq!(SupportLevel::BestEffort.label(), "best effort");
        assert_eq!(SupportLevel::Unsupported.label(), "unsupported");
        assert_eq!(
            SupportLevel::MissingDependency.label(),
            "missing dependency"
        );
    }

    #[test]
    fn test_terminal_capability_report_uses_supported_for_kitty() {
        let profile = TerminalProfile::from_env_vars(Some("kitty"), None);
        let report = terminal_capability_report(&profile);

        assert_eq!(report.level, SupportLevel::Supported);
        assert_eq!(report.backend, "kitty");
        assert!(report.reason.is_none());
    }

    #[test]
    fn test_terminal_capability_report_uses_best_effort_for_unknown_terminal() {
        let profile = TerminalProfile::from_env_vars(Some("wezterm"), None);
        let report = terminal_capability_report(&profile);

        assert_eq!(report.level, SupportLevel::BestEffort);
        assert_eq!(report.backend, "unknown-terminal");
        assert!(report.reason.is_some());
    }

    #[test]
    fn test_detect_capabilities_populates_all_sections() {
        let snapshot = detect_capabilities();

        assert!(!snapshot.os.backend.is_empty());
        assert!(!snapshot.arch.backend.is_empty());
        assert!(!snapshot.shell.backend.is_empty());
        assert!(!snapshot.terminal.backend.is_empty());
        assert!(!snapshot.desktop_appearance.backend.is_empty());
        assert!(!snapshot.share_capture.backend.is_empty());
        assert!(!snapshot.font_platform.backend.is_empty());
        assert!(!snapshot.package_manager.backend.is_empty());
    }
}
