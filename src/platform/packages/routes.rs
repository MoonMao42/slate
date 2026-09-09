//! Pure installation policy shared by selection, preflight and execution.
//! A route is not proof of network access, permissions or helper availability.
use super::PackageManagerBackend;

#[derive(Clone, Copy, Debug)]
pub(crate) struct InstallContext {
    pub package_manager: PackageManagerBackend,
    pub supported_os: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolInstallRoute {
    Homebrew,
    Apt,
    UserLocalStarship,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unavailable {
    UnsupportedOs,
    MissingPackageManager,
    NoAptMapping,
}

impl Unavailable {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::UnsupportedOs => "automatic tool installation supports macOS and Linux only",
            Self::MissingPackageManager => "no supported package manager; install this tool manually or provide Homebrew on macOS / apt on Linux",
            Self::NoAptMapping => "no apt mapping for this tool; install it manually before configuration",
        }
    }
}

impl InstallContext {
    pub(crate) fn detect() -> Self {
        Self {
            package_manager: super::detect_backend(),
            supported_os: cfg!(any(target_os = "macos", target_os = "linux")),
        }
    }

    pub(crate) fn route(self, tool_id: &str) -> std::result::Result<ToolInstallRoute, Unavailable> {
        if !self.supported_os {
            return Err(Unavailable::UnsupportedOs);
        }
        match self.package_manager {
            PackageManagerBackend::Homebrew => Ok(ToolInstallRoute::Homebrew),
            _ if tool_id == "starship" => Ok(ToolInstallRoute::UserLocalStarship),
            PackageManagerBackend::Apt if super::apt::package_name(tool_id).is_some() => {
                Ok(ToolInstallRoute::Apt)
            }
            PackageManagerBackend::Apt => Err(Unavailable::NoAptMapping),
            PackageManagerBackend::Unsupported => Err(Unavailable::MissingPackageManager),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_route_policy_distinguishes_missing_manager_from_unsupported_os() {
        let context = InstallContext {
            package_manager: PackageManagerBackend::Unsupported,
            supported_os: true,
        };
        assert_eq!(
            context.route("starship"),
            Ok(ToolInstallRoute::UserLocalStarship)
        );
        assert_eq!(
            context.route("bat"),
            Err(Unavailable::MissingPackageManager)
        );
        let apt = InstallContext {
            package_manager: PackageManagerBackend::Apt,
            ..context
        };
        assert_eq!(
            apt.route("starship"),
            Ok(ToolInstallRoute::UserLocalStarship)
        );
        assert_eq!(apt.route("bat"), Ok(ToolInstallRoute::Apt));
        assert_eq!(apt.route("yazi"), Err(Unavailable::NoAptMapping));
        assert_eq!(apt.route("unknown"), Err(Unavailable::NoAptMapping));
        let brew = InstallContext {
            package_manager: PackageManagerBackend::Homebrew,
            ..context
        };
        assert_eq!(brew.route("starship"), Ok(ToolInstallRoute::Homebrew));
        assert_eq!(brew.route("yazi"), Ok(ToolInstallRoute::Homebrew));
        for backend in [
            PackageManagerBackend::Homebrew,
            PackageManagerBackend::Apt,
            PackageManagerBackend::Unsupported,
        ] {
            assert_eq!(
                InstallContext {
                    package_manager: backend,
                    supported_os: false
                }
                .route("starship"),
                Err(Unavailable::UnsupportedOs)
            );
        }
    }
}
