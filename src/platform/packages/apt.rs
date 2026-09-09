//! Bounded apt installation without password prompts or automatic repair.
//! Elevated/detached children can outlive capture; interruption is uncertainty,
//! not proof that package mutations stopped or can be rolled back.
use crate::{
    detection,
    error::{Result, SlateError},
    platform::process_output::{self, Completion, Limits},
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const LIMITS: Limits = Limits {
    timeout: Duration::from_secs(1800),
    max_output: 2 * 1024 * 1024,
};

pub(crate) fn package_name(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        "bat" => Some("bat"),
        "btop" => Some("btop"),
        "delta" => Some("git-delta"),
        "eza" => Some("eza"),
        "lazygit" => Some("lazygit"),
        "fastfetch" => Some("fastfetch"),
        "zsh-syntax-highlighting" => Some("zsh-syntax-highlighting"),
        _ => None,
    }
}

pub(crate) fn install(tool_id: &str) -> Result<()> {
    // Effective UID is a read-only property; root installations do not depend
    // on sudo being installed. Tests inject this decision and all executables.
    install_resolving(
        tool_id,
        unsafe { libc::geteuid() } == 0,
        detection::command_path,
        LIMITS,
    )
}

fn install_resolving(
    tool_id: &str,
    is_root: bool,
    mut resolve: impl FnMut(&str) -> Option<PathBuf>,
    limits: Limits,
) -> Result<()> {
    let package = package_name(tool_id).ok_or_else(|| {
        if tool_id == "starship" {
            SlateError::Internal("Starship uses the user-local route; run slate setup --only starship instead of the apt package API.".into())
        } else {
            SlateError::Internal(format!(
                "Slate has no apt mapping for '{}'; install it manually before rerunning setup.",
                tool_id.escape_default(),
            ))
        }
    })?;
    let apt = resolve("apt-get").ok_or_else(|| {
        SlateError::Internal("apt-get was not found; use a supported Linux distribution.".into())
    })?;
    let sudo = if is_root {
        None
    } else {
        Some(resolve("sudo").ok_or_else(|| SlateError::Internal(
            "sudo was not found; ask an administrator to install the mapped apt package, then rerun Slate as your normal user.".into()
        ))?)
    };
    run(&apt, sudo.as_deref(), package, limits)
}

fn run(apt: &Path, sudo: Option<&Path>, package: &str, limits: Limits) -> Result<()> {
    let mut command = if let Some(sudo) = sudo {
        let mut command = Command::new(sudo);
        // Assign only these fixed variables, not a preserved caller environment
        // or a privileged shell/env wrapper. Sudo policy can refuse assignments.
        // Assignments MUST precede --: sudo stops parsing environment options
        // at that delimiter and would treat a later assignment as the command.
        command
            .args(["-n", "DEBIAN_FRONTEND=noninteractive", "LC_ALL=C", "--"])
            .arg(apt);
        command
    } else {
        Command::new(apt)
    };
    detection::apply_normalized_path(&mut command);
    command
        .env("DEBIAN_FRONTEND", "noninteractive")
        .env("LC_ALL", "C")
        .args([
            "install",
            "-y",
            "--no-remove",
            "-o",
            "DPkg::Use-Pty=0",
            "--",
        ])
        .arg(package);
    let output = process_output::capture(&mut command, limits).map_err(|_| {
        SlateError::AptInstallUncertain("could not start or capture apt/sudo".into())
    })?;
    match output.completion {
        Completion::Exited(status) if status.success() => Ok(()),
        Completion::Exited(status) if status.code().is_some() => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
            let reason = if stderr.contains("a password is required") {
                "sudo needs authentication; authenticate in your own terminal and retry Slate there as your normal user, or ask an administrator to install the package"
            } else if stderr.contains("not in the sudoers")
                || stderr.contains("not allowed")
                || stderr.contains("permission denied")
            {
                "permission or sudo policy denied; ask an administrator to inspect access"
            } else if stderr.contains("could not get lock")
                || stderr.contains("unable to acquire")
                || stderr.contains("is another process using it")
            {
                "package database is busy; wait for its owner to finish and do not delete lock files"
            } else if stderr.contains("unable to locate package")
                || stderr.contains("has no installation candidate")
            {
                "package is unavailable in configured repositories; ask an administrator to inspect repository support"
            } else if stderr.contains("temporary failure resolving")
                || stderr.contains("could not resolve")
                || stderr.contains("failed to fetch")
            {
                "package download failed; inspect network and repository availability"
            } else {
                "inspect apt/dpkg for details; package scripts or configuration may require administrator attention"
            };
            Err(SlateError::Internal(format!(
                "apt installation for {package} failed with exit code {}: {reason}. Partial package changes may remain; no automatic repair was attempted. Command output omitted.",
                status.code().expect("known exit code"),
            )))
        }
        Completion::Exited(_) => Err(SlateError::AptInstallUncertain(
            "apt/sudo terminated by a signal".into(),
        )),
        Completion::TimedOut => Err(SlateError::AptInstallUncertain(format!(
            "apt/sudo exceeded the {}-second wait limit",
            limits.timeout.as_secs(),
        ))),
        Completion::OutputLimit => Err(SlateError::AptInstallUncertain(format!(
            "apt/sudo exceeded the {}-byte combined output limit",
            limits.max_output,
        ))),
    }
}

#[cfg(test)]
mod tests;
