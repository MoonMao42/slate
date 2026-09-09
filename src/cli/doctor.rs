use crate::adapter::GhosttyAdapter;
use crate::detection::{self, ToolEvidence};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

mod auto_theme;
use crate::adapter::ghostty::references as ghostty_references;
mod ghostty_menu;
mod ghostty_scan;
mod ghostty_validation;
mod ghostty_window_style;
mod integrations;

/// Public targets also used by static completion generation.
pub const TARGETS: [&str; 18] = [
    "ghostty",
    "kitty",
    "alacritty",
    "nvim",
    "zsh",
    "bash",
    "fish",
    "opencode",
    "btop",
    "starship",
    "yazi",
    "zellij",
    "lazygit",
    "eza",
    "fastfetch",
    "opacity",
    "font",
    "auto-theme",
];

/// Restricted menu checks, shared by the catalog, detail pages and routing.
pub(crate) const TOOL_FILE_CHECKS: [(&str, &str, &str); 10] = [
    (
        "fastfetch",
        "Fastfetch",
        "预设文件与已保存主题 · 不检查启动效果",
    ),
    ("btop", "btop", "主题引用与生成配色"),
    ("eza", "eza", "生成配色、配置目录与环境覆盖"),
    (
        "starship",
        "Starship Prompt",
        "配置选择、配色、提示符样式与启用设置",
    ),
    ("yazi", "Yazi", "主题引用、界面与语法配色、个人覆盖"),
    ("zellij", "Zellij", "主题槽位、生成配色与同名冲突"),
    ("lazygit", "Lazygit", "生成配色与配置文件选择"),
    (
        "ghostty",
        "Ghostty",
        "配置引用、循环与标题栏覆盖 · 不启动原生校验",
    ),
    (
        "kitty",
        "Kitty",
        "直接主题引用与远程控制设置 · 不启动 Kitty",
    ),
    (
        "alacritty",
        "Alacritty",
        "有效导入列表与 TOML 语法 · 不启动 Alacritty",
    ),
];

