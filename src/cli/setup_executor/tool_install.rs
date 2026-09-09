use crate::cli::tool_selection::BrewKind;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::platform::packages::{self, InstallContext, PackageManagerBackend, ToolInstallRoute};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolInstallMethod {
    Homebrew,
    Apt,
    UserLocal(PathBuf),
}

impl ToolInstallMethod {
    pub(crate) fn success_message(&self, label: &str) -> String {
        self.success_message_in(label, crate::cli::ui_language::output_language())
    }

    fn success_message_in(
        &self,
        label: &str,
        language: crate::config::ui_language::UiLanguage,
    ) -> String {
        use crate::cli::file_output::terminal_text;
        use crate::config::ui_language::UiLanguage;
        let label = terminal_text(label);
        match self {
            Self::Homebrew | Self::Apt => match language {
                UiLanguage::Chinese => format!("✓ {label} 已安装"),
                UiLanguage::English => format!("✓ {label} installed"),
            },
            Self::UserLocal(bin_dir) => {
                let path = terminal_text(&bin_dir.to_string_lossy());
                match language {
                    UiLanguage::Chinese => format!("✓ {label} 已安装到 {path}"),
                    UiLanguage::English => format!("✓ {label} installed locally at {path}"),
                }
            }
        }
    }
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    use crate::config::ui_language::UiLanguage;

    #[test]
    fn install_receipts_translate_without_losing_destination_or_claiming_activation() {
        for language in [UiLanguage::Chinese, UiLanguage::English] {
            for method in [
                ToolInstallMethod::Homebrew,
                ToolInstallMethod::Apt,
                ToolInstallMethod::UserLocal(PathBuf::from("/fixture/\x1b[2J\n/bin")),
            ] {
                let text = method.success_message_in("Starship\rtest", language);
                assert!(text.contains("Starship"));
                assert!(!text.contains('\x1b') && !text.contains('\r'));
                assert_eq!(text.lines().count(), 1);
                assert!(text.contains(if language == UiLanguage::Chinese {
                    "已安装"
                } else {
                    "installed"
                }));
                if matches!(method, ToolInstallMethod::UserLocal(_)) {
                    assert!(text.contains("/fixture/") && text.ends_with("/bin"));
                }
                assert!(!text.contains("activated") && !text.contains("已启用"));
            }
        }
    }
}

/// Route each tool once. Starship without Homebrew uses the staged local installer
/// directly instead of relying on a non-matching text error to trigger fallback.
pub(crate) fn install_tool(
    tool_id: &str,
    package: &str,
    kind: BrewKind,
    env: &SlateEnv,
) -> Result<ToolInstallMethod> {
    install_for_backend(
        InstallContext::detect(),
        tool_id,
        env,
        |backend| match backend {
            PackageManagerBackend::Homebrew => packages::install_with_homebrew(package, kind),
            PackageManagerBackend::Apt => packages::apt::install(tool_id),
            PackageManagerBackend::Unsupported => unreachable!("rejected before installation"),
        },
        super::starship::install,
    )
}

/// The caller revalidates the reviewed route before invoking this. Dispatch
/// consumes that route instead of choosing a different backend a second time.
pub(crate) fn install_planned(
    planned: &crate::cli::tool_selection::PlannedToolInstall,
    env: &SlateEnv,
) -> Result<ToolInstallMethod> {
    let tool = &planned.metadata;
    install_via_route(
        planned.route,
        tool.id,
        env,
        |backend| match backend {
            PackageManagerBackend::Homebrew => {
                packages::install_with_homebrew(tool.brew_package, tool.brew_kind)
            }
            PackageManagerBackend::Apt => packages::apt::install(tool.id),
            PackageManagerBackend::Unsupported => unreachable!("not a package route"),
        },
        super::starship::install,
    )
}

fn install_for_backend(
    context: InstallContext,
    tool_id: &str,
    env: &SlateEnv,
    package_install: impl FnOnce(PackageManagerBackend) -> Result<()>,
    local_install: impl FnOnce(&SlateEnv) -> Result<()>,
) -> Result<ToolInstallMethod> {
    let route = context
        .route(tool_id)
        .map_err(|unavailable| crate::error::SlateError::Internal(unavailable.reason().into()))?;
    install_via_route(route, tool_id, env, package_install, local_install)
}

fn install_via_route(
    route: ToolInstallRoute,
    tool_id: &str,
    env: &SlateEnv,
    package_install: impl FnOnce(PackageManagerBackend) -> Result<()>,
    local_install: impl FnOnce(&SlateEnv) -> Result<()>,
) -> Result<ToolInstallMethod> {
    if route == ToolInstallRoute::UserLocalStarship {
        local_install(env)?;
        return Ok(ToolInstallMethod::UserLocal(env.user_local_bin()));
    }
    let backend = match route {
        ToolInstallRoute::Homebrew => PackageManagerBackend::Homebrew,
        ToolInstallRoute::Apt => PackageManagerBackend::Apt,
        ToolInstallRoute::UserLocalStarship => unreachable!("handled before package installation"),
    };
    match package_install(backend) {
        Ok(()) => Ok(match backend {
            PackageManagerBackend::Homebrew => ToolInstallMethod::Homebrew,
            PackageManagerBackend::Apt => ToolInstallMethod::Apt,
            PackageManagerBackend::Unsupported => unreachable!("rejected before installation"),
        }),
        Err(err) if tool_id == "starship" && should_try_local_starship_fallback(&err) => {
            local_install(env)?;
            Ok(ToolInstallMethod::UserLocal(env.user_local_bin()))
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn should_try_local_starship_fallback(err: &crate::error::SlateError) -> bool {
    if !super::installation_fallback_allowed(err) {
        return false;
    }
    let message = err.to_string().to_lowercase();
    message.contains("permission denied")
        || message.contains("not writable")
        || message.contains("homebrew was not found")
}

#[cfg(test)]
mod tests;
