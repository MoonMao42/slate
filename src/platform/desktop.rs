use crate::error::{Result, SlateError};
use crate::platform::capabilities::{CapabilityReport, SupportLevel};
use crate::theme::ThemeAppearance;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopAppearanceBackend {
    MacosDefaults,
    XdgDesktopPortal,
    GnomeGsettings,
    Unsupported,
}

impl DesktopAppearanceBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::MacosDefaults => "macOS defaults",
            Self::XdgDesktopPortal => "XDG desktop portal",
            Self::GnomeGsettings => "GNOME gsettings",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn supports_watcher(self) -> bool {
        matches!(
            self,
            Self::MacosDefaults | Self::XdgDesktopPortal | Self::GnomeGsettings
        )
    }
}

pub fn detect_backend() -> DesktopAppearanceBackend {
    if cfg!(target_os = "macos") {
        return DesktopAppearanceBackend::MacosDefaults;
    }

    if cfg!(target_os = "linux") && crate::platform::portal::settings_available() {
        return DesktopAppearanceBackend::XdgDesktopPortal;
    }

    if cfg!(target_os = "linux")
        && is_gnome_session()
        && crate::detection::command_path("gsettings").is_some()
    {
        return DesktopAppearanceBackend::GnomeGsettings;
    }

    DesktopAppearanceBackend::Unsupported
}

fn capability_report_for_backend(backend: DesktopAppearanceBackend) -> CapabilityReport {
    match backend {
        DesktopAppearanceBackend::MacosDefaults => CapabilityReport::supported("macos-defaults"),
        DesktopAppearanceBackend::XdgDesktopPortal => {
            CapabilityReport::supported("xdg-desktop-portal")
        }
        DesktopAppearanceBackend::GnomeGsettings => CapabilityReport {
            level: SupportLevel::BestEffort,
            backend: "gnome-gsettings",
            reason: Some(
                "XDG desktop portal was unavailable, so Slate fell back to GNOME gsettings."
                    .to_string(),
            ),
        },
        DesktopAppearanceBackend::Unsupported => CapabilityReport::unsupported(
            "unsupported",
            "Desktop appearance detection is unavailable on this platform.",
        ),
    }
}

pub fn capability_report() -> CapabilityReport {
    capability_report_for_backend(detect_backend())
}

pub fn detect_system_appearance() -> ThemeAppearance {
    match detect_backend() {
        DesktopAppearanceBackend::MacosDefaults => detect_macos_appearance(),
        DesktopAppearanceBackend::XdgDesktopPortal => detect_portal_appearance()
            .or_else(detect_gnome_appearance_if_available)
            .unwrap_or(ThemeAppearance::Light),
        DesktopAppearanceBackend::GnomeGsettings => detect_gnome_appearance(),
        DesktopAppearanceBackend::Unsupported => ThemeAppearance::Light,
    }
}

pub fn watch_appearance_changes<F>(mut on_change: F) -> Result<()>
where
    F: FnMut(ThemeAppearance) -> Result<()>,
{
    match detect_backend() {
        DesktopAppearanceBackend::MacosDefaults => Err(SlateError::PlatformError(
            "macOS appearance watching is handled by the embedded Swift watcher.".to_string(),
        )),
        DesktopAppearanceBackend::XdgDesktopPortal => {
            crate::platform::portal::watch_color_scheme_changes(|value| {
                on_change(portal_color_scheme_to_appearance(value))
            })
        }
        DesktopAppearanceBackend::GnomeGsettings => watch_gnome_appearance_changes(on_change),
        DesktopAppearanceBackend::Unsupported => Err(SlateError::PlatformError(
            "Desktop appearance watching is unavailable on this platform.".to_string(),
        )),
    }
}

pub fn is_gnome_session() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("DESKTOP_SESSION"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    desktop.contains("gnome")
}

fn detect_macos_appearance() -> ThemeAppearance {
    match std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("Dark") {
                ThemeAppearance::Dark
            } else {
                ThemeAppearance::Light
            }
        }
        _ => ThemeAppearance::Light,
    }
}

fn portal_color_scheme_to_appearance(value: u32) -> ThemeAppearance {
    match value {
        1 => ThemeAppearance::Dark,
        2 => ThemeAppearance::Light,
        _ => ThemeAppearance::Light,
    }
}

