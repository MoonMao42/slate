//! Opt-in native diagnostics, separate from the default file-only Neovim scan.
use super::Report;
use crate::adapter::nvim::availability::{self, NvimProbe};
use crate::env::SlateEnv;
use crate::platform::version_check::{VersionPolicy, PROBE_LIMITS};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub(super) struct VersionProbe {
    status: &'static str,
    binary: Option<String>,
    binary_path_is_lossy: bool,
    in_path: bool,
    version: Option<String>,
    minimum_version: &'static str,
    timeout_ms: u128,
    max_output_bytes: usize,
    message: String,
}

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    append(report, availability::inspect(env));
}

fn append(report: &mut Report, probe: NvimProbe) {
    report.scope = "File checks plus an explicitly requested native `nvim --version` check. Slate does not write configuration, install tools, activate hooks or launch an interactive editor. The executable is not sandboxed and may have its own side effects. The probe uses a post-spawn deadline and combined output cap; OS calls and deliberately detached descendants are not hard-bounded. Raw program output is omitted. A supported version does not prove runtime API compatibility or live editor behavior. Exit success means a report was produced, not that every check passed.";
    let binary = probe.binary.as_deref();
    let minimum = VersionPolicy::min_version("nvim").expect("Neovim has a version policy");
    let (status, severity, version, message, suggestion) = match probe.version {
        Ok(None) => (
            "missing", "warning", None,
            "No Neovim executable was found; no version command was started".into(),
            Some("Install Neovim or check its PATH/home-local location, then rerun this diagnostic.".into()),
        ),
        Ok(Some(version)) if VersionPolicy::supports("nvim", &version) => (
            "supported", "ok", Some(version.to_string()),
            format!("Neovim {version} meets the minimum version {minimum}"),
            (!probe.in_path).then(|| "The executable was found outside PATH; a direct shell command may resolve differently.".into()),
        ),
        Ok(Some(version)) => (
            "unsupported", "warning", Some(version.to_string()),
            format!("Neovim {version} is below the minimum version {minimum}"),
            Some(format!("Upgrade the reported executable to Neovim {minimum} or newer, then rerun this diagnostic.")),
        ),
        Err(error) => (
            "failed", "error", None,
            format!("Neovim version check failed: {error}"),
            Some("Inspect the reported executable's permissions and --version behavior. No setup or activation was attempted.".into()),
        ),
    };
    report.add_code(
        "version_probe",
        severity,
        &message,
        binary.unwrap_or(Path::new("nvim")),
        suggestion,
    );
    report.version_probe = Some(VersionProbe {
        status,
        binary: binary.map(|path| path.display().to_string()),
        binary_path_is_lossy: binary.is_some_and(|path| path.to_str().is_none()),
        in_path: probe.in_path,
        version,
        minimum_version: minimum,
        timeout_ms: PROBE_LIMITS.timeout.as_millis(),
        max_output_bytes: PROBE_LIMITS.max_output,
        message,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SlateError;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn nvim_version_report_distinguishes_missing_and_failed_without_native_calls() {
        let td = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        let mut report = super::super::inspect("nvim", &env);
        assert!(report.version_probe.is_none());
        append(
            &mut report,
            NvimProbe {
                binary: None,
                in_path: false,
                version: Ok(None),
            },
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["version_probe"]["status"], "missing");
        assert!(json["version_probe"]["binary"].is_null());
        assert!(json["version_probe"]["version"].is_null());

        let binary = Path::new(std::ffi::OsStr::from_bytes(b"/private/nvim-\xff\x1b"));
        let mut report = super::super::inspect("nvim", &env);
        append(
            &mut report,
            NvimProbe {
                binary: Some(binary.into()),
                in_path: false,
                version: Err(SlateError::PlatformError("version unavailable".into())),
            },
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["version_probe"]["status"], "failed");
        assert_eq!(json["version_probe"]["binary_path_is_lossy"], true);
        assert_eq!(json["version_probe"]["in_path"], false);
        assert!(json["version_probe"]["version"].is_null());
        let text = super::super::text_report(&report);
        assert!(!text.contains('\u{1b}'));
        assert!(text.contains("lossy display"));
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
    }
}
