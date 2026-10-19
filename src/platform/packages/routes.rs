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
    Pacman,
    Dnf,
    UserLocalStarship,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unavailable {
    UnsupportedOs,
    MissingPackageManager,
    NoAptMapping,
    NoPacmanMapping,
    NoDnfMapping,
    DnfZellijRequiresCopr,
}

impl Unavailable {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::UnsupportedOs => "automatic tool installation supports macOS and Linux only",
            Self::MissingPackageManager => "no supported package manager; install this tool manually or provide Homebrew on macOS / apt, pacman, or dnf on Linux",
            Self::NoAptMapping => "no apt mapping for this tool; install it manually before configuration",
            Self::NoPacmanMapping => "no pacman mapping for this tool; install it manually before configuration",
            Self::NoDnfMapping => "no dnf mapping for this tool; install it manually before configuration",
            Self::DnfZellijRequiresCopr => "Zellij requires the varlad/zellij COPR repository on Fedora; prepare that repository and install Zellij manually before configuration",
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
            PackageManagerBackend::Pacman if super::pacman::package_name(tool_id).is_some() => {
                Ok(ToolInstallRoute::Pacman)
            }
            PackageManagerBackend::Pacman => Err(Unavailable::NoPacmanMapping),
            PackageManagerBackend::Dnf if tool_id == "zellij" => {
                Err(Unavailable::DnfZellijRequiresCopr)
            }
            PackageManagerBackend::Dnf if super::dnf::package_name(tool_id).is_some() => {
                Ok(ToolInstallRoute::Dnf)
            }
            PackageManagerBackend::Dnf => Err(Unavailable::NoDnfMapping),
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
        let pacman = InstallContext {
            package_manager: PackageManagerBackend::Pacman,
            ..context
        };
        assert_eq!(pacman.route("yazi"), Ok(ToolInstallRoute::Pacman));
        assert_eq!(pacman.route("unknown"), Err(Unavailable::NoPacmanMapping));
        let dnf = InstallContext {
            package_manager: PackageManagerBackend::Dnf,
            ..context
        };
        assert_eq!(dnf.route("bat"), Ok(ToolInstallRoute::Dnf));
        assert_eq!(dnf.route("zellij"), Err(Unavailable::DnfZellijRequiresCopr));
        assert!(Unavailable::DnfZellijRequiresCopr
            .reason()
            .contains("varlad/zellij COPR"));
        assert_eq!(dnf.route("yazi"), Err(Unavailable::NoDnfMapping));
        let brew = InstallContext {
            package_manager: PackageManagerBackend::Homebrew,
            ..context
        };
        assert_eq!(brew.route("starship"), Ok(ToolInstallRoute::Homebrew));
        assert_eq!(brew.route("yazi"), Ok(ToolInstallRoute::Homebrew));
        for backend in [
            PackageManagerBackend::Homebrew,
            PackageManagerBackend::Apt,
            PackageManagerBackend::Pacman,
            PackageManagerBackend::Dnf,
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
