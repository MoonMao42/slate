//! Policy fixtures, not package managers or downloaded installers.
use super::*;
use crate::error::SlateError;

fn context(backend: PackageManagerBackend) -> InstallContext {
    InstallContext {
        package_manager: backend,
        supported_os: true,
    }
}

#[test]
fn install_review_dispatch_consumes_the_given_route_without_selecting_a_backend() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    for (route, tool, method) in [
        (
            ToolInstallRoute::Homebrew,
            "delta",
            ToolInstallMethod::Homebrew,
        ),
        (ToolInstallRoute::Apt, "delta", ToolInstallMethod::Apt),
        (
            ToolInstallRoute::UserLocalStarship,
            "starship",
            ToolInstallMethod::UserLocal(env.user_local_bin()),
        ),
    ] {
        let result = install_via_route(
            route,
            tool,
            &env,
            |backend| {
                assert_eq!(
                    backend,
                    match route {
                        ToolInstallRoute::Homebrew => PackageManagerBackend::Homebrew,
                        ToolInstallRoute::Apt => PackageManagerBackend::Apt,
                        ToolInstallRoute::UserLocalStarship =>
                            panic!("local route must not invoke a package manager"),
                    }
                );
                Ok(())
            },
            |selected| {
                assert_eq!(route, ToolInstallRoute::UserLocalStarship);
                assert_eq!(selected.home(), env.home());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(result, method);
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn bootstrap_install_missing_package_manager_uses_local_starship_only() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    for fail in [false, true] {
        let calls = std::cell::Cell::new(0);
        let result = install_for_backend(
            context(PackageManagerBackend::Unsupported),
            "starship",
            &env,
            |_| panic!("no package manager may be invoked"),
            |selected| {
                assert_eq!(selected.home(), env.home());
                calls.set(calls.get() + 1);
                if fail {
                    Err(SlateError::StarshipInstallUncertain("fixture".into()))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(calls.get(), 1);
        if fail {
            assert!(matches!(
                result,
                Err(SlateError::StarshipInstallUncertain(_))
            ));
        } else {
            assert_eq!(
                result.unwrap(),
                ToolInstallMethod::UserLocal(env.user_local_bin())
            );
        }
    }
    assert!(install_for_backend(
        context(PackageManagerBackend::Unsupported),
        "bat",
        &env,
        |_| panic!("missing package manager"),
        |_| panic!("bat has no local installer"),
    )
    .is_err());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn apt_tool_routes_starship_directly_to_local_and_propagates_failure() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    for fail in [false, true] {
        let calls = std::cell::Cell::new(0);
        let result = install_for_backend(
            context(PackageManagerBackend::Apt),
            "starship",
            &env,
            |_| panic!("apt must never be asked to install Starship"),
            |selected| {
                assert_eq!(selected.home(), env.home());
                calls.set(calls.get() + 1);
                if fail {
                    Err(SlateError::StarshipInstallUncertain("fixture".into()))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(calls.get(), 1);
        if fail {
            assert!(matches!(
                result,
                Err(SlateError::StarshipInstallUncertain(_))
            ));
        } else {
            assert_eq!(
                result.unwrap(),
                ToolInstallMethod::UserLocal(env.user_local_bin())
            );
        }
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn apt_tool_routes_preserve_backend_receipts_and_never_install_on_unsupported() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    for (backend, expected) in [
        (PackageManagerBackend::Apt, ToolInstallMethod::Apt),
        (PackageManagerBackend::Homebrew, ToolInstallMethod::Homebrew),
    ] {
        let result = install_for_backend(
            context(backend),
            "bat",
            &env,
            |selected| {
                assert_eq!(selected, backend);
                Ok(())
            },
            |_| panic!("no local fallback for bat"),
        )
        .unwrap();
        assert_eq!(result, expected);
        assert_eq!(result.success_message("bat"), "✓ bat installed");
    }
    assert!(install_for_backend(
        InstallContext {
            package_manager: PackageManagerBackend::Unsupported,
            supported_os: false
        },
        "starship",
        &env,
        |_| panic!("unsupported package install"),
        |_| panic!("unsupported local install"),
    )
    .is_err());
}

#[test]
fn apt_tool_routes_keep_homebrew_fallback_policy_and_uncertainty_gate() {
    let temp = tempfile::tempdir().unwrap();
    let env = SlateEnv::with_home(temp.path().to_owned());
    for uncertain in [false, true] {
        let result = install_for_backend(
            context(PackageManagerBackend::Homebrew),
            "starship",
            &env,
            |_| {
                Err(if uncertain {
                    SlateError::HomebrewInstallUncertain("permission denied".into())
                } else {
                    SlateError::Internal("permission denied".into())
                })
            },
            |_| {
                assert!(!uncertain);
                Ok(())
            },
        );
        if uncertain {
            assert!(matches!(
                result,
                Err(SlateError::HomebrewInstallUncertain(_))
            ));
        } else {
            assert_eq!(
                result.unwrap(),
                ToolInstallMethod::UserLocal(env.user_local_bin())
            );
        }
    }
    let error = SlateError::AptInstallUncertain("permission denied; Homebrew was not found".into());
    assert!(!should_try_local_starship_fallback(&error));
    assert!(!super::super::installation_fallback_allowed(&error));
    assert!(matches!(
        install_for_backend(
            context(PackageManagerBackend::Apt),
            "bat",
            &env,
            |_| Err(error),
            |_| panic!("uncertainty must not trigger another installer"),
        ),
        Err(SlateError::AptInstallUncertain(_))
    ));
}
