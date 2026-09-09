//! Stage the trusted upstream installer, then publish one checked executable.
//! Staging is not a sandbox; deadlines start after spawn, and detached children
//! or arbitrary script side effects are not covered by file recovery.
mod target;

use crate::{
    config::file_read::{self, FileIdentity, Links},
    env::SlateEnv,
    error::{Result, SlateError},
    platform::process_output::{self, Completion, Limits},
};
use std::{fs, path::Path, process::Command, time::Duration};

const INSTALL_URL: &str = "https://starship.rs/install.sh";
const DOWNLOAD_LIMITS: Limits = Limits {
    timeout: Duration::from_secs(75),
    max_output: 1024 * 1024,
};
const INSTALL_LIMITS: Limits = Limits {
    timeout: Duration::from_secs(600),
    max_output: 2 * 1024 * 1024,
};
const MAX_BINARY_BYTES: u64 = 64 * 1024 * 1024;

fn failure(reason: impl std::fmt::Display) -> SlateError {
    SlateError::Internal(format!(
        "Starship local fallback: {reason}. Command output omitted; the upstream installer is not sandboxed"
    ))
}

pub(super) fn install(env: &SlateEnv) -> Result<()> {
    let curl = crate::detection::command_path("curl")
        .ok_or_else(|| failure("curl was not found; install curl before retrying"))?;
    install_with(env, &curl, DOWNLOAD_LIMITS, INSTALL_LIMITS)
}

fn install_with(env: &SlateEnv, curl: &Path, download: Limits, install: Limits) -> Result<()> {
    // Read-only preflight happens before download, temporary/cache creation or
    // target-directory creation. Only HOME itself may resolve through an alias.
    let mut target = target::Target::capture(env)?;
    let temp = super::font_install::create_writable_temp_dir(env)?;
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir)?;
    let temp_identity = directory_identity(temp.path())?;
    let bin_identity = directory_identity(&bin_dir)?;
    let script = fetch(curl, download)?;
    if script.is_empty() {
        return Err(failure("downloaded installer is empty"));
    }
    let installer = temp.path().join("install.sh");
    fs::write(&installer, script)?;
    let mut command = Command::new("/bin/sh");
    crate::detection::apply_normalized_path(&mut command);
    command
        .arg(&installer)
        .args(["-y", "-b"])
        .arg(&bin_dir)
        .current_dir(temp.path())
        .env("TMPDIR", temp.path())
        .env_remove("ENV")
        .env_remove("BASH_ENV")
        .env_remove("SSLKEYLOGFILE");
    let output = process_output::capture(&mut command, install).map_err(|_| {
        SlateError::StarshipInstallUncertain("could not start or capture the installer".into())
    })?;
    match output.completion {
        Completion::Exited(status) if status.success() => {}
        Completion::Exited(status) if status.code().is_some() => {
            return Err(failure(format!(
                "installer failed with exit code {}",
                status.code().expect("known exit code")
            )));
        }
        completion => {
            let reason = match completion {
                Completion::Exited(_) => "installer terminated by a signal".to_owned(),
                Completion::TimedOut => format!(
                    "installer exceeded the {}-second wait limit",
                    install.timeout.as_secs()
                ),
                Completion::OutputLimit => format!(
                    "installer exceeded the {}-byte combined output limit",
                    install.max_output
                ),
            };
            return Err(SlateError::StarshipInstallUncertain(reason));
        }
    }
    if directory_identity(temp.path())? != temp_identity
        || directory_identity(&bin_dir)? != bin_identity
    {
        return Err(failure("staging directory was replaced"));
    }
    let binary = file_read::read(&bin_dir.join("starship"), MAX_BINARY_BYTES, Links::Reject)
        .map_err(failure)?
        .ok_or_else(|| failure("installer did not create a binary"))?;
    if binary.bytes.is_empty() || binary.mode.is_none_or(|mode| mode & 0o100 == 0) {
        return Err(failure("staged binary is empty or not owner-executable"));
    }
    // File shape/mode validation only: this does not execute the downloaded
    // binary or prove provenance, architecture compatibility or runtime health.
    target.publish(env, &binary.bytes)
}

fn fetch(curl: &Path, limits: Limits) -> Result<Vec<u8>> {
    let mut command = Command::new(curl);
    crate::detection::apply_normalized_path(&mut command);
    command
        .args([
            "--disable", // First argument: do not load ~/.curlrc.
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
            "60",
            INSTALL_URL,
        ])
        .env_remove("SSLKEYLOGFILE");
    let output = process_output::capture(&mut command, limits)
        .map_err(|_| failure("could not start or capture installer download"))?;
    match output.completion {
        Completion::Exited(status) if status.success() => Ok(output.stdout),
        Completion::Exited(_) => Err(failure("installer download failed")),
        Completion::TimedOut => Err(failure("installer download exceeded the wait limit")),
        Completion::OutputLimit => Err(failure("installer download exceeded the output limit")),
    }
}

fn directory_identity(path: &Path) -> Result<FileIdentity> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() {
        return Err(failure("directory is linked or not a directory"));
    }
    Ok(FileIdentity::from_metadata(&meta))
}

#[cfg(test)]
mod tests;
