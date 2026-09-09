//! Shell-aware Quick defaults shared with preflight's installation intent.
//! Manual selection and exact-tool retries retain the full installation catalog.
use super::{ToolCatalog, ToolPresence};
use crate::{
    error::{Result, SlateError},
    platform::shell::ShellBackend,
};
use std::collections::HashMap;

pub(crate) fn core_tools(shell: ShellBackend) -> &'static [&'static str] {
    match shell {
        ShellBackend::Zsh => &["starship", "zsh-syntax-highlighting"],
        ShellBackend::Bash | ShellBackend::Fish => &["starship"],
        // This is not permission to proceed: preflight's shell check and plan()
        // both reject unsupported shells independently of package availability.
        ShellBackend::Unsupported => &[],
    }
}

pub(crate) fn plan(
    installed: &HashMap<String, ToolPresence>,
    current_terminal: Option<&str>,
    shell: ShellBackend,
) -> Result<(Vec<String>, Vec<String>)> {
    if shell == ShellBackend::Unsupported {
        return Err(SlateError::PlatformError(
            "Quick setup requires zsh, bash, or fish; check the SHELL environment variable before retrying.".into(),
        ));
    }
    let core = core_tools(shell);
    let selected = core
        .iter()
        .filter(|&&id| {
            !installed.get(id).is_some_and(|presence| presence.installed)
                && ToolCatalog::get_tool(id).is_some_and(|tool| tool.installable)
        })
        .map(|id| id.to_string())
        .collect::<Vec<_>>();

    let mut configure = installed
        .iter()
        .filter(|(id, presence)| {
            presence.is_tier1()
                && (shell == ShellBackend::Zsh || id.as_str() != "zsh-syntax-highlighting")
        })
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    // HashMap discovery order is not a meaningful order for a review card.
    configure.sort();
    for id in core.iter().copied().chain(current_terminal) {
        if installed.get(id).is_some_and(|presence| presence.installed)
            && !configure.iter().any(|tool| tool == id)
        {
            configure.push(id.to_owned());
        }
    }
    for id in &selected {
        if !configure.contains(id) {
            configure.push(id.clone());
        }
    }
    Ok((selected, configure))
}

#[cfg(test)]
mod tests;
