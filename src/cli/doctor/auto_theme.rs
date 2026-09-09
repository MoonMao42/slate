use crate::cli::auto_theme_resolution as resolution;
use crate::config::{pairing, ConfigManager};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::platform::dark_mode_notify::{
    inspect_installation, InstallationInspection, InstallationState, RuntimeInspection,
    RuntimeState,
};
use crate::theme::ThemeRegistry;
use serde::Serialize;

mod output;

#[derive(Serialize)]
struct Report {
    schema_version: u8,
    target: &'static str,
    auto_theme_enabled: Option<bool>,
    isolated: bool,
    #[serde(serialize_with = "output::runtime")]
    runtime: RuntimeInspection,
    #[serde(serialize_with = "output::installation")]
    installation: InstallationInspection,
    resolution: resolution::Report,
    issues: Vec<&'static str>,
    next_steps: Vec<&'static str>,
    scope: &'static str,
}

fn inspect(env: &SlateEnv) -> Result<Report> {
    let registry = ThemeRegistry::new()?;
    let pairing = pairing::inspect(env).ok();
    let resolution = resolution::inspect(env, &registry, pairing.as_ref());
    let enabled = ConfigManager::from_env_paths(env)
        .inspect_auto_theme_enabled()
        .ok();
    let runtime = RuntimeInspection::inspect(env);
    let installation = inspect_installation(env);
    let mut issues = Vec::new();
    let mut next_steps = Vec::new();
    if matches!(
        installation.directory_access,
        crate::platform::dark_mode_notify::DirectoryAccess::Denied
            | crate::platform::dark_mode_notify::DirectoryAccess::NotDirectory
    ) {
        issues.push("The managed helper directory is not writable/searchable or is not a directory; enabling auto-theme may fail before saving the preference.");
        next_steps.push("Inspect the reported helper directory's type, ownership, permissions and ACL rules. No permissions were changed and no write probe was performed; do not use a recursive permission reset.");
    }
    if resolution.has_errors() {
        issues.push("Conditional automatic selection cannot resolve one or both appearances; watcher readiness does not prove theme selection is usable.");
        next_steps.push("Inspect `slate config pairing` for per-appearance details; review auto.toml and any required current tracking file locally.");
        next_steps.push("For a valid pairing document, preview replacement IDs with `slate config pairing --dark <ID> --light <ID> --dry-run`, or remove selected overrides with --clear-dark/--clear-light. Neither operation starts the watcher or applies a theme.");
    }
    if enabled.is_none() {
        issues.push("The saved auto-theme preference cannot be read; no default is inferred.");
        next_steps.push("Review config.toml syntax, [auto_theme].enabled, permissions, links, the 256 KiB limit and isolated-profile boundaries. Compare `slate config get auto-theme`; unreadable is not a disabled default.");
    }
    match installation.launcher.state {
        InstallationState::Current => {}
        InstallationState::Missing if enabled != Some(true) => {}
        InstallationState::Legacy | InstallationState::Unrecognized => {
            issues.push("The launcher is legacy or unrecognized; this does not prove a legacy process is running.");
            next_steps.push("Review the launcher and any known old watcher before refreshing. Untracked legacy processes are not adopted or stopped automatically.");
        }
        InstallationState::Missing | InstallationState::Outdated => {
            issues.push("The managed launcher is missing or differs from this binary/profile's generated launcher.");
            next_steps.push("Review local launcher edits, then use `slate config set auto-theme enable` to refresh this profile's bindings and launcher.");
        }
        InstallationState::Unsafe | InstallationState::Unreadable => {
            issues.push("The launcher is unsafe or unreadable; it was not executed.");
            next_steps.push("Inspect launcher type and permissions; preserve custom files before replacing managed artifacts.");
        }
    }
    if installation.launcher.executable == Some(false) {
        issues.push("The launcher has no execute permission.");
    }
    if enabled == Some(true)
        && installation.helper.as_ref().is_some_and(|helper| {
            helper.state != InstallationState::Current || helper.executable == Some(false)
        })
    {
        issues.push("The macOS appearance helper is missing, outdated, unsafe, or not executable.");
        next_steps.push("Refresh the helper with `slate config set auto-theme enable`; builds without the native helper require Xcode Command Line Tools and a rebuild.");
    }
    match runtime.state {
        RuntimeState::Unreadable => {
            issues.push("Watcher runtime/control files cannot be safely inspected.");
            next_steps.push("Check the runtime directory and regular private control files (directory 0700, files 0600); do not delete an active lock to bypass ownership.");
        }
        RuntimeState::Failed => {
            issues.push("The last managed watcher exited with a recorded failure.");
            next_steps.push("Inspect the reported private watcher.log locally, fix the cause, then retry `slate config set auto-theme enable`.");
        }
        RuntimeState::Stale => {
            issues.push("The previous instance has no matching completion record; it is not reported as running or as a confirmed crash.");
            next_steps.push("Inspect the private log if needed; a new managed start can reuse inactive control files without deleting them.");
        }
        RuntimeState::Starting | RuntimeState::Stopping | RuntimeState::Changing => {
            next_steps.push("Run `slate doctor auto-theme` again after the current transition; readiness is not inferred from a saved record alone.");
        }
        RuntimeState::Absent | RuntimeState::Stopped
            if enabled == Some(true) && !env.session().is_isolated() =>
        {
            issues.push(
                "Auto-theme is enabled, but no managed watcher is running for this profile/cache.",
            );
            next_steps.push("Use `slate config set auto-theme enable` to start this profile, keeping the same HOME/XDG environment.");
        }
        _ => {}
    }
    if env.session().is_isolated() {
        next_steps.push("SLATE_HOME disables desktop watcher startup; this inspection only reads the isolated profile.");
    }
    Ok(Report {
        schema_version: 1, target: "auto-theme", auto_theme_enabled: enabled, isolated: env.session().is_isolated(),
        runtime, installation, resolution, issues, next_steps,
        scope: "Read-only files and lifetime-lock inspection. No desktop probes, process enumeration, launcher execution, or log contents. Only this config/cache's managed runtime is reported. Conditional dark/light choices use the automatic application policy, not a desktop query, atomic configuration snapshot or apply-readiness check. Runtime and configuration are separate observations; ready does not prove either theme was applied.",
    })
}

