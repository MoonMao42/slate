//! Bounded pacman installation for Arch Linux without interactive prompts.
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
        "yazi" => Some("yazi"),
        "zellij" => Some("zellij"),
        "zsh-syntax-highlighting" => Some("zsh-syntax-highlighting"),
        _ => None,
    }
}

pub(crate) fn install(tool_id: &str) -> Result<()> {
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
            SlateError::Internal(
                "Starship uses the user-local route; run slate setup --only starship instead of the pacman package API.".into(),
            )
        } else {
            SlateError::Internal(format!(
                "Slate has no pacman mapping for '{}'; install it manually before rerunning setup.",
                tool_id.escape_default(),
            ))
        }
    })?;
    let pacman = resolve("pacman").ok_or_else(|| {
        SlateError::Internal("pacman was not found; use a supported Linux distribution.".into())
    })?;
    let sudo = if is_root {
        None
    } else {
        Some(resolve("sudo").ok_or_else(|| {
            SlateError::Internal(
                "sudo was not found; ask an administrator to install the mapped pacman package, then rerun Slate as your normal user.".into(),
            )
        })?)
    };
    run(&pacman, sudo.as_deref(), package, limits)
}

fn run(pacman: &Path, sudo: Option<&Path>, package: &str, limits: Limits) -> Result<()> {
    let mut command = if let Some(sudo) = sudo {
        let mut command = Command::new(sudo);
        command.args(["-n", "LC_ALL=C", "--"]).arg(pacman);
        command
    } else {
        Command::new(pacman)
    };
    detection::apply_normalized_path(&mut command);
    command
        .env("LC_ALL", "C")
        .args(["-S", "--noconfirm", "--needed", "--"])
        .arg(package);
    let output = process_output::capture(&mut command, limits).map_err(|_| {
        SlateError::AptInstallUncertain("could not start or capture pacman/sudo".into())
    })?;
    match output.completion {
        Completion::Exited(status) if status.success() => Ok(()),
        Completion::Exited(status) if status.code().is_some() => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
            let reason = if stderr.contains("a password is required") {
                "sudo needs authentication; authenticate in your own terminal and retry Slate there as your normal user"
            } else if stderr.contains("unable to lock database") {
                "pacman database is locked by another process; wait for it to finish"
            } else if stderr.contains("target not found") {
                "package is unavailable in configured pacman repositories"
            } else {
                "inspect pacman output for details; package installation may require administrator attention"
            };
            Err(SlateError::Internal(format!(
                "pacman installation for {package} failed with exit code {}: {reason}. Command output omitted.",
                status.code().expect("known exit code"),
            )))
        }
        Completion::Exited(_) => Err(SlateError::AptInstallUncertain(
            "pacman/sudo terminated by a signal".into(),
        )),
        Completion::TimedOut => Err(SlateError::AptInstallUncertain(format!(
            "pacman/sudo exceeded the {}-second wait limit",
            limits.timeout.as_secs(),
        ))),
        Completion::OutputLimit => Err(SlateError::AptInstallUncertain(format!(
            "pacman/sudo exceeded the {}-byte combined output limit",
            limits.max_output,
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacman_mappings_cover_arch_packages() {
        assert_eq!(package_name("bat"), Some("bat"));
        assert_eq!(package_name("delta"), Some("git-delta"));
        assert_eq!(package_name("yazi"), Some("yazi"));
        assert_eq!(package_name("zellij"), Some("zellij"));
        assert_eq!(package_name("unknown"), None);
    }
}
