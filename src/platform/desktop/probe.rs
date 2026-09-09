use crate::{
    error::{Result, SlateError},
    platform::process_output::{self, Completion, Limits},
    theme::ThemeAppearance,
};
use std::{path::Path, process::Command, time::Duration};

const LIMITS: Limits = Limits {
    timeout: Duration::from_secs(2),
    max_output: 8 * 1024,
};

#[derive(Clone, Copy)]
pub(super) enum Kind {
    MacosDefaults,
    GnomeGsettings,
}

impl Kind {
    fn command(self) -> &'static str {
        match self {
            Self::MacosDefaults => "defaults",
            Self::GnomeGsettings => "gsettings",
        }
    }
    fn args(self) -> [&'static str; 3] {
        match self {
            Self::MacosDefaults => ["read", "-g", "AppleInterfaceStyle"],
            Self::GnomeGsettings => ["get", "org.gnome.desktop.interface", "color-scheme"],
        }
    }
    fn error(self, reason: impl std::fmt::Display) -> SlateError {
        SlateError::PlatformError(format!(
            "{} appearance query {reason}; native output omitted",
            self.command()
        ))
    }
}

pub(super) fn query(kind: Kind) -> Result<ThemeAppearance> {
    let binary = crate::detection::command_in_actual_path(kind.command())
        .or_else(|| crate::detection::command_path(kind.command()))
        .ok_or_else(|| kind.error("executable is unavailable"))?;
    query_binary(kind, &binary, LIMITS)
}

fn query_binary(kind: Kind, binary: &Path, limits: Limits) -> Result<ThemeAppearance> {
    let binary = std::path::absolute(binary)
        .map_err(|e| kind.error(format!("path resolution failed ({:?})", e.kind())))?;
    let output = process_output::capture(
        Command::new(binary).args(kind.args()).env("LC_ALL", "C"),
        limits,
    )
    .map_err(|e| kind.error(format!("could not complete ({:?})", e.kind())))?;
    let status = match output.completion {
        Completion::TimedOut => {
            return Err(kind.error(format!("timed out after {} ms", limits.timeout.as_millis())))
        }
        Completion::OutputLimit => {
            return Err(kind.error(format!(
                "exceeded {} bytes of combined output",
                limits.max_output
            )))
        }
        Completion::Exited(status) => status,
    };
    if matches!(kind, Kind::MacosDefaults)
        && status.code() == Some(1)
        && output.stdout.is_empty()
        && missing_macos_preference(&output.stderr)
    {
        return Ok(ThemeAppearance::Light);
    }
    if !status.success() {
        return Err(kind.error(format!("failed ({status})")));
    }
    let value =
        std::str::from_utf8(&output.stdout).map_err(|_| kind.error("returned non-UTF-8 data"))?;
    let appearance = match kind {
        Kind::MacosDefaults => match value.trim() {
            "Dark" => Some(ThemeAppearance::Dark),
            "Light" => Some(ThemeAppearance::Light),
            _ => None,
        },
        Kind::GnomeGsettings => parse_gnome_value(value),
    };
    appearance.ok_or_else(|| kind.error("returned an unrecognized value"))
}

fn missing_macos_preference(stderr: &[u8]) -> bool {
    std::str::from_utf8(stderr).is_ok_and(|text| text.lines().any(|line| matches!(line.trim(),
        "Error: Could not find key 'AppleInterfaceStyle' in domain 'kCFPreferencesAnyApplication'." |
        "The domain/default pair of (kCFPreferencesAnyApplication, AppleInterfaceStyle) does not exist"
    )))
}

