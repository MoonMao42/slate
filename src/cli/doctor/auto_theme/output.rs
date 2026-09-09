//! Wire/text views of an already captured diagnostic. Never reread paths or
//! alter native runtime types to work around JSON's UTF-8 string requirement.
use super::{resolution, InstallationInspection, InstallationState, Report, RuntimeInspection};
use crate::cli::ui_language::tr;
use serde::{Serialize, Serializer};
use serde_json::{json, Value};
use std::{fmt::Write, path::Path};

pub(super) fn runtime<S: Serializer>(
    report: &RuntimeInspection,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    json!({
        "state": report.state,
        "lock_held": report.lock_held,
        "directory": report.directory.as_ref().map(|path| path.to_string_lossy()),
        "directory_is_lossy": report.directory.as_ref().map(|path| path.to_str().is_none()),
        "log_path": report.log_path.as_ref().map(|path| path.to_string_lossy()),
        "log_path_is_lossy": report.log_path.as_ref().map(|path| path.to_str().is_none()),
        "message": report.message,
    })
    .serialize(serializer)
}

fn component(path: &Path, state: InstallationState, executable: Option<bool>) -> Value {
    json!({
        "path": path.to_string_lossy(),
        "path_is_lossy": path.to_str().is_none(),
        "state": state,
        "executable": executable,
    })
}

pub(super) fn installation<S: Serializer>(
    report: &InstallationInspection,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    json!({
        "launcher": component(&report.launcher.path, report.launcher.state, report.launcher.executable),
        "directory": report.directory.to_string_lossy(),
        "directory_is_lossy": report.directory.to_str().is_none(),
        "directory_access": report.directory_access,
        "directory_access_scope": "Read-only real-user-ID write/search access check for an existing directory; no probe file. Allowed is not a guarantee of later writes, file replacement, or effective-ID access.",
        "helper": report.helper.as_ref().map(|helper| component(&helper.path, helper.state, helper.executable)),
    }).serialize(serializer)
}

pub(super) fn text(report: &Report) -> String {
    let enabled = match report.auto_theme_enabled {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    let mut output = format!(
        "Auto-theme doctor\nEnabled: {enabled}\nRuntime: {}\n",
        report.runtime.message,
    );
    resolution::append_text(&mut output, &report.resolution);
    let _ = writeln!(
        output,
        "Helper directory: {} — {}",
        report.installation.directory_access.label(),
        super::super::terminal_path(&report.installation.directory)
    );
    let _ = writeln!(
        output,
        "Launcher: {} — {}",
        report.installation.launcher.state.label(),
        super::super::terminal_path(&report.installation.launcher.path),
    );
    if let Some(helper) = &report.installation.helper {
        let _ = writeln!(
            output,
            "Helper: {} — {}",
            helper.state.label(),
            super::super::terminal_path(&helper.path),
        );
    }
    if let Some(path) = &report.runtime.log_path {
        let _ = writeln!(
            output,
            "Private log (not read): {}",
            super::super::terminal_path(path)
        );
    }
    for issue in &report.issues {
        let _ = writeln!(output, "Warning: {issue}");
    }
    for step in &report.next_steps {
        let _ = writeln!(output, "Next: {step}");
    }
    let _ = writeln!(output, "{}", report.scope);
    output
}

pub(super) fn menu_text(report: &Report) -> String {
    use crate::platform::dark_mode_notify::RuntimeState;
    // Do not hide an actionable failure behind a reassuring compact summary.
    if !report.issues.is_empty() {
        return format!(
            "{}\n{}",
            tr("自动换色诊断", "Auto-Theme Diagnostics"),
            text(report)
        );
    }
    let enabled = match report.auto_theme_enabled {
        Some(true) => tr("已开启", "On"),
        Some(false) => tr("已关闭", "Off"),
        None => tr("无法读取", "Unreadable"),
    };
    let runtime = match report.runtime.state {
        RuntimeState::Absent => tr("未发现运行实例", "No running instance found"),
        RuntimeState::Starting => tr("正在启动，请稍后再检查", "Starting; check again shortly"),
        RuntimeState::Ready => tr(
            "已就绪，未验证实际换色结果",
            "Ready; actual color changes unverified",
        ),
        RuntimeState::Stopping => tr("正在停止，请稍后再检查", "Stopping; check again shortly"),
        RuntimeState::Stopped => tr("已停止", "Stopped"),
        RuntimeState::Failed => tr("上次运行失败", "Last run failed"),
        RuntimeState::Stale => tr(
            "仅有旧记录，未确认运行",
            "Stale record; running state unconfirmed",
        ),
        RuntimeState::Changing => tr("状态变化中，请稍后再检查", "Changing; check again shortly"),
        RuntimeState::Unreadable => tr(
            "无法安全读取运行状态",
            "Runtime state could not be read safely",
        ),
    };
    let mut output = format!(
        "{}\n{}{enabled}\n{}{runtime}\n{}\n",
        tr("自动换色诊断", "Auto-Theme Diagnostics"),
        tr("已保存设置：", "Saved setting: "),
        tr("后台：", "Service: "),
        tr(
            "主题配对：可解析，未验证实际应用",
            "Theme pairing: readable; actual application unverified"
        )
    );
    if report.isolated {
        output.push_str(tr(
            "隔离配置：不启动桌面后台。\n",
            "Isolated profile: desktop service is not started.\n",
        ));
    }
    output.push_str(tr("本次只读检查，没有修改配置或启停后台。\n完整诊断：slate doctor auto-theme\n", "Read-only check; no configuration or service changes.\nFull diagnostics: slate doctor auto-theme\n"));
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::dark_mode_notify::RuntimeState;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

    #[test]
    fn auto_theme_output_runtime_serialization_keeps_optional_and_lossy_paths_distinct() {
        #[derive(Serialize)]
        struct Wire(#[serde(serialize_with = "runtime")] RuntimeInspection);
        // Synthetic paths exercise the wire format even on filesystems that
        // cannot create non-UTF-8 filenames. This is not watcher runtime evidence.
        for path in [
            None,
            Some(PathBuf::from("/valid/中文")),
            Some(PathBuf::from(OsString::from_vec(b"/invalid-\xff".to_vec()))),
        ] {
            let report = Wire(RuntimeInspection {
                state: RuntimeState::Unreadable,
                lock_held: None,
                directory: path.clone(),
                log_path: path.as_ref().map(|path| path.join("watcher.log")),
                message: "fixture",
            });
            let value = serde_json::to_value(report).unwrap();
            assert_eq!(value["state"], "unreadable");
            assert!(value["lock_held"].is_null());
            assert_eq!(
                value["directory"],
                json!(path.as_ref().map(|path| path.to_string_lossy()))
            );
            assert_eq!(
                value["directory_is_lossy"],
                json!(path.as_ref().map(|path| path.to_str().is_none()))
            );
            assert_eq!(value["log_path_is_lossy"], value["directory_is_lossy"]);
        }
    }
}
