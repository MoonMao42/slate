//! File evidence only. Literal statement-shaped lines are not shell execution
//! or a complete parser of control flow, quoted blocks, functions or heredocs.
use super::Report;
use crate::{
    adapter::marker_block,
    config::{
        file_read::{self, Links, Source, MAX_TOOL_CONFIG_BYTES},
        recovery_paths,
    },
    detection::shell_quote,
    env::SlateEnv,
    platform::shell::fish_quote,
};
use std::path::Path;

pub(super) fn inspect(report: &mut Report, env: &SlateEnv, target: &str) {
    report.scope = "Read-only checks of Slate's selected startup file and managed environment, up to 8 MiB each. Final symlinks and special files are reported, not followed; isolated-profile escapes are rejected. Matching literal source-line text is not proof of shell execution, reachability or live appearance. Shell syntax, startup precedence, variable expansion, relative/indirect sources, functions, heredocs and multiline quoting are not evaluated. Files are observed separately; no shell, installer or configuration writer is run.";
    let startup = match target {
        "zsh" => env.zshrc_path(),
        "bash" => env.bash_integration_path(),
        "fish" => env.fish_loader_path(),
        _ => unreachable!("validated shell target"),
    };
    let managed = env
        .config_dir()
        .join("managed/shell")
        .join(format!("env.{target}"));
    let content = read(
        report,
        env,
        &startup,
        "startup_file",
        "Selected shell startup file",
    );
    read(
        report,
        env,
        &managed,
        "managed_environment",
        "Slate shell environment",
    );
    if target == "bash" {
        report.add_code("startup_convention", "info", if cfg!(target_os = "macos") {
            "Slate targets login Bash on macOS; non-login .bashrc chaining is not inspected or created"
        } else {
            "Slate targets interactive .bashrc on Linux; login-profile chaining is not inspected or created"
        }, &startup, None);
    }
    let Some(content) = content else {
        return;
    };
    if target != "fish" {
        if let Err(error) = marker_block::validate_block_state_bytes(&content.bytes) {
            report.add_code("loader_markers", "error", format!("Setup cannot update the loader markers: {error}"), &startup,
                Some("Inspect marker lines or a trusted backup before retrying setup; do not remove surrounding user configuration.".into()));
            return;
        }
        let has_markers = content
            .bytes
            .windows(marker_block::START.len())
            .any(|word| word == marker_block::START.as_bytes());
        report.add_code(
            "loader_markers",
            if has_markers { "ok" } else { "info" },
            if has_markers {
                "Slate marker boundaries are valid; shell syntax is not checked"
            } else {
                "No Slate marker block found; manual source statements may still be present"
            },
            &startup,
            None,
        );
    } else {
        report.add_code("loader_ownership", "info", "Setup regenerates this entire Slate-owned conf.d file; user config.fish is not inspected or modified", &startup, None);
    }
    inspect_reference(report, &content.bytes, &startup, &managed, target == "fish");
}

fn inspect_reference(
    report: &mut Report,
    content: &[u8],
    startup: &Path,
    managed: &Path,
    fish: bool,
) {
    let Some(expected) = managed
        .to_str()
        .filter(|value| !value.contains(['\n', '\r']))
    else {
        report.add_code("loader_reference", "info", "Literal reference comparison is unavailable for non-UTF-8 or multiline paths", startup,
            Some("Inspect the exact path and loader manually; the displayed path is not a substitute for its bytes.".into()));
        return;
    };
    let found = literal_reference(content, expected, fish);
    report.add_code("loader_reference", if found { "ok" } else { "warning" },
        if found { "Matching literal Slate source-line text found; execution is not verified" }
        else { "No matching literal Slate source-line text found; indirect or dynamic loading is not checked" },
        startup,
        (!found).then(|| "Inspect manual source logic and the managed environment path. `slate setup` can connect this entry after loader checks; it is not an automatic repair performed by doctor.".into()));
}

fn read(
    report: &mut Report,
    env: &SlateEnv,
    path: &Path,
    code: &'static str,
    label: &str,
) -> Option<Source> {
    let source = recovery_paths::validate_file_path(env, path, "shell diagnostic")
        .map_err(|error| error.to_string())
        .and_then(|()| {
            file_read::read(path, MAX_TOOL_CONFIG_BYTES, Links::Reject)
                .map_err(|error| error.to_string())
        });
    match source {
        Ok(Some(source)) => {
            report.add_code(
                code,
                "ok",
                format!("{label} is a readable regular file"),
                path,
                None,
            );
            Some(source)
        }
        Ok(None) => {
            report.add_code(code, "warning", format!("{label} is missing"), path,
                Some("Review your startup configuration, then use `slate setup` if you want Slate to connect it.".into()));
            None
        }
        Err(error) => {
            report.add_code(code, "error", format!("Cannot inspect {label}: {error}"), path,
                Some("Inspect links, parent directories, permissions and the 8 MiB regular-file limit. Preserve or relocate final links before setup; doctor has changed nothing.".into()));
            None
        }
    }
}

fn literal_reference(content: &[u8], expected: &str, fish: bool) -> bool {
    let mut words = vec![if fish {
        fish_quote(expected)
    } else {
        shell_quote(expected)
    }];
    // Only additional literal spellings with no shell expansion or escaping.
    if expected
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"/_-.".contains(&byte))
    {
        words.push(expected.to_owned());
    }
    if !expected.contains(['\\', '"', '$', '`']) {
        words.push(format!("\"{expected}\""));
    }
    let commands: &[&str] = if fish { &["source"] } else { &["source", "."] };
    content.split(|byte| *byte == b'\n').any(|line| {
        let line = line.trim_ascii();
        commands.iter().any(|command| {
            let Some(rest) = line.strip_prefix(command.as_bytes()) else {
                return false;
            };
            if !rest.first().is_some_and(u8::is_ascii_whitespace) {
                return false;
            }
            let rest = rest.trim_ascii_start();
            words.iter().any(|word| {
                rest.strip_prefix(word.as_bytes()).is_some_and(|tail| {
                    tail.is_empty()
                        || (tail.first().is_some_and(u8::is_ascii_whitespace)
                            && tail.trim_ascii_start().starts_with(b"#"))
                })
            })
        })
    })
}

#[cfg(test)]
mod tests;