pub(super) fn tool_file_check_hint(id: &str, chinese: &'static str) -> &'static str {
    super::ui_language::tr(
        chinese,
        match id {
            "fastfetch" => "Preset files and saved theme; startup not checked",
            "btop" => "Theme reference and generated colors",
            "eza" => "Generated colors, config directory and environment overrides",
            "starship" => "Config selection, colors, layout and activation preference",
            "yazi" => "Theme reference, UI and syntax colors, personal overrides",
            "zellij" => "Theme selection, generated colors and name conflicts",
            "lazygit" => "Generated colors and config selection",
            "ghostty" => "Config references, cycles and window overrides; no native validation",
            "kitty" => "Direct theme reference and remote-control settings; Kitty not launched",
            "alacritty" => "Effective imports and TOML syntax; Alacritty not launched",
            _ => "Read-only file check",
        },
    )
}

pub(crate) fn has_tool_file_check(target: &str) -> bool {
    TOOL_FILE_CHECKS.iter().any(|(id, _, _)| *id == target)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GhosttyConfigEntry {
    label: &'static str,
    path: PathBuf,
    exists: bool,
    slate_refs: Vec<String>,
    selected: bool,
    load_order_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GhosttyDoctorReport {
    entries: Vec<GhosttyConfigEntry>,
    duplicate_refs: Vec<(String, Vec<PathBuf>)>,
    config_file_cycles: Vec<Vec<PathBuf>>,
    selected_reason: String,
    validation: GhosttyValidation,
    scan_issues: Vec<ghostty_scan::Issue>,
    window_style_overrides: Vec<ghostty_window_style::Assignment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GhosttyValidation {
    Passed {
        binary: PathBuf,
    },
    Failed {
        binary: PathBuf,
        output: String,
    },
    TimedOut {
        binary: PathBuf,
    },
    OutputLimit {
        binary: PathBuf,
        output: String,
    },
    Unavailable {
        binary: PathBuf,
        reason: &'static str,
    },
    Skipped(&'static str),
}

impl GhosttyValidation {
    fn binary(&self) -> Option<&Path> {
        match self {
            Self::Passed { binary }
            | Self::Failed { binary, .. }
            | Self::TimedOut { binary }
            | Self::OutputLimit { binary, .. }
            | Self::Unavailable { binary, .. } => Some(binary),
            Self::Skipped(_) => None,
        }
    }

    fn status(&self) -> &'static str {
        match self {
            Self::Passed { .. } => "passed",
            Self::Failed { .. } => "failed",
            Self::TimedOut { .. } => "timed_out",
            Self::OutputLimit { .. } => "output_limit",
            Self::Unavailable { .. } => "error",
            Self::Skipped(_) => "skipped",
        }
    }

    fn message(&self) -> Option<&str> {
        match self {
            Self::Passed { .. } => None,
            Self::Failed { output, .. } | Self::OutputLimit { output, .. } => Some(output),
            Self::TimedOut { .. } => Some("Validator timed out; this invocation was terminated"),
            Self::Unavailable { reason, .. } | Self::Skipped(reason) => Some(reason),
        }
    }
}

const GHOSTTY_SCOPE: &str = "Best-effort scan of literal config-file references, not a complete Ghostty parser or proof of live appearance. Candidate order and reference syntax follow Ghostty 1.3.1, not an installed-version probe. Selection names Slate's last-existing write target, with XDG config.ghostty as its fallback; native preferred-file checks and process overrides are not modeled. Missing required files, overlong lines and unmodeled cross-file list resets make the scan incomplete. Reads are limited to 8 MiB/file, 32 MiB total, 256 paths, 4096 references and 64 include levels. Ordinary config symlinks are supported; references resolving outside an isolated profile are reported instead of read. Files are observed separately, not atomically; external directory changes are not locked out. Slate does not modify settings; native validation runs Ghostty itself and its bounded output may contain configuration values. An incomplete scan skips native validation.";

pub fn handle(target: Option<&str>, json: bool) -> Result<()> {
    handle_with_version_check(target, json, false)
}

pub(super) fn handle_auto_theme_with_env(env: &SlateEnv, json: bool) -> Result<()> {
    auto_theme::handle(env, json)
}

pub(super) fn handle_auto_theme_menu(env: &SlateEnv) -> Result<()> {
    auto_theme::handle_menu(env)
}

/// Preserve file-only Neovim diagnostics unless native checking is requested.
pub fn handle_with_version_check(
    target: Option<&str>,
    json: bool,
    check_version: bool,
) -> Result<()> {
    handle_with_options(target, json, check_version, false)
}

pub fn handle_with_options(
    target: Option<&str>,
    json: bool,
    check_version: bool,
    files_only: bool,
) -> Result<()> {
    if files_only && (target.unwrap_or("ghostty") != "ghostty" || check_version) {
        return Err(SlateError::InvalidConfig(
            "--files-only is supported only for Ghostty, without --check-version; no native check was run".into(),
        ));
    }
    if check_version && target != Some("nvim") {
        return Err(SlateError::InvalidConfig(
            "--check-version is only supported with `slate doctor nvim`; no native check was run"
                .into(),
        ));
    }
    match target.unwrap_or("ghostty") {
        "auto-theme" => handle_auto_theme_with_env(&SlateEnv::from_process()?, json),
        "ghostty" => {
            let env = SlateEnv::from_process()?;
            let report = if files_only {
                build_ghostty_report_with_validator(&env, None)?
            } else {
                build_ghostty_report(&env)?
            };
            let output = if json {
                format!("{}\n", format_ghostty_report_json(&report)?)
            } else {
                format_ghostty_report(&report)
            };
            write_report(&output)
        }
        target @ ("kitty" | "alacritty" | "nvim" | "zsh" | "bash" | "fish" | "opencode"
        | "opacity" | "font" | "btop" | "starship" | "yazi" | "zellij" | "lazygit"
        | "eza" | "fastfetch") => {
            let env = SlateEnv::from_process()?;
            integrations::handle(target, &env, json, check_version)
        }
        other => Err(SlateError::InvalidConfig(format!(
            "Unknown doctor target '{}'. Choose {}.",
            other.escape_default(),
            TARGETS.join(", ")
        ))),
    }
}

/// Restricted menu route: never use Ghostty's native validator from this page.
pub(super) fn show_tool_files(env: &SlateEnv, target: &str, back: &str) -> Result<()> {
    // Prepare before entering the scratch screen so failures remain visible.
    let report = tool_file_report(env, target)?;
    let mut page = super::menu::ReadOnlyPage::enter()?;
    let title = format!(
        "{target} · {}",
        super::ui_language::tr("配置检查", "Configuration Check")
    );
    let result = page.view(&report, &title, back).map_err(|error| {
        if error.kind() == std::io::ErrorKind::Interrupted {
            SlateError::UserCancelled
        } else {
            SlateError::IOError(error)
        }
    });
    page.finish(result)
}

/// Build the same file-only observations for a scrollable read-only page.
pub(super) fn tool_file_report(env: &SlateEnv, target: &str) -> Result<String> {
    if target == "ghostty" {
        let report = build_ghostty_report_with_validator(env, None)?;
        return Ok(ghostty_menu::render(&report));
    }
    if has_tool_file_check(target) {
        Ok(integrations::menu_output(target, env))
    } else {
        Err(SlateError::InvalidConfig(
            "Unsupported file-only tool check".into(),
        ))
    }
}

fn write_report(output: &str) -> Result<()> {
    match std::io::stdout().lock().write_all(output.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn build_ghostty_report(env: &SlateEnv) -> Result<GhosttyDoctorReport> {
    build_ghostty_report_with_validator(env, Some(run_ghostty_validation))
}

fn build_ghostty_report_with_validator(
    env: &SlateEnv,
    validator: Option<fn(&SlateEnv) -> GhosttyValidation>,
) -> Result<GhosttyDoctorReport> {
    let adapter = GhosttyAdapter;
    let selected = adapter.integration_config_path_with_env(env)?;
    let candidates = GhosttyAdapter::config_candidates_with_env(env)?;
    let mut scan = ghostty_scan::Scan::new(env);

    let entries = candidates
        .into_iter()
        .enumerate()
        .map(|(idx, candidate)| {
            let path = candidate.path;
            let refs = scan.slate_refs(&path);
            GhosttyConfigEntry {
                label: candidate.label,
                exists: path.exists(),
                selected: path == selected,
                path,
                slate_refs: refs,
                load_order_index: idx,
            }
        })
        .collect::<Vec<_>>();

    let mut refs_to_paths: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for entry in &entries {
        for slate_ref in &entry.slate_refs {
            refs_to_paths
                .entry(slate_ref.clone())
                .or_default()
                .push(entry.path.clone());
        }
    }
    let duplicate_refs = refs_to_paths
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .collect();

    scan.visit_all(entries.iter().map(|entry| entry.path.clone()));
    let validation = match validator {
        None => GhosttyValidation::Skipped("file-only check; native validation was not requested"),
        Some(validate) if scan.issues.is_empty() => validate(env),
        Some(_) => GhosttyValidation::Skipped(
            "configuration scan incomplete; inspect scan issues before native validation",
        ),
    };
    Ok(GhosttyDoctorReport {
        config_file_cycles: scan.cycles,
        selected_reason: selected_entry_reason(&entries),
        entries,
        duplicate_refs,
        validation,
        scan_issues: scan.issues,
        window_style_overrides: scan.window_style_overrides.into_values().collect(),
    })
}

fn selected_entry_reason(entries: &[GhosttyConfigEntry]) -> String {
    let Some(selected) = entries.iter().find(|entry| entry.selected) else {
        return "no selected Ghostty config entry".to_string();
    };

    if selected.exists {
        format!(
            "{} is the last existing candidate in Ghostty 1.3.1 default-file order (Slate write target)",
            selected.label
        )
    } else {
        format!(
            "{} is Slate's default write target because no Ghostty config candidates exist yet",
            selected.label
        )
    }
}

fn run_ghostty_validation(env: &SlateEnv) -> GhosttyValidation {
    if env.session().is_isolated() {
        return GhosttyValidation::Skipped("isolated profile; skipping host Ghostty validation");
    }
    if std::env::var_os("HOME").is_some_and(|home| home != env.home().as_os_str()) {
        return GhosttyValidation::Skipped("non-process HOME; skipping host Ghostty validation");
    }

    let Some(binary) = ghostty_binary_path(env) else {
        return GhosttyValidation::Skipped("Ghostty CLI not found");
    };
    ghostty_validation::run(&binary, ghostty_validation::TIMEOUT)
}

fn ghostty_binary_path(env: &SlateEnv) -> Option<PathBuf> {
    match detection::detect_tool_presence_with_env("ghostty", env).evidence {
        Some(ToolEvidence::Executable(path)) if path.exists() => Some(path),
        Some(ToolEvidence::AppBundle(path)) => ghostty_app_binary(&path),
        _ => fallback_ghostty_binary_path(env),
    }
}

fn fallback_ghostty_binary_path(env: &SlateEnv) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        for app in [
            env.home().join("Applications/Ghostty.app"),
            PathBuf::from("/Applications/Ghostty.app"),
        ] {
            if let Some(binary) = ghostty_app_binary(&app) {
                return Some(binary);
            }
        }
    }

    detection::command_path_with_env("ghostty", env)
}

fn ghostty_app_binary(app_bundle: &Path) -> Option<PathBuf> {
    let binary = app_bundle.join("Contents/MacOS/ghostty");
    binary.exists().then_some(binary)
}

fn normalize_config_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn format_ghostty_report(report: &GhosttyDoctorReport) -> String {
    let mut out = String::new();
    out.push_str("◆ Ghostty doctor\n");

    let selected = report
        .entries
        .iter()
        .find(|entry| entry.selected)
        .map(|entry| terminal_path(&entry.path))
        .unwrap_or_else(|| "(none)".to_string());
    out.push_str(&format!("selected entry: {selected}\n"));
    out.push_str(&format!("selected reason: {}\n", report.selected_reason));
    out.push_str("candidate entries:\n");

    for entry in &report.entries {
        let status = if entry.exists { "present" } else { "missing" };
        let selected = if entry.selected { " selected" } else { "" };
        out.push_str(&format!(
            "  - {}: {}{} load-order={} refs={}\n",
            entry.label,
            status,
            selected,
            entry.load_order_index + 1,
            entry.slate_refs.len()
        ));
        out.push_str(&format!("    {}\n", terminal_path(&entry.path)));
    }

    if report.duplicate_refs.is_empty() && report.config_file_cycles.is_empty() {
        if report.scan_issues.is_empty() {
            out.push_str("cycle risk: none from inspected duplicate Slate-managed refs or config-file cycles\n");
        } else {
            out.push_str("cycle risk: unknown; configuration scan is incomplete\n");
        }
    } else {
        out.push_str("cycle risk: Ghostty config-file issues detected\n");
        if !report.duplicate_refs.is_empty() {
            out.push_str("duplicate Slate-managed refs:\n");
            for (slate_ref, paths) in &report.duplicate_refs {
                out.push_str(&format!("  - {}\n", integrations::terminal_text(slate_ref)));
                for path in paths {
                    out.push_str(&format!("    in {}\n", terminal_path(path)));
                }
            }
        }
        if !report.config_file_cycles.is_empty() {
            out.push_str("config-file cycles:\n");
            for cycle in &report.config_file_cycles {
                let rendered = cycle
                    .iter()
                    .map(|path| terminal_path(path))
                    .collect::<Vec<_>>()
                    .join(" -> ");
                out.push_str(&format!("  - {rendered}\n"));
            }
        }
        out.push_str("fix: remove the repeated config-file edge, run `slate theme <current-theme>` to rebuild Slate refs in one entry, or `slate clean` to remove Slate refs.\n");
    }

    if !report.scan_issues.is_empty() {
        out.push_str("scan incomplete:\n");
        for issue in &report.scan_issues {
            out.push_str(&format!(
                "  - {}: {}\n    {}{}\n",
                issue.code,
                integrations::terminal_text(&issue.message),
                integrations::terminal_text(&issue.path),
                if issue.path_is_lossy {
                    " (lossy display; not an exact path)"
                } else {
                    ""
                }
            ));
        }
    }
    out.push_str(&ghostty_window_style::format(
        &report.window_style_overrides,
        report.scan_issues.is_empty(),
    ));
    out.push_str(&format_ghostty_validation(&report.validation));
    out.push_str(GHOSTTY_SCOPE);
    out.push('\n');

    out
}

fn terminal_path(path: &Path) -> String {
    let mut text = integrations::terminal_text(&path.display().to_string());
    if path.to_str().is_none() {
        text.push_str(" (lossy display; not an exact path)");
    }
    text
}

fn format_ghostty_validation(validation: &GhosttyValidation) -> String {
    let status = if matches!(validation, GhosttyValidation::Passed { .. }) {
        "ok"
    } else {
        validation.status()
    };
    let mut text = format!("ghostty validate: {status}");
    if let Some(binary) = validation.binary() {
        text.push_str(&format!(" ({})", terminal_path(binary)));
    }
    text.push('\n');
    if let Some(message) = validation.message() {
        for line in message
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(8)
        {
            text.push_str(&integrations::terminal_text(line));
            text.push('\n');
        }
    }
    if matches!(validation, GhosttyValidation::OutputLimit { .. }) {
        text.push_str(
            "Validator exceeded 64 KiB of output; invocation terminated and output truncated.\n",
        );
    }
    text
}

fn format_ghostty_report_json(report: &GhosttyDoctorReport) -> Result<String> {
    let selected_entry = report
        .entries
        .iter()
        .find(|entry| entry.selected)
        .map(|entry| entry.path.display().to_string());
    let entries = report
        .entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "label": entry.label,
                "path": entry.path.display().to_string(),
                "path_is_lossy": entry.path.to_str().is_none(),
                "exists": entry.exists,
                "selected": entry.selected,
                "load_order_index": entry.load_order_index,
                "slate_ref_count": entry.slate_refs.len(),
                "slate_refs": entry.slate_refs,
            })
        })
        .collect::<Vec<_>>();
    let duplicate_refs = report
        .duplicate_refs
        .iter()
        .map(|(slate_ref, paths)| {
            serde_json::json!({
                "slate_ref": slate_ref,
                "paths": paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let config_file_cycles = report
        .config_file_cycles
        .iter()
        .map(|cycle| {
            cycle
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let validation = serde_json::json!({
        "status": report.validation.status(),
        "message": report.validation.message(),
        "binary": report.validation.binary().map(|path| path.display().to_string()),
        "binary_path_is_lossy": report.validation.binary().is_some_and(|path| path.to_str().is_none()),
        "output_truncated": matches!(report.validation, GhosttyValidation::OutputLimit { .. }),
        "timeout_ms": ghostty_validation::TIMEOUT.as_millis(),
        "output_limit_bytes": ghostty_validation::MAX_OUTPUT,
    });
    let paths_are_lossy = report
        .entries
        .iter()
        .any(|entry| entry.path.to_str().is_none())
        || report
            .duplicate_refs
            .iter()
            .flat_map(|(_, paths)| paths)
            .any(|path| path.to_str().is_none())
        || report
            .config_file_cycles
            .iter()
            .flatten()
            .any(|path| path.to_str().is_none())
        || report.scan_issues.iter().any(|issue| issue.path_is_lossy)
        || report
            .window_style_overrides
            .iter()
            .any(|assignment| assignment.path_is_lossy)
        || report
            .validation
            .binary()
            .is_some_and(|path| path.to_str().is_none());

    serde_json::to_string_pretty(&serde_json::json!({
        "target": "ghostty",
        "schema_version": 1,
        "reference_syntax": "ghostty-1.3.1",
        "entry_order": "ghostty-1.3.1-defaults",
        "scope": GHOSTTY_SCOPE,
        "scan_complete": report.scan_issues.is_empty(),
        "scan_issues": report.scan_issues,
        "window_style": ghostty_window_style::json(&report.window_style_overrides, report.scan_issues.is_empty()),
        "paths_are_lossy": paths_are_lossy,
        "selected_entry": selected_entry,
        "selected_reason": report.selected_reason,
        "entries": entries,
        "cycle_risk": !report.duplicate_refs.is_empty() || !report.config_file_cycles.is_empty(),
        "duplicate_refs": duplicate_refs,
        "config_file_cycles": config_file_cycles,
        "validation": validation,
    }))
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ghostty_doctor_detects_duplicate_slate_refs_across_entries() {
        let td = TempDir::new().unwrap();
        let home = td.path().join("home with spaces");
        fs::create_dir_all(&home).unwrap();
        let env = SlateEnv::with_home(home);
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        let managed = env.config_dir().join("managed/ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        fs::write(
            ghostty_dir.join("config.ghostty"),
            format!("config-file = \"{}/theme.conf\"\n", managed.display()),
        )
        .unwrap();
        fs::write(
            ghostty_dir.join("config"),
            format!("include = \"{}/theme.conf\"\n", managed.display()),
        )
        .unwrap();

        let report = build_ghostty_report(&env).unwrap();

        assert_eq!(report.duplicate_refs.len(), 1);
        assert!(report.config_file_cycles.is_empty());
        assert_eq!(
            report.duplicate_refs[0].0,
            format!("{}/theme.conf", managed.display())
        );
        assert!(format_ghostty_report(&report).contains("duplicate Slate-managed refs"));
        let file_only = build_ghostty_report_with_validator(&env, None).unwrap();
        assert_eq!(file_only.duplicate_refs, report.duplicate_refs);
        assert_eq!(file_only.config_file_cycles, report.config_file_cycles);
        assert!(matches!(
            file_only.validation,
            GhosttyValidation::Skipped("file-only check; native validation was not requested")
        ));
    }

    #[test]
    fn ghostty_doctor_ignores_user_refs_and_comments() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        let managed = env.config_dir().join("managed/ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        fs::write(
            ghostty_dir.join("config.ghostty"),
            format!(
                "# config-file = \"{}/theme.conf\"\nconfig-file = \"/tmp/user/theme.conf\"\nconfig-file = \"{}-old/theme.conf\"\nconfig-file = \"/tmp{}/theme.conf\"\n",
                managed.display(),
                managed.display(),
                managed.display()
            ),
        )
        .unwrap();

        let report = build_ghostty_report(&env).unwrap();

        assert!(report.duplicate_refs.is_empty());
        assert!(report
            .entries
            .iter()
            .all(|entry| entry.slate_refs.is_empty()));
    }

    #[test]
    fn ghostty_doctor_explains_selected_entry_by_load_order() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        fs::write(ghostty_dir.join("config"), "# first\n").unwrap();
        fs::write(ghostty_dir.join("config.ghostty"), "# later\n").unwrap();

        let report = build_ghostty_report(&env).unwrap();
        let output = format_ghostty_report(&report);

        assert_eq!(
            report.selected_reason,
            "XDG config.ghostty is the last existing candidate in Ghostty 1.3.1 default-file order (Slate write target)"
        );
        assert!(
            output.contains("selected reason: XDG config.ghostty is the last existing candidate")
        );
        assert!(output.contains("load-order=1"));
        assert!(output.contains("load-order=2"));
    }

    #[test]
    fn ghostty_doctor_detects_plain_config_file_cycles() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();

        let entry = ghostty_dir.join("config.ghostty");
        let nested = ghostty_dir.join("nested.conf");
        fs::write(&entry, "config-file = \"nested.conf\"\n").unwrap();
        fs::write(&nested, format!("config-file = \"{}\"\n", entry.display())).unwrap();

        let report = build_ghostty_report(&env).unwrap();
        let output = format_ghostty_report(&report);
        let json: serde_json::Value =
            serde_json::from_str(&format_ghostty_report_json(&report).unwrap()).unwrap();

        assert_eq!(report.config_file_cycles.len(), 1);
        assert!(output.contains("config-file cycles"));
        assert_eq!(json["cycle_risk"], true);
        assert_eq!(json["config_file_cycles"][0].as_array().unwrap().len(), 3);
    }

    #[cfg(unix)]
    #[test]
    fn ghostty_doctor_detects_config_file_cycles_through_symlinks() {
        use std::os::unix::fs::symlink;

        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let ghostty_dir = env.xdg_config_home().join("ghostty");
        let dotfiles_dir = td.path().join("dotfiles/ghostty");
        fs::create_dir_all(&ghostty_dir).unwrap();
        fs::create_dir_all(&dotfiles_dir).unwrap();

        let entry = ghostty_dir.join("config.ghostty");
        let linked_nested = ghostty_dir.join("linked.conf");
        let real_nested = dotfiles_dir.join("nested.conf");
        fs::write(&entry, "config-file = \"linked.conf\"\n").unwrap();
        fs::write(
            &real_nested,
            format!("config-file = \"{}\"\n", entry.display()),
        )
        .unwrap();
        symlink(&real_nested, &linked_nested).unwrap();

        let report = build_ghostty_report(&env).unwrap();
        assert_eq!(report.config_file_cycles.len(), 1);

        let rendered_cycle = report.config_file_cycles[0]
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");

        assert!(rendered_cycle.contains(&entry.canonicalize().unwrap().display().to_string()));
        assert!(rendered_cycle.contains(&real_nested.canonicalize().unwrap().display().to_string()));
    }

    #[test]
    fn ghostty_doctor_formats_healthy_report() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());

        let report = build_ghostty_report(&env).unwrap();
        let output = format_ghostty_report(&report);

        assert!(output.contains("◆ Ghostty doctor"));
        assert!(output.contains("cycle risk: none"));
        assert!(output.contains("ghostty validate: skipped"));
    }

    #[test]
    fn ghostty_validation_formatter_limits_failed_output() {
        let validation = GhosttyValidation::Failed {
            binary: PathBuf::from("/Applications/Ghostty.app/Contents/MacOS/ghostty"),
            output: (1..=12)
                .map(|idx| format!("error line {idx}"))
                .collect::<Vec<_>>()
                .join("\n"),
        };

        let output = format_ghostty_validation(&validation);

        assert!(output.contains("ghostty validate: failed"));
        assert!(output.contains("/Applications/Ghostty.app/Contents/MacOS/ghostty"));
        assert!(output.contains("error line 8"));
        assert!(!output.contains("error line 9"));
    }

    #[test]
    // SWATCH-RENDERER: hostile path bytes in this JSON serialization fixture are intentional.
    fn ghostty_report_json_is_machine_readable() {
        let mut report = GhosttyDoctorReport {
            entries: vec![GhosttyConfigEntry {
                label: "XDG config.ghostty",
                path: PathBuf::from("/tmp/ghostty/config.ghostty"),
                exists: true,
                slate_refs: vec!["/tmp/slate/managed/ghostty/theme.conf".to_string()],
                selected: true,
                load_order_index: 0,
            }],
            duplicate_refs: Vec::new(),
            config_file_cycles: Vec::new(),
            scan_issues: Vec::new(),
            window_style_overrides: Vec::new(),
            selected_reason: "XDG config.ghostty is the last existing candidate in Ghostty 1.3.1 default-file order (Slate write target)"
                .to_string(),
            validation: GhosttyValidation::Passed {
                binary: PathBuf::from("/Applications/Ghostty.app/Contents/MacOS/ghostty"),
            },
        };

        let json = format_ghostty_report_json(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["target"], "ghostty");
        assert_eq!(parsed["cycle_risk"], false);
        assert_eq!(parsed["selected_reason"], report.selected_reason);
        assert_eq!(parsed["entry_order"], "ghostty-1.3.1-defaults");
        assert_eq!(parsed["config_file_cycles"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["validation"]["status"], "passed");
        assert_eq!(
            parsed["validation"]["binary"],
            "/Applications/Ghostty.app/Contents/MacOS/ghostty"
        );
        assert_eq!(parsed["entries"][0]["load_order_index"], 0);
        assert_eq!(parsed["entries"][0]["slate_ref_count"], 1);
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["scan_complete"], true);
        assert_eq!(parsed["paths_are_lossy"], false);
        use std::os::unix::ffi::OsStringExt;
        report.entries[0].path = PathBuf::from(std::ffi::OsString::from_vec(
            b"/tmp/config-\xff\x1b[31m".to_vec(),
        ));
        let parsed: serde_json::Value =
            serde_json::from_str(&format_ghostty_report_json(&report).unwrap()).unwrap();
        assert_eq!(parsed["paths_are_lossy"], true);
        assert_eq!(parsed["entries"][0]["path_is_lossy"], true);
        let text = format_ghostty_report(&report);
        assert!(text.contains("lossy display; not an exact path"));
        assert!(!text.contains('\u{1b}'));

        // A nested finding can be the only lossy path in an otherwise ordinary
        // report. Keep the aggregate flag and terminal escaping accurate too.
        report.window_style_overrides = vec![ghostty_window_style::Assignment::new(
            &report.entries[0].path,
            3,
        )];
        report.entries[0].path = PathBuf::from("/tmp/ghostty/config.ghostty");
        let parsed: serde_json::Value =
            serde_json::from_str(&format_ghostty_report_json(&report).unwrap()).unwrap();
        assert_eq!(parsed["paths_are_lossy"], true);
        assert_eq!(
            parsed["window_style"]["managed_overrides"][0]["path_is_lossy"],
            true
        );
        assert_eq!(
            parsed["window_style"]["managed_overrides"][0]["first_assignment_line"],
            3
        );
        assert_eq!(parsed["window_style"]["status"], "managed_override");
        let text = format_ghostty_report(&report);
        assert!(text.contains("lossy display; not an exact path"));
        assert!(!text.contains('\u{1b}'));
    }

    #[test]
    fn ghostty_binary_path_uses_user_app_bundle() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let binary = env
            .home()
            .join("Applications/Ghostty.app/Contents/MacOS/ghostty");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(&binary, "").unwrap();

        assert_eq!(ghostty_binary_path(&env), Some(binary));
    }
}