fn detect_portal_appearance() -> Option<ThemeAppearance> {
    crate::platform::portal::read_color_scheme()
        .ok()
        .flatten()
        .map(portal_color_scheme_to_appearance)
}

fn detect_gnome_appearance_if_available() -> Option<ThemeAppearance> {
    if crate::detection::command_path("gsettings").is_some() {
        Some(detect_gnome_appearance())
    } else {
        None
    }
}

fn detect_gnome_appearance() -> ThemeAppearance {
    let Some(gsettings) = crate::detection::command_path("gsettings") else {
        return ThemeAppearance::Light;
    };

    match Command::new(gsettings)
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        Ok(output) if output.status.success() => {
            parse_gnome_color_scheme_output(&String::from_utf8_lossy(&output.stdout))
        }
        _ => ThemeAppearance::Light,
    }
}

fn parse_gnome_color_scheme_output(output: &str) -> ThemeAppearance {
    let stdout = output.to_ascii_lowercase();
    if stdout.contains("prefer-dark") || stdout.contains("dark") {
        ThemeAppearance::Dark
    } else {
        ThemeAppearance::Light
    }
}

fn watch_gnome_appearance_changes<F>(mut on_change: F) -> Result<()>
where
    F: FnMut(ThemeAppearance) -> Result<()>,
{
    let Some(gsettings) = crate::detection::command_path("gsettings") else {
        return Err(SlateError::PlatformError(
            "GNOME appearance watching requires gsettings.".to_string(),
        ));
    };

    let mut child = Command::new(gsettings)
        .args(["monitor", "org.gnome.desktop.interface", "color-scheme"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| {
            SlateError::PlatformError(format!(
                "Failed to launch GNOME appearance monitor: {}",
                err
            ))
        })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        SlateError::PlatformError("GNOME appearance monitor did not expose stdout.".to_string())
    })?;

    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(SlateError::IOError)?;
        if !line.to_ascii_lowercase().contains("color-scheme") {
            continue;
        }
        on_change(parse_gnome_color_scheme_output(&line))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_label() {
        assert_eq!(
            DesktopAppearanceBackend::MacosDefaults.label(),
            "macOS defaults"
        );
        assert_eq!(
            DesktopAppearanceBackend::XdgDesktopPortal.label(),
            "XDG desktop portal"
        );
        assert_eq!(
            DesktopAppearanceBackend::GnomeGsettings.label(),
            "GNOME gsettings"
        );
        assert_eq!(DesktopAppearanceBackend::Unsupported.label(), "unsupported");
    }

    #[test]
    fn test_supported_backends_report_watcher_support() {
        assert!(DesktopAppearanceBackend::MacosDefaults.supports_watcher());
        assert!(DesktopAppearanceBackend::XdgDesktopPortal.supports_watcher());
        assert!(DesktopAppearanceBackend::GnomeGsettings.supports_watcher());
        assert!(!DesktopAppearanceBackend::Unsupported.supports_watcher());
    }

    #[test]
    fn test_capability_report_for_portal_backend_is_supported() {
        let report = capability_report_for_backend(DesktopAppearanceBackend::XdgDesktopPortal);

        assert_eq!(report.level, SupportLevel::Supported);
        assert_eq!(report.backend, "xdg-desktop-portal");
    }

    #[test]
    fn test_capability_report_for_gnome_backend_uses_best_effort() {
        let report = capability_report_for_backend(DesktopAppearanceBackend::GnomeGsettings);

        assert_eq!(report.level, SupportLevel::BestEffort);
        assert_eq!(report.backend, "gnome-gsettings");
        assert_eq!(
            report.reason.as_deref(),
            Some("XDG desktop portal was unavailable, so Slate fell back to GNOME gsettings.")
        );
    }

    #[test]
    fn test_portal_color_scheme_mapping() {
        assert_eq!(portal_color_scheme_to_appearance(1), ThemeAppearance::Dark);
        assert_eq!(portal_color_scheme_to_appearance(2), ThemeAppearance::Light);
        assert_eq!(portal_color_scheme_to_appearance(0), ThemeAppearance::Light);
    }

    #[test]
    fn test_parse_gnome_color_scheme_output() {
        assert_eq!(
            parse_gnome_color_scheme_output("color-scheme: 'prefer-dark'"),
            ThemeAppearance::Dark
        );
        assert_eq!(
            parse_gnome_color_scheme_output("color-scheme: 'default'"),
            ThemeAppearance::Light
        );
    }
}
