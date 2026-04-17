use crate::error::{Result, SlateError};
use crate::platform::capabilities::{CapabilityReport, SupportLevel};
use std::path::Path;
use std::process::{Command, Stdio};

const GNOME_FALLBACK_REASON: &str =
    "XDG desktop portal screenshot capture was unavailable, so Slate fell back to GNOME screenshot.";
const UNSUPPORTED_CAPTURE_REASON: &str =
    "No supported screenshot backend was found. Share URI export is still available.";
const GNOME_MISSING_REASON: &str = "GNOME screenshot fallback requires gnome-screenshot.";
const GNOME_CANCELLED_REASON: &str = "GNOME screenshot fallback was cancelled.";
const MACOS_CANCELLED_REASON: &str = "Screenshot cancelled or failed.";
const PORTAL_CANCELLED_REASON: &str = "Portal screenshot capture was cancelled.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareCaptureBackend {
    MacosScreenCapture,
    XdgDesktopPortal,
    GnomeScreenshot,
    Unsupported,
}

impl ShareCaptureBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::MacosScreenCapture => "macOS screencapture",
            Self::XdgDesktopPortal => "XDG desktop portal",
            Self::GnomeScreenshot => "GNOME screenshot",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareCaptureResult {
    pub captured: bool,
    pub reason: Option<String>,
}

pub fn detect_backend() -> ShareCaptureBackend {
    if cfg!(target_os = "macos") {
        return ShareCaptureBackend::MacosScreenCapture;
    }

    if cfg!(target_os = "linux") && crate::platform::portal::screenshot_available() {
        return ShareCaptureBackend::XdgDesktopPortal;
    }

    if cfg!(target_os = "linux")
        && crate::platform::desktop::is_gnome_session()
        && crate::detection::command_path("gnome-screenshot").is_some()
    {
        return ShareCaptureBackend::GnomeScreenshot;
    }

    ShareCaptureBackend::Unsupported
}

fn capability_report_for_backend(backend: ShareCaptureBackend) -> CapabilityReport {
    match backend {
        ShareCaptureBackend::MacosScreenCapture => {
            CapabilityReport::supported("macos-screencapture")
        }
        ShareCaptureBackend::XdgDesktopPortal => CapabilityReport::supported("xdg-desktop-portal"),
        ShareCaptureBackend::GnomeScreenshot => CapabilityReport {
            level: SupportLevel::BestEffort,
            backend: "gnome-screenshot",
            reason: Some(GNOME_FALLBACK_REASON.to_string()),
        },
        ShareCaptureBackend::Unsupported => {
            CapabilityReport::unsupported("unsupported", UNSUPPORTED_CAPTURE_REASON)
        }
    }
}

pub fn capability_report() -> CapabilityReport {
    capability_report_for_backend(detect_backend())
}

pub fn capture_interactive(output_path: &Path) -> Result<ShareCaptureResult> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match detect_backend() {
        ShareCaptureBackend::MacosScreenCapture => {
            let status = Command::new("screencapture")
                .args([
                    "-w",
                    "-o",
                    output_path.to_str().unwrap_or("slate-share.png"),
                ])
                .status();
            match status {
                Ok(status) if status.success() => Ok(ShareCaptureResult {
                    captured: true,
                    reason: None,
                }),
                _ => Ok(ShareCaptureResult {
                    captured: false,
                    reason: Some(MACOS_CANCELLED_REASON.to_string()),
                }),
            }
        }
        ShareCaptureBackend::XdgDesktopPortal => {
            match crate::platform::portal::take_interactive_screenshot(output_path)? {
                crate::platform::portal::PortalCaptureStatus::Captured => Ok(ShareCaptureResult {
                    captured: true,
                    reason: None,
                }),
                crate::platform::portal::PortalCaptureStatus::Cancelled => Ok(ShareCaptureResult {
                    captured: false,
                    reason: Some(PORTAL_CANCELLED_REASON.to_string()),
                }),
            }
        }
        ShareCaptureBackend::GnomeScreenshot => {
            let Some(command) = crate::detection::command_path("gnome-screenshot") else {
                return Ok(ShareCaptureResult {
                    captured: false,
                    reason: Some(GNOME_MISSING_REASON.to_string()),
                });
            };

            let status = Command::new(command)
                .args([
                    "-a",
                    "-f",
                    output_path.to_str().unwrap_or("slate-share.png"),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|err| {
                    SlateError::PlatformError(format!(
                        "Failed to launch GNOME screenshot backend: {}",
                        err
                    ))
                })?;

            if status.success() {
                Ok(ShareCaptureResult {
                    captured: true,
                    reason: None,
                })
            } else {
                Ok(ShareCaptureResult {
                    captured: false,
                    reason: Some(GNOME_CANCELLED_REASON.to_string()),
                })
            }
        }
        ShareCaptureBackend::Unsupported => Ok(ShareCaptureResult {
            captured: false,
            reason: Some(UNSUPPORTED_CAPTURE_REASON.to_string()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_labels() {
        assert_eq!(
            ShareCaptureBackend::MacosScreenCapture.label(),
            "macOS screencapture"
        );
        assert_eq!(
            ShareCaptureBackend::XdgDesktopPortal.label(),
            "XDG desktop portal"
        );
        assert_eq!(
            ShareCaptureBackend::GnomeScreenshot.label(),
            "GNOME screenshot"
        );
    }

    #[test]
    fn test_capability_report_for_portal_backend_is_supported() {
        let report = capability_report_for_backend(ShareCaptureBackend::XdgDesktopPortal);

        assert_eq!(report.level, SupportLevel::Supported);
        assert_eq!(report.backend, "xdg-desktop-portal");
    }

    #[test]
    fn test_capability_report_for_gnome_backend_uses_best_effort() {
        let report = capability_report_for_backend(ShareCaptureBackend::GnomeScreenshot);

        assert_eq!(report.level, SupportLevel::BestEffort);
        assert_eq!(report.backend, "gnome-screenshot");
        assert_eq!(report.reason.as_deref(), Some(GNOME_FALLBACK_REASON));
    }

    #[test]
    fn test_capability_report_for_unsupported_backend_preserves_share_uri_reason() {
        let report = capability_report_for_backend(ShareCaptureBackend::Unsupported);

        assert_eq!(report.level, SupportLevel::Unsupported);
        assert_eq!(report.backend, "unsupported");
        assert!(report
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("Share URI export is still available"));
    }
}
