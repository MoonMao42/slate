//! Bounded, source-free observations independent of writer/recovery storage.
use super::Report;
use crate::{
    config::{
        file_read::{self, Links, MAX_STATE_BYTES},
        recovery_paths,
    },
    detection::{detect_tool_presence_with_env, ToolEvidence},
    env::SlateEnv,
    theme::{ThemeRegistry, ThemeVariant},
};
use std::path::Path;

/// Missing is distinct from unreadable. Neither is evidence of disconnection.
pub(super) fn read(
    report: &mut Report,
    env: &SlateEnv,
    path: &Path,
    limit: u64,
    code: &'static str,
    label: &str,
) -> Result<Option<String>, ()> {
    let read = || {
        recovery_paths::validate_file_path(env, path, label).map_err(|_| ())?;
        file_read::read(path, limit, Links::Reject)
            .map_err(|_| ())?
            .map(|source| String::from_utf8(source.bytes).map_err(|_| ()))
            .transpose()
    };
    match read() {
        Ok(content) => {
            report.add_code(
                code,
                if content.is_some() { "ok" } else { "info" },
                format!(
                    "{label} is {}",
                    if content.is_some() {
                        "readable"
                    } else {
                        "absent"
                    }
                ),
                path,
                None,
            );
            Ok(content)
        }
        Err(()) => {
            report.add_code(code, "error", format!("Cannot safely inspect {label}; file contents omitted"), path,
                Some(format!("Check UTF-8 encoding, regular-file access and the {limit}-byte limit. Final file symlinks and paths outside an isolated profile are not read; review linked dotfiles manually. No repair was attempted.")));
            Err(())
        }
    }
}

pub(super) fn availability(report: &mut Report, env: &SlateEnv, target: &str) {
    let presence = detect_tool_presence_with_env(target, env);
    let path = match &presence.evidence {
        Some(ToolEvidence::Executable(path)) => path.as_path(),
        _ => Path::new(""),
    };
    report.add_code("availability", if presence.is_tier1() { "ok" } else { "warning" },
        if presence.is_tier1() { "Executable found on PATH; not launched or version-checked" }
        else if presence.installed { "Executable found only in a fallback location, not on PATH; not launched" }
        else { "No executable detected; configuration can still be inspected" }, path,
        (!presence.is_tier1()).then(|| "Review installation and PATH if the tool cannot be started from your shell. This check does not install anything.".into()));
}

pub(super) fn saved_theme(report: &mut Report, env: &SlateEnv) -> Option<ThemeVariant> {
    let path = env.managed_file("current");
    let content = read(
        report,
        env,
        &path,
        MAX_STATE_BYTES,
        "theme_file",
        "Saved theme record",
    )
    .ok()?;
    let theme = content
        .as_deref()
        .and_then(|id| ThemeRegistry::new().ok()?.get(id.trim()).cloned());
    report.add_code(
        "saved_theme",
        if theme.is_some() { "ok" } else { "warning" },
        if theme.is_some() {
            "Saved theme is recognized; palette comparison is available"
        } else {
            "Saved theme is absent or unknown; no default palette is assumed"
        },
        &path,
        theme.is_none().then(|| {
            "Choose and save a theme with `slate theme` before comparing generated colors.".into()
        }),
    );
    theme
}

pub(super) fn parse_toml(
    report: &mut Report,
    path: &Path,
    content: &str,
    code: &'static str,
) -> Option<toml::Value> {
    match content.parse() {
        Ok(doc) => Some(doc),
        Err(_) => {
            report.add_code(code, "error", "Invalid TOML; file contents and parser excerpts omitted", path,
                Some("Review the file locally before changing its settings. No repair was attempted.".into()));
            None
        }
    }
}

/// Ownership and byte-for-byte generation are separate from selection/live state.
pub(super) fn generated_asset(
    report: &mut Report,
    env: &SlateEnv,
    target: &str,
    path: &Path,
    codes: [&'static str; 3],
    owns: fn(&[u8]) -> bool,
    expected: Option<&str>,
) {
    let Ok(content) = read(
        report,
        env,
        path,
        file_read::MAX_TOOL_CONFIG_BYTES,
        codes[0],
        "Generated theme asset",
    ) else {
        return;
    };
    let Some(content) = content else {
        report.add_code(
            codes[1],
            "warning",
            "Generated asset is absent; a theme selection alone cannot load its colors",
            path,
            None,
        );
        return;
    };
    let owned = owns(content.as_bytes());
    report.add_code(
        codes[1],
        if owned { "ok" } else { "warning" },
        if owned {
            "Slate's generated-file marker is present; this is not an authenticity check"
        } else {
            "The same-named asset lacks Slate's marker; synchronization will not overwrite it"
        },
        path,
        (!owned).then(|| {
            "Preserve and review this file manually before connecting. Nothing was replaced.".into()
        }),
    );
    if owned {
        if let Some(expected) = expected {
            let matches = content == expected;
            report.add_code(codes[2], if matches { "ok" } else { "warning" },
                if matches { "Asset exactly matches the generated file for the saved theme" }
                else { "Asset differs from the generated file for the saved theme; comments or formatting can also cause a difference" }, path,
                (!matches).then(|| format!("Review `slate tools sync {target} --dry-run` before regenerating. No repair was attempted.")));
        }
    }
}
