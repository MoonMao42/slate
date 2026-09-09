//! Bounded native validation. No reader threads can outlive a timed-out child.
use super::GhosttyValidation;
use crate::platform::process_output::{self, Completion, Limits};
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

pub(super) const TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const MAX_OUTPUT: usize = 64 * 1024;

pub(super) fn run(binary: &Path, timeout: Duration) -> GhosttyValidation {
    match capture(binary, timeout, || {}) {
        Ok(validation) => validation,
        Err(_) => GhosttyValidation::Unavailable {
            binary: binary.to_owned(),
            reason: "Could not start or safely read the Ghostty validator",
        },
    }
}

fn capture(
    binary: &Path,
    timeout: Duration,
    after_spawn: impl FnOnce(),
) -> io::Result<GhosttyValidation> {
    let captured = process_output::capture_with_hook(
        Command::new(binary).arg("+validate-config"),
        Limits {
            timeout,
            max_output: MAX_OUTPUT,
        },
        after_spawn,
    )?;
    let binary = binary.to_owned();
    Ok(match captured.completion {
        Completion::TimedOut => GhosttyValidation::TimedOut { binary },
        Completion::OutputLimit => GhosttyValidation::OutputLimit {
            binary,
            output: combined(&captured.stdout, &captured.stderr),
        },
        Completion::Exited(status) => {
            let output = combined(&captured.stdout, &captured.stderr);
            if status.success() && output.trim().is_empty() {
                GhosttyValidation::Passed { binary }
            } else {
                GhosttyValidation::Failed { binary, output }
            }
        }
    })
}

fn combined(stdout: &[u8], stderr: &[u8]) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn script(root: &Path, body: &str) -> std::path::PathBuf {
        let path = root.join("validator");
        fs::write(&path, format!("#!/bin/sh\n[ \"$1\" = +validate-config ] || exit 91\nif read -r unexpected; then exit 92; fi\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn ghostty_validation_classifies_native_results_and_bounds_combined_output() {
        let td = tempfile::tempdir().unwrap();
        for (body, status) in [
            ("exit 0", "passed"),
            ("exit 7", "failed"),
            ("printf 'warning\\n'; exit 0", "failed"),
            ("printf 'bad config\\n' >&2; exit 3", "failed"),
            ("i=0; while [ $i -lt 6000 ]; do printf 'stdout noise\\n'; printf 'stderr noise\\n' >&2; i=$((i+1)); done", "output_limit"),
        ] {
            let binary = script(td.path(), body);
            let report = run(&binary, TIMEOUT);
            assert_eq!(report.status(), status, "{report:?}");
            assert!(report.message().unwrap_or_default().len() <= MAX_OUTPUT);
            if status == "output_limit" {
                assert!(report.message().unwrap().contains("stdout noise"));
                assert!(report.message().unwrap().contains("stderr noise"));
            }
        }
        assert_eq!(run(&td.path().join("missing"), TIMEOUT).status(), "error");
    }

    #[test]
    fn ghostty_validation_timeout_reaps_leader_and_stops_inherited_pipe_group() {
        for inherited in [false, true] {
            let td = tempfile::tempdir().unwrap();
            let leader_file = td.path().join("leader");
            let child_file = td.path().join("descendant");
            let body = if inherited {
                format!(
                    "printf '%s' \"$$\" > '{}'; /bin/sleep 20 & printf '%s' \"$!\" > '{}'; exit 0",
                    leader_file.display(),
                    child_file.display()
                )
            } else {
                format!(
                    "printf '%s' \"$$\" > '{}'; exec /bin/sleep 20",
                    leader_file.display()
                )
            };
            let binary = script(td.path(), &body);
            let start = Instant::now();
            assert_eq!(
                capture(&binary, Duration::from_millis(200), || {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while !leader_file.exists() || (inherited && !child_file.exists()) {
                        assert!(Instant::now() < deadline, "validator fixture did not start");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                })
                .unwrap()
                .status(),
                "timed_out"
            );
            assert!(start.elapsed() < Duration::from_secs(6));
            let leader = fs::read_to_string(&leader_file)
                .unwrap()
                .parse::<i32>()
                .unwrap();
            assert_eq!(
                unsafe { libc::waitpid(leader, std::ptr::null_mut(), libc::WNOHANG) },
                -1
            );
            assert_eq!(
                io::Error::last_os_error().raw_os_error(),
                Some(libc::ECHILD)
            );
            if inherited {
                let descendant = fs::read_to_string(&child_file).unwrap();
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    let output = Command::new("/bin/ps")
                        .args(["-o", "stat=", "-p", &descendant])
                        .output()
                        .unwrap();
                    let state = String::from_utf8_lossy(&output.stdout);
                    if state.trim().is_empty() || state.trim_start().starts_with('Z') {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "owned descendant remains live: {state}"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    #[test]
    fn ghostty_validation_allows_exact_output_bound_and_escapes_terminal_controls() {
        let td = tempfile::tempdir().unwrap();
        let body = format!(
            "i=0; while [ $i -lt {} ]; do printf a; i=$((i+1)); done; exit 1",
            MAX_OUTPUT
        );
        let report = run(&script(td.path(), &body), TIMEOUT);
        assert_eq!(report.status(), "failed");
        assert_eq!(report.message().unwrap().len(), MAX_OUTPUT);
        let report = run(
            &script(td.path(), "printf '\\033[31merror\\n'; exit 1"),
            TIMEOUT,
        );
        let text = super::super::format_ghostty_validation(&report);
        assert!(!text.contains('\u{1b}'));
        assert!(text.contains("\\u{1b}[31merror"));
    }
}