pub(super) fn handle(env: &SlateEnv, json: bool) -> Result<()> {
    let report = inspect(env)?;
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&report)?)
    } else {
        output::text(&report)
    };
    super::write_report(&output)
}

pub(super) fn handle_menu(env: &SlateEnv) -> Result<()> {
    super::write_report(&output::menu_text(&inspect(env)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_theme_menu_is_compact_but_preserves_actionable_diagnostics() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let mut report = inspect(&env).unwrap();
        assert!(report.issues.is_empty());
        let compact = output::menu_text(&report);
        assert!(compact.contains("已保存设置：已关闭"));
        assert!(compact.contains("未发现运行实例"));
        assert!(compact.contains("隔离配置"));
        assert!(compact.lines().count() <= 8);
        assert!(!compact.contains("Helper directory:"));
        assert!(!home.path().join(".config").exists());
        report.issues.push("fixture actionable failure");
        report.next_steps.push("fixture recovery instruction");
        let detailed = output::menu_text(&report);
        assert!(detailed.contains("fixture actionable failure"));
        assert!(detailed.contains("fixture recovery instruction"));
        assert!(detailed.contains(report.scope));
    }

    #[test]
    #[ignore = "explicit disabled profile with denied helper-directory access; read-only inspection"]
    fn auto_theme_report_surfaces_denied_directory_even_when_disabled() {
        let home = std::env::var_os("SLATE_INSPECT_BLOCKED_HOME").expect("provide profile home");
        let env = SlateEnv::with_home(home.into());
        let report = inspect(&env).unwrap();
        assert_eq!(report.auto_theme_enabled, Some(false));
        assert_eq!(
            report.installation.directory_access,
            crate::platform::dark_mode_notify::DirectoryAccess::Denied
        );
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("helper directory")));
        assert!(report
            .next_steps
            .iter()
            .any(|step| step.contains("ACL rules")));
        let text = output::text(&report);
        assert!(text.contains("write/search access denied"));
        assert!(text.contains("Warning: The managed helper directory"));
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["auto_theme_enabled"], false);
        assert_eq!(json["installation"]["directory_access"], "denied");
        assert!(json["issues"]
            .as_array()
            .is_some_and(|issues| !issues.is_empty()));
    }
}
