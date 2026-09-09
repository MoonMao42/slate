//! Installation intent is separate from download intent: fonts and user-local
//! Starship do not require Homebrew/apt, and guided choices are not known yet.
use super::{PreflightCheck, PreflightScenario};
use crate::{
    detection::ToolPresence,
    platform::packages::{InstallContext, ToolInstallRoute},
    platform::shell::ShellBackend,
};
use std::collections::HashMap;

pub(super) fn requires_package_manager(
    installed: &HashMap<String, ToolPresence>,
    scenario: PreflightScenario,
    context: InstallContext,
    shell: ShellBackend,
) -> bool {
    match scenario {
        PreflightScenario::GuidedSetup | PreflightScenario::ConfigOnlyReconfigure => false,
        // Legacy generic API has no exact tool. Production --only uses retry_check.
        PreflightScenario::RetryInstall => true,
        PreflightScenario::QuickSetup => crate::cli::tool_selection::quick_core_tools(shell)
            .iter()
            .any(|tool| {
                !installed
                    .get(*tool)
                    .is_some_and(|presence| presence.installed)
                    && context.route(tool) != Ok(ToolInstallRoute::UserLocalStarship)
            }),
    }
}

pub(super) fn retry_check(tool_id: &str, context: InstallContext) -> PreflightCheck {
    let route = context.route(tool_id);
    let description = match route {
        Ok(ToolInstallRoute::Homebrew) => {
            "Homebrew route; package access is checked during installation"
        }
        Ok(ToolInstallRoute::Apt) => "apt route; package access is checked during installation",
        Ok(ToolInstallRoute::UserLocalStarship) => {
            "user-local installer; curl, network and target checks run during installation"
        }
        Err(unavailable) => unavailable.reason(),
    };
    PreflightCheck {
        name: "Tool Installation".into(),
        description: format!("{}: {description}", tool_id.escape_default()),
        passed: route.is_ok(),
        blocking: true,
    }
}

#[cfg(test)]
mod tests;
