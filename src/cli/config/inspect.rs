use super::catalog::{self, Setting, SETTINGS};
use crate::{
    config::{recovery_paths, ConfigManager},
    env::SlateEnv,
    error::Result,
    opacity::OpacityPreset,
};
use serde::Serialize;
use std::{io::Write, path::PathBuf};

#[derive(Serialize)]
struct Report {
    schema_version: u8,
    scope: &'static str,
    settings: Vec<Entry>,
}

#[derive(Serialize)]
struct Entry {
    key: &'static str,
    description: &'static str,
    status: &'static str,
    value: Option<serde_json::Value>,
    path: String,
    path_is_lossy: bool,
    set_values: &'static [&'static str],
    #[serde(skip_serializing_if = "Option::is_none")]
    issue: Option<&'static str>,
}

fn source(env: &SlateEnv, key: &str) -> PathBuf {
    match key {
        "opacity" => env.managed_file("current-opacity"),
        "auto-theme" | "sound" => env.managed_file("config.toml"),
        "fastfetch" => env.managed_file("autorun-fastfetch"),
        "editor" => env.nvim_auto_activation_path(),
        _ => unreachable!("catalog key"),
    }
}

fn value(config: &ConfigManager, key: &str) -> Result<Option<serde_json::Value>> {
    let flag = match key {
        "opacity" => {
            let Some(stored) = config.get_current_opacity()? else {
                return Ok(None);
            };
            let preset = stored.parse::<OpacityPreset>()?;
            return Ok(Some(serde_json::Value::String(preset.to_string())));
        }
        "auto-theme" => config.inspect_auto_theme_enabled()?,
        "fastfetch" => config.has_fastfetch_autorun()?,
        "sound" => config.inspect_sound_enabled()?,
        "editor" => config.is_editor_auto_activation_enabled()?,
        _ => unreachable!("catalog key"),
    };
    Ok(Some(serde_json::Value::Bool(flag)))
}

fn inspect(env: &SlateEnv, selected: &[&Setting]) -> Report {
    let config = ConfigManager::from_env_paths(env);
    let settings = selected.iter().map(|setting| {
        let path = source(env, setting.key);
        let result = recovery_paths::validate_file_path(env, &path, "configuration preference")
            .and_then(|()| value(&config, setting.key));
        let (status, value, issue) = match result {
            Ok(Some(value)) => ("ok", Some(value), None),
            Ok(None) => ("unset", None, None),
            Err(_) => ("error", None, Some("Cannot resolve this preference. Inspect the source path, permissions, links, file-size limits and value format; file contents are omitted. Other preferences can still be inspected independently.")),
        };
        Entry { key: setting.key, description: setting.description, status, value, path: path.display().to_string(), path_is_lossy: path.to_str().is_none(), set_values: setting.set_values, issue }
    }).collect();
    Report { schema_version: 1, scope: "Read-only resolved preferences, including defaults; not proof that running tools applied them. Reads are separate observations, not a configuration snapshot. No tools, writers or sound are started. Final links, special files and isolated-profile escapes are rejected; configuration documents are bounded to 256 KiB and state files to 4 KiB. Unset opacity has no inferred preset; errors have null values, not false defaults.", settings }
}

fn text(report: &Report) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for setting in &report.settings {
        let value = match &setting.value {
            Some(serde_json::Value::Bool(flag)) => {
                if setting.key == "sound" {
                    if *flag {
                        "on"
                    } else {
                        "off"
                    }
                } else if *flag {
                    "enable"
                } else {
                    "disable"
                }
            }
            Some(serde_json::Value::String(value)) => value,
            _ => setting.status,
        };
        let _ = writeln!(
            output,
            "{}: {}\n  {}\n  {}{}",
            setting.key,
            value,
            setting.description,
            catalog::escaped(&setting.path),
            if setting.path_is_lossy {
                " (lossy display; not an exact path)"
            } else {
                ""
            }
        );
        if let Some(issue) = setting.issue {
            let _ = writeln!(output, "  {issue}");
        }
    }
    let _ = writeln!(output, "{}", report.scope);
    output
}

/// Get one catalog preference, or list all; never initialize ConfigManager.
pub fn handle_inspect(env: &SlateEnv, key: Option<&str>, json: bool) -> Result<()> {
    let selected = match key {
        Some(key) => vec![catalog::setting(key)?],
        None => SETTINGS.iter().collect(),
    };
    let report = inspect(env, &selected);
    let output = if json {
        format!("{}\n", serde_json::to_string_pretty(&report)?)
    } else {
        text(&report)
    };
    match std::io::stdout().lock().write_all(output.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}
