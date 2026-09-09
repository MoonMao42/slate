//! Curl transport with bounded output, lifetime and regular-file writes. Its
//! configuration file is disabled; the executable itself is not sandboxed.
use super::{archive, create_writable_temp_dir, files};
use crate::{
    env::SlateEnv,
    error::{Result, SlateError},
    platform::process_output::{self, Completion, Limits},
};
use std::{io, os::unix::process::CommandExt, path::Path, process::Command, time::Duration};

const RELEASE_BASE: &str = "https://github.com/ryanoasis/nerd-fonts/releases/latest/download";
const LIMITS: Limits = Limits {
    timeout: Duration::from_secs(300),
    max_output: 64 * 1024,
};

fn failure(reason: &str) -> SlateError {
    SlateError::Internal(format!(
        "Font release download {reason}; command output omitted"
    ))
}

pub(super) fn install(asset: &str, curl: &Path, env: &SlateEnv) -> Result<files::Report> {
    install_with(asset, curl, env, LIMITS, archive::MAX_ARCHIVE_BYTES)
}

fn install_with(
    asset: &str,
    curl: &Path,
    env: &SlateEnv,
    limits: Limits,
    max_file: u64,
) -> Result<files::Report> {
    if asset.is_empty() || !asset.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(failure("has an invalid catalog asset"));
    }
    let temp = create_writable_temp_dir(env)?;
    let archive_path = temp.path().join("release.zip");
    fetch(
        curl,
        &format!("{RELEASE_BASE}/{asset}.zip"),
        &archive_path,
        limits,
        max_file,
    )?;
    let extracted = archive::extract(&archive_path, temp.path())?;
    files::install(extracted.path(), env)
}

fn fetch(curl: &Path, url: &str, destination: &Path, limits: Limits, max_file: u64) -> Result<()> {
    let mut command = Command::new(curl);
    // Must be FIRST: later --disable would not prevent loading ~/.curlrc.
    command
        .arg("--disable")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--globoff",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-redirs",
            "5",
            "--connect-timeout",
            "10",
            "--max-time",
            "90",
            "--http1.1",
            "--retry",
            "2",
            "--retry-max-time",
            "180",
            "--max-filesize",
        ])
        .arg(max_file.to_string())
        .args(["--user-agent", "slate-font-bootstrap", "--output"])
        .arg(destination)
        .arg(url)
        .env_remove("SSLKEYLOGFILE");
    limit_file_writes(&mut command, max_file);
    let output = process_output::capture(&mut command, limits)
        .map_err(|_| failure("could not start or capture curl"))?;
    match output.completion {
        Completion::Exited(status) if status.success() => Ok(()),
        Completion::Exited(_) => Err(failure("failed (network, HTTP or file-size error)")),
        Completion::TimedOut => Err(failure("timed out")),
        Completion::OutputLimit => Err(failure("exceeded the command-output limit")),
    }
}

fn limit_file_writes(command: &mut Command, max: u64) {
    // curl <8.4 only checks --max-filesize when the response declares a length.
    // Bound regular-file writes in the CHILD too, including unknown-length HTTP
    // responses. This is a per-file limit, not a total-disk or syscall sandbox.
    // SAFETY: this post-fork hook uses only async-signal-safe syscalls and stack
    // values. It never allocates, locks or changes the parent's limits.
    unsafe {
        command.pre_exec(move || {
            let mut current = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_FSIZE, &mut current) != 0 {
                return Err(io::Error::last_os_error());
            }
            let bounded = libc::rlimit {
                rlim_cur: current.rlim_cur.min(max as libc::rlim_t),
                rlim_max: current.rlim_max.min(max as libc::rlim_t),
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &bounded) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
