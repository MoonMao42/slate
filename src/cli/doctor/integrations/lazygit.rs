//! No shell sourcing, YAML execution or native Git/Lazygit invocation.
use super::{tool_files, Report};
use crate::{
    adapter::LazygitAdapter,
    config::{file_read::MAX_TOOL_CONFIG_BYTES, recovery_paths},
    env::SlateEnv,
};
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
};

pub(super) fn inspect(report: &mut Report, env: &SlateEnv) {
    report.scope = "Read-only Lazygit file and captured environment checks; no tool is launched or configuration changed. The saved theme record (4 KiB limit) and managed fragment (8 MiB limit) are read. Up to 32 selected paths are inspected through metadata; personal YAML, merge results, startup scripts, CLI overrides, native compatibility and running appearance are not evaluated. Final symlinks and relative paths are left for manual review. Files are observed separately, not atomically.";
    tool_files::availability(report, env, "lazygit");
    let theme = tool_files::saved_theme(report, env);
    let managed = LazygitAdapter::theme_path(env);
    if let Ok(Some(content)) = tool_files::read(
        report,
        env,
        &managed,
        MAX_TOOL_CONFIG_BYTES,
        "fragment_file",
        "Slate Lazygit GUI fragment",
    ) {
        if let Some(theme) = theme {
            match LazygitAdapter.generate_yaml_config(&theme) {
                Ok(expected) => {
                    let same = content == expected;
                    report.add_code("palette_match", if same { "ok" } else { "warning" },
                        if same { "Fragment exactly matches the generated palette for the saved theme; this does not prove it is loaded" }
                        else { "Fragment differs from current generated colors; old scalar YAML, edits or formatting may explain the difference" }, &managed,
                        (!same).then(|| "Review `slate tools sync lazygit --dry-run`. Regenerating colors does not repair older shell integration.".into()));
                }
                Err(_) => report.add_code(
                    "palette_match",
                    "error",
                    "Comparison palette could not be generated; no match is claimed",
                    &managed,
                    None,
                ),
            }
        }
    }
    selection(report, env, &managed);
    report.add_code("shell_startup", "info", "Startup scripts and the caller's running shell were not evaluated", &managed,
        Some("First open a fresh terminal and run `slate doctor lazygit` from the shell used to launch Lazygit; this process may have inherited an older environment. If the selection is still missing or legacy, review `slate setup` for shell integration. `tools sync lazygit` changes only the palette fragment.".into()));
    report.add_code("reload", "info", "Live Lazygit colors remain unverified", &managed,
        Some("After applying, reopen Lazygit from the updated shell. Personal colors or explicit --use-config-file options may override Slate; no Git operations were run.".into()));
}

fn selection(report: &mut Report, env: &SlateEnv, managed: &std::path::Path) {
    if env.session().is_isolated() {
        report.add_code(
            "isolated_profile",
            "info",
            "Isolated profile: host LG_CONFIG_FILE is ignored",
            managed,
            None,
        );
    }
    let Some(value) = env.lazygit_config_selection() else {
        report.add_code("environment_selection", "warning", "No nonempty LG_CONFIG_FILE was captured; Slate's managed fragment is not selected by this environment", managed,
            Some("Lazygit uses its native default unless command-line options override it. A generated fragment alone is not a connection.".into()));
        report.add_code(
            "native_default",
            "info",
            "Native default config path for the captured profile; its YAML is not evaluated",
            env.lazygit_default_config(),
            None,
        );
        return;
    };
    let mut legacy = managed.as_os_str().to_owned();
    legacy.push(":");
    legacy.push(env.xdg_config_home().join("lazygit/config.yml"));
    if value == legacy {
        report.add_code("environment_selection", "error", "Captured the exact legacy Slate colon-separated value; Lazygit requires comma-separated config files", managed,
            Some("First open a fresh terminal and rerun `slate doctor lazygit`; saved startup files may already be updated while this process retains the old environment. If the legacy value persists, review `slate setup` for shell integration. Do not replace arbitrary colons in personal filenames.".into()));
        return;
    }
    let bytes = value.as_bytes();
    if bytes.len() > 16 * 1024 || bytes.split(|b| *b == b',').count() > 32 {
        report.add_code("environment_selection", "warning", "Config selection exceeds the 16 KiB/32-path inspection limit; no connection is claimed", managed, None);
        return;
    }
    let paths = bytes
        .split(|b| *b == b',')
        .map(|part| PathBuf::from(OsString::from_vec(part.to_vec())))
        .collect::<Vec<_>>();
    let contains_managed = paths.iter().any(|path| path == managed);
    report.add_code("environment_selection", if contains_managed { "info" } else { "warning" },
        if contains_managed { "Captured comma list contains the exact managed path; existence and personal overrides are separate checks" }
        else { "Captured custom config selection does not name Slate's exact managed path; aliases are not resolved" }, managed,
        (!contains_managed).then(|| "Keep custom LG_CONFIG_FILE if intentional. Slate does not replace it; review it locally before expecting synchronized colors.".into()));
    for path in &paths {
        if !path.is_absolute() {
            report.add_code("selected_file", "warning", "An empty or relative config entry cannot be checked without assuming a launch directory", managed, None);
            continue;
        }
        if recovery_paths::validate_file_path(env, path, "Lazygit config selection").is_err() {
            report.add_code("selected_file", "warning", "Selected path cannot be safely inspected; linked or nonregular files require manual review", path, None);
            continue;
        }
        match std::fs::metadata(path) {
            Ok(meta) if meta.is_file() => report.add_code("selected_file", "info", "Selected regular file exists; readability and YAML validity have not been checked", path, None),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => report.add_code("selected_file", "error", "Selected config file is missing; Lazygit can reject the entire list", path,
                Some("Review the captured LG_CONFIG_FILE in the launch shell. Slate did not create or remove any listed files.".into())),
            _ => report.add_code("selected_file", "warning", "Cannot inspect selected config file metadata", path, None),
        }
    }
    if contains_managed && paths.len() > 1 {
        report.add_code(
            "personal_overrides",
            "info",
            "Multiple configs are selected; personal settings and merge results are not evaluated",
            managed,
            None,
        );
    }
}
