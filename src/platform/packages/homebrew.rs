//! Bounded Homebrew installation. Interruption is not a known failed exit:
//! package-manager mutations are not rolled back and automatic fallbacks stop.
use super::BrewKind;
use crate::{
    error::{Result, SlateError},
    platform::process_output::{self, Completion, Limits},
};
use std::{path::Path, process::Command, time::Duration};

pub(crate) const FONT_LIMITS: Limits = Limits {
    timeout: Duration::from_secs(600),
    max_output: 512 * 1024,
};

pub(crate) const TOOL_LIMITS: Limits = Limits {
    timeout: Duration::from_secs(1800),
    max_output: 2 * 1024 * 1024,
};

pub(crate) fn install(brew: &Path, package: &str, kind: BrewKind, limits: Limits) -> Result<()> {
    let mut command = Command::new(brew);
    crate::detection::apply_normalized_path(&mut command);
    command.arg("install");
    if matches!(kind, BrewKind::Cask) {
        command.arg("--cask");
    }
    command.arg(package);
    let output = process_output::capture(&mut command, limits).map_err(|_| {
        SlateError::HomebrewInstallUncertain("could not start or capture Homebrew".into())
    })?;
    match output.completion {
        Completion::Exited(status) if status.success() => Ok(()),
        Completion::Exited(status) if status.code().is_some() => {
            // Classify bounded stderr, never echo native text or paths. Normal
            // failed exits retain the existing fallback policy; they do not
            // guarantee that Homebrew left no partial changes.
            let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
            let reason =
                if stderr.contains("is not writable") || stderr.contains("permission denied") {
                    "permission denied; ask the Homebrew owner or administrator to inspect access"
                } else if stderr.contains("couldn't connect to server")
                    || stderr.contains("could not resolve host")
                    || stderr.contains("network is unreachable")
                {
                    "network unreachable; check the connection"
                } else {
                    "inspect Homebrew for details"
                };
            Err(SlateError::Internal(format!(
                "Homebrew installation for {package} failed with exit code {}: {reason}. Command output omitted.",
                status.code().expect("known exit code"),
            )))
        }
        Completion::Exited(_) => Err(SlateError::HomebrewInstallUncertain(
            "Homebrew terminated by a signal".into(),
        )),
        Completion::TimedOut => Err(SlateError::HomebrewInstallUncertain(format!(
            "Homebrew exceeded the {}-second wait limit",
            limits.timeout.as_secs(),
        ))),
        Completion::OutputLimit => Err(SlateError::HomebrewInstallUncertain(format!(
            "Homebrew exceeded the {}-byte combined output limit",
            limits.max_output,
        ))),
    }
}