pub(super) fn parse_gnome_value(value: &str) -> Option<ThemeAppearance> {
    match value.trim() {
        "'prefer-dark'" => Some(ThemeAppearance::Dark),
        "'prefer-light'" | "'default'" => Some(ThemeAppearance::Light),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, time::Instant};

    fn fixture(home: &Path, body: &str) -> std::path::PathBuf {
        let path = home.join("appearance-probe");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn appearance_probe_accepts_only_known_complete_values() {
        let td = tempfile::tempdir().unwrap();
        for (kind, body, expected) in [
            (
                Kind::MacosDefaults,
                "printf ' Dark\\n'",
                ThemeAppearance::Dark,
            ),
            (
                Kind::MacosDefaults,
                "printf 'Light\\n'",
                ThemeAppearance::Light,
            ),
            (
                Kind::GnomeGsettings,
                "printf \"'prefer-dark'\\n\"",
                ThemeAppearance::Dark,
            ),
            (
                Kind::GnomeGsettings,
                "printf \"'prefer-light'\\n\"",
                ThemeAppearance::Light,
            ),
            (
                Kind::GnomeGsettings,
                "printf \"'default'\\n\"",
                ThemeAppearance::Light,
            ),
        ] {
            let path = fixture(td.path(), body);
            assert_eq!(query_binary(kind, &path, LIMITS).unwrap(), expected);
        }
        for kind in [Kind::MacosDefaults, Kind::GnomeGsettings] {
            for body in [
                "printf 'PRIVATE Dark failed\\n'",
                "printf 'Dark\\nLight\\n'",
                "printf '\\377'",
                "exit 0",
            ] {
                let error = query_binary(kind, &fixture(td.path(), body), LIMITS)
                    .unwrap_err()
                    .to_string();
                assert!(!error.contains("PRIVATE"), "{error}");
                assert!(error.contains("native output omitted"), "{error}");
            }
        }
    }

    #[test]
    fn appearance_probe_missing_macos_preference_is_not_a_generic_error_fallback() {
        let td = tempfile::tempdir().unwrap();
        for message in [
            "Error: Could not find key 'AppleInterfaceStyle' in domain 'kCFPreferencesAnyApplication'.",
            "timestamp defaults[123:456]\nThe domain/default pair of (kCFPreferencesAnyApplication, AppleInterfaceStyle) does not exist",
        ] {
            let body = format!("printf '%s\\n' \"{message}\" >&2\nexit 1");
            assert_eq!(query_binary(Kind::MacosDefaults, &fixture(td.path(), &body), LIMITS).unwrap(), ThemeAppearance::Light);
        }
        for body in [
            "printf 'PRIVATE failure\\n' >&2\nexit 1",
            "printf \"Error: Could not find key 'OtherKey' in domain 'kCFPreferencesAnyApplication'.\\n\" >&2\nexit 1",
        ] {
            let error = query_binary(Kind::MacosDefaults, &fixture(td.path(), body), LIMITS).unwrap_err().to_string();
            assert!(error.contains("failed"), "{error}");
            assert!(!error.contains("PRIVATE"));
        }
    }

    #[test]
    fn appearance_probe_timeout_output_limit_and_spawn_errors_are_content_free() {
        let td = tempfile::tempdir().unwrap();
        for (body, expected) in [
            ("printf PRIVATE >&2\n/bin/sleep 10 &\nexit 0", "timed out"),
            (
                "while :; do printf PRIVATE_FLOOD >&2; done",
                "combined output",
            ),
            ("printf PRIVATE >&2\nexit 23", "failed"),
        ] {
            let path = fixture(td.path(), body);
            let started = Instant::now();
            let error = query_binary(
                Kind::GnomeGsettings,
                &path,
                Limits {
                    max_output: 256,
                    ..LIMITS
                },
            )
            .unwrap_err()
            .to_string();
            assert!(started.elapsed() < Duration::from_secs(6), "{error}");
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("PRIVATE"), "{error}");
        }
        let error = query_binary(
            Kind::MacosDefaults,
            &td.path().join("PRIVATE_missing"),
            LIMITS,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("NotFound"));
        assert!(!error.contains("PRIVATE"));
    }
}
