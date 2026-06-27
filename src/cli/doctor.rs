use crate::adapter::GhosttyAdapter;
use crate::detection::{self, ToolEvidence};
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GhosttyValidation {
    Passed { binary: PathBuf },
    Failed { binary: PathBuf, output: String },
    Skipped(&'static str),
}

pub fn handle(target: Option<&str>, json: bool) -> Result<()> {
    match target.unwrap_or("ghostty") {
        "ghostty" => {
            let env = SlateEnv::from_process()?;
            let report = build_ghostty_report(&env)?;
            if json {
                println!("{}", format_ghostty_report_json(&report)?);
            } else {
                print!("{}", format_ghostty_report(&report));
            }
            Ok(())
        }
        other => Err(SlateError::InvalidConfig(format!(
            "Unknown doctor target '{}'. Try `slate doctor ghostty`.",
            other
        ))),
    }
}

fn ghostty_candidate_labels() -> Vec<&'static str> {
    let mut labels = vec!["XDG config.ghostty", "XDG config"];
    if cfg!(target_os = "macos") {
        labels.push("macOS App Support config.ghostty");
        labels.push("macOS App Support config");
    }
    labels
}

fn build_ghostty_report(env: &SlateEnv) -> Result<GhosttyDoctorReport> {
    let adapter = GhosttyAdapter;
    let selected = adapter.integration_config_path_with_env(env)?;
    let candidates = adapter.integration_candidate_paths_with_env(env)?;
    let managed_root = env.config_dir().join("managed").join("ghostty");

    let entries = ghostty_candidate_labels()
        .into_iter()
        .zip(candidates)
        .enumerate()
        .map(|(idx, (label, path))| {
            let refs = read_slate_refs(&path, &managed_root);
            GhosttyConfigEntry {
                label,
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

    Ok(GhosttyDoctorReport {
        config_file_cycles: detect_config_file_cycles(&entries, env.home()),
        selected_reason: selected_entry_reason(&entries),
        entries,
        duplicate_refs,
        validation: run_ghostty_validation(env),
    })
}

fn selected_entry_reason(entries: &[GhosttyConfigEntry]) -> String {
    let Some(selected) = entries.iter().find(|entry| entry.selected) else {
        return "no selected Ghostty config entry".to_string();
    };

    if selected.exists {
        format!(
            "{} is the last existing entry in Ghostty load order",
            selected.label
        )
    } else {
        format!(
            "{} is the default entry because no Ghostty config candidates exist yet",
            selected.label
        )
    }
}

fn run_ghostty_validation(env: &SlateEnv) -> GhosttyValidation {
    if std::env::var_os("SLATE_HOME").is_some() {
        return GhosttyValidation::Skipped("SLATE_HOME is set; skipping host Ghostty validation");
    }
    if std::env::var_os("HOME").is_some_and(|home| home != env.home().as_os_str()) {
        return GhosttyValidation::Skipped("non-process HOME; skipping host Ghostty validation");
    }

    let Some(binary) = ghostty_binary_path(env) else {
        return GhosttyValidation::Skipped("Ghostty CLI not found");
    };
    let output = match Command::new(&binary).arg("+validate-config").output() {
        Ok(output) => output,
        Err(_) => return GhosttyValidation::Skipped("Ghostty CLI not found"),
    };

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.success() && combined.trim().is_empty() {
        GhosttyValidation::Passed { binary }
    } else {
        GhosttyValidation::Failed {
            binary,
            output: combined,
        }
    }
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

fn read_slate_refs(path: &Path, managed_root: &Path) -> Vec<String> {
    let Ok(content) = fs::read(path) else {
        return Vec::new();
    };
    let managed = managed_root.display().to_string();
    let managed_bytes = managed.as_bytes();

    content
        .split(|b| *b == b'\n')
        .filter_map(|line| extract_slate_ref(line, managed_bytes))
        .collect()
}

fn extract_slate_ref(line: &[u8], managed_root: &[u8]) -> Option<String> {
    let trimmed = trim_ascii(line);
    if trimmed.starts_with(b"#") || trimmed.is_empty() {
        return None;
    }

    let key = line_key(trimmed);
    if key != b"config-file" && key != b"include" {
        return None;
    }

    let idx = find_managed_root_index(trimmed, managed_root)?;
    let path = &trimmed[idx..];
    let quote = trimmed[..idx]
        .iter()
        .rev()
        .find(|b| matches!(**b, b'"' | b'\''))
        .copied();
    let end = match quote {
        Some(quote) => path.iter().position(|b| *b == quote).unwrap_or(path.len()),
        None => path
            .iter()
            .position(|b| matches!(*b, b'\r' | b' ' | b'\t'))
            .unwrap_or(path.len()),
    };
    String::from_utf8(path[..end].to_vec()).ok()
}

fn find_managed_root_index(line: &[u8], managed_root: &[u8]) -> Option<usize> {
    if managed_root.is_empty() || line.len() < managed_root.len() {
        return None;
    }

    line.windows(managed_root.len())
        .enumerate()
        .find_map(|(idx, window)| {
            if window != managed_root {
                return None;
            }

            let previous = if idx == 0 {
                None
            } else {
                line.get(idx - 1).copied()
            };
            if !path_reference_starts_at_value_boundary(previous) {
                return None;
            }

            match line.get(idx + managed_root.len()).copied() {
                Some(b'/') | Some(b'\\') | Some(b'"') | Some(b'\'') | Some(b'\r') | Some(b'\n')
                | None => Some(idx),
                Some(next) if next.is_ascii_whitespace() => Some(idx),
                _ => None,
            }
        })
}

fn detect_config_file_cycles(entries: &[GhosttyConfigEntry], home: &Path) -> Vec<Vec<PathBuf>> {
    let mut stack = Vec::new();
    let mut completed = BTreeSet::new();
    let mut emitted = BTreeSet::new();
    let mut cycles = Vec::new();

    for entry in entries.iter().filter(|entry| entry.exists) {
        let start = normalize_config_path(&entry.path);
        visit_config_file(
            &start,
            home,
            &mut stack,
            &mut completed,
            &mut emitted,
            &mut cycles,
            0,
        );
    }

    cycles
}

fn visit_config_file(
    path: &Path,
    home: &Path,
    stack: &mut Vec<PathBuf>,
    completed: &mut BTreeSet<PathBuf>,
    emitted: &mut BTreeSet<String>,
    cycles: &mut Vec<Vec<PathBuf>>,
    depth: usize,
) {
    let path = normalize_config_path(path);
    if let Some(pos) = stack.iter().position(|entry| entry == &path) {
        let mut cycle = stack[pos..].to_vec();
        cycle.push(path);
        let signature = cycle
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\0");
        if emitted.insert(signature) {
            cycles.push(cycle);
        }
        return;
    }
    if completed.contains(&path) || depth > 64 || !path.exists() {
        return;
    }

    let refs = read_config_file_refs(&path, home);
    stack.push(path.clone());
    for referenced in refs {
        visit_config_file(
            &referenced,
            home,
            stack,
            completed,
            emitted,
            cycles,
            depth + 1,
        );
    }
    stack.pop();
    completed.insert(path);
}

fn read_config_file_refs(path: &Path, home: &Path) -> Vec<PathBuf> {
    let Ok(content) = fs::read(path) else {
        return Vec::new();
    };
    let current_dir = path.parent().unwrap_or_else(|| Path::new("/"));

    content
        .split(|b| *b == b'\n')
        .filter_map(|line| extract_config_file_ref(line, current_dir, home))
        .collect()
}

fn extract_config_file_ref(line: &[u8], current_dir: &Path, home: &Path) -> Option<PathBuf> {
    let trimmed = trim_ascii(line);
    if trimmed.starts_with(b"#") || trimmed.is_empty() {
        return None;
    }

    let key = line_key(trimmed);
    if key != b"config-file" {
        return None;
    }

    let value = if let Some(eq_idx) = trimmed.iter().position(|b| *b == b'=') {
        trim_ascii(&trimmed[eq_idx + 1..])
    } else {
        trim_ascii(&trimmed[key.len()..])
    };
    let raw_path = extract_path_value(value)?;
    let raw_path = String::from_utf8(raw_path.to_vec()).ok()?;

    Some(resolve_config_ref_path(&raw_path, current_dir, home))
}

fn extract_path_value(value: &[u8]) -> Option<&[u8]> {
    if value.is_empty() {
        return None;
    }

    if matches!(value.first(), Some(b'"' | b'\'')) {
        let quote = value[0];
        return value[1..]
            .iter()
            .position(|b| *b == quote)
            .map(|end| &value[1..1 + end]);
    }

    let end = value
        .iter()
        .position(|b| matches!(*b, b'\r' | b' ' | b'\t'))
        .unwrap_or(value.len());
    Some(&value[..end])
}

fn resolve_config_ref_path(raw_path: &str, current_dir: &Path, home: &Path) -> PathBuf {
    let expanded = if raw_path == "~" {
        home.to_path_buf()
    } else if let Some(rest) = raw_path.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(raw_path)
    };
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        current_dir.join(expanded)
    };

    normalize_config_path(&absolute)
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

fn line_key(line: &[u8]) -> &[u8] {
    let key_end = line
        .iter()
        .position(|b| *b == b'=' || b.is_ascii_whitespace())
        .unwrap_or(line.len());
    trim_ascii(&line[..key_end])
}

fn path_reference_starts_at_value_boundary(previous: Option<u8>) -> bool {
    match previous {
        Some(b'=') | Some(b'"') | Some(b'\'') | Some(b'[') | Some(b'(') | Some(b'{')
        | Some(b',') => true,
        Some(prev) => prev.is_ascii_whitespace(),
        None => true,
    }
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map(|idx| idx + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

fn format_ghostty_report(report: &GhosttyDoctorReport) -> String {
    let mut out = String::new();
    out.push_str("◆ Ghostty doctor\n");

    let selected = report
        .entries
        .iter()
        .find(|entry| entry.selected)
        .map(|entry| entry.path.display().to_string())
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
        out.push_str(&format!("    {}\n", entry.path.display()));
    }

    if report.duplicate_refs.is_empty() && report.config_file_cycles.is_empty() {
        out.push_str("cycle risk: none from duplicate Slate-managed refs or config-file cycles\n");
    } else {
        out.push_str("cycle risk: Ghostty config-file issues detected\n");
        if !report.duplicate_refs.is_empty() {
            out.push_str("duplicate Slate-managed refs:\n");
            for (slate_ref, paths) in &report.duplicate_refs {
                out.push_str(&format!("  - {slate_ref}\n"));
                for path in paths {
                    out.push_str(&format!("    in {}\n", path.display()));
                }
            }
        }
        if !report.config_file_cycles.is_empty() {
            out.push_str("config-file cycles:\n");
            for cycle in &report.config_file_cycles {
                let rendered = cycle
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                out.push_str(&format!("  - {rendered}\n"));
            }
        }
        out.push_str("fix: remove the repeated config-file edge, run `slate theme <current-theme>` to rebuild Slate refs in one entry, or `slate clean` to remove Slate refs.\n");
    }

    out.push_str(&format_ghostty_validation(&report.validation));

    out
}

fn format_ghostty_validation(validation: &GhosttyValidation) -> String {
    match validation {
        GhosttyValidation::Passed { binary } => {
            format!("ghostty validate: ok ({})\n", binary.display())
        }
        GhosttyValidation::Skipped(reason) => format!("ghostty validate: skipped ({reason})\n"),
        GhosttyValidation::Failed { binary, output } => {
            let excerpt = output
                .lines()
                .filter(|line| !line.trim().is_empty())
                .take(8)
                .collect::<Vec<_>>()
                .join("\n");
            if excerpt.is_empty() {
                format!("ghostty validate: failed ({})\n", binary.display())
            } else {
                format!(
                    "ghostty validate: failed ({})\n{excerpt}\n",
                    binary.display()
                )
            }
        }
    }
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
    let validation = match &report.validation {
        GhosttyValidation::Passed { binary } => serde_json::json!({
            "status": "passed",
            "message": null,
            "binary": binary.display().to_string(),
        }),
        GhosttyValidation::Skipped(reason) => serde_json::json!({
            "status": "skipped",
            "message": reason,
            "binary": null,
        }),
        GhosttyValidation::Failed { binary, output } => serde_json::json!({
            "status": "failed",
            "message": output,
            "binary": binary.display().to_string(),
        }),
    };

    serde_json::to_string_pretty(&serde_json::json!({
        "target": "ghostty",
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
        fs::write(ghostty_dir.join("config.ghostty"), "# first\n").unwrap();
        fs::write(ghostty_dir.join("config"), "# later\n").unwrap();

        let report = build_ghostty_report(&env).unwrap();
        let output = format_ghostty_report(&report);

        assert_eq!(
            report.selected_reason,
            "XDG config is the last existing entry in Ghostty load order"
        );
        assert!(output.contains("selected reason: XDG config is the last existing entry"));
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
    fn ghostty_report_json_is_machine_readable() {
        let report = GhosttyDoctorReport {
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
            selected_reason: "XDG config.ghostty is the last existing entry in Ghostty load order"
                .to_string(),
            validation: GhosttyValidation::Passed {
                binary: PathBuf::from("/Applications/Ghostty.app/Contents/MacOS/ghostty"),
            },
        };

        let json = format_ghostty_report_json(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["target"], "ghostty");
        assert_eq!(parsed["cycle_risk"], false);
        assert_eq!(
            parsed["selected_reason"],
            "XDG config.ghostty is the last existing entry in Ghostty load order"
        );
        assert_eq!(parsed["config_file_cycles"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["validation"]["status"], "passed");
        assert_eq!(
            parsed["validation"]["binary"],
            "/Applications/Ghostty.app/Contents/MacOS/ghostty"
        );
        assert_eq!(parsed["entries"][0]["load_order_index"], 0);
        assert_eq!(parsed["entries"][0]["slate_ref_count"], 1);
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
