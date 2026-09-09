//! Env-aware Neovim checks use disposable scripts, never a real editor.
use slate_cli::adapter::{NvimAdapter, ToolAdapter};
use slate_cli::env::SlateEnv;
use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
use std::path::PathBuf;
use std::time::Duration;

#[test]
#[ignore = "invoked only by the deadline-guarded private-profile parent"]
fn nvim_availability_adapter_child() {
    let target = PathBuf::from(std::env::var_os("SLATE_NVIM_TARGET_HOME").unwrap());
    let env = SlateEnv::with_home(target.clone());
    let case = std::env::var("SLATE_NVIM_PROBE_CASE").unwrap();
    let result = NvimAdapter.is_installed_with_env(&env);
    match case.as_str() {
        "ready" | "ready-dev" => assert!(result.unwrap()),
        "old" | "floor-dev" => assert!(!result.unwrap()),
        "invalid" | "short" | "dependency" | "unlaunchable" => {
            let error = result.unwrap_err().to_string();
            assert!(
                error.contains(if case == "unlaunchable" {
                    "Could not start or safely read"
                } else {
                    "Could not read a complete semantic version"
                }),
                "{error}"
            );
            assert!(!error.contains("private-output"));
            // The shared coordinator must report the actual check failure,
            // not a successful skip, and must not apply Neovim state.
            let mut registry = slate_cli::adapter::registry::ToolRegistry::new();
            registry.register(Box::new(NvimAdapter));
            let themes = slate_cli::theme::ThemeRegistry::new().unwrap();
            let results =
                registry.apply_theme_to_tools_with_env(themes.get("nord").unwrap(), &env, None);
            assert_eq!(results.len(), 1);
            assert!(matches!(
                results[0].status,
                slate_cli::adapter::registry::ToolApplyStatus::Failed(_)
            ));
        }
        _ => panic!("unexpected fixture case"),
    }
    let log = PathBuf::from(std::env::var_os("SLATE_NVIM_PROBE_LOG").unwrap());
    if case == "unlaunchable" {
        assert!(!log.exists());
    } else {
        assert_eq!(
            std::fs::read_to_string(log).unwrap(),
            if matches!(case.as_str(), "invalid" | "short" | "dependency") {
                "probe\nprobe\n"
            } else {
                "probe\n"
            }
        );
    }
    assert!(!target.join(".config").exists());
    assert!(!target.join(".cache").exists());
}

#[test]
fn nvim_availability_uses_injected_home_and_exact_fallback() {
    for (case, body) in [
        ("ready", "printf 'NVIM v0.8.0\\n'"),
        (
            "ready-dev",
            "printf 'NVIM v0.12.0-dev-123+gabc\\nLuaJIT 2.1.0\\n'",
        ),
        ("old", "printf 'NVIM v0.7.2\\n'"),
        ("floor-dev", "printf 'NVIM v0.8.0-dev\\nLuaJIT 2.1.0\\n'"),
        ("invalid", "printf 'private-output\\n'"),
        ("short", "printf 'NVIM v0.8\\nLuaJIT 2.1.0\\n'"),
        (
            "dependency",
            "printf 'NVIM private-output\\nLuaJIT 2.1.0\\n'",
        ),
        ("unlaunchable", "exit 91"),
    ] {
        let td = tempfile::tempdir().unwrap();
        // Linux permits raw filename bytes; macOS APFS rejects invalid UTF-8
        // before any application code runs. There use Unicode and spaces.
        let name = if cfg!(target_os = "linux") {
            std::ffi::OsString::from_vec(b"injected-\xff home".to_vec())
        } else {
            std::ffi::OsString::from("injected-编辑器 home")
        };
        let target = td.path().join(name);
        let bin = target.join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let executable = bin.join("nvim");
        // Execute access is not proof that an interpreter can be launched.
        // /dev/null cannot be a directory, so this never starts a host program.
        let script = if case == "unlaunchable" {
            "#!/dev/null/slate-test-interpreter\n".to_string()
        } else {
            format!("#!/bin/sh\n[ \"$1\" = --version ] || exit 91\nprintf 'probe\\n' >> \"$SLATE_NVIM_PROBE_LOG\"\n{body}\n")
        };
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Ambient HOME points elsewhere, and PATH cannot resolve `nvim`.
        // Only the explicit profile's fallback executable is valid evidence.
        let ambient = td.path().join("ambient");
        let ambient_bin = ambient.join(".local/bin");
        std::fs::create_dir_all(&ambient_bin).unwrap();
        std::fs::write(ambient_bin.join("nvim"), "#!/bin/sh\nexit 93\n").unwrap();
        std::fs::set_permissions(
            ambient_bin.join("nvim"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", &ambient)
            .env("SLATE_HOME", &ambient)
            .env("PATH", td.path().join("empty-bin"))
            .env("SLATE_NVIM_TARGET_HOME", &target)
            .env("SLATE_NVIM_PROBE_CASE", case)
            .env("SLATE_NVIM_PROBE_LOG", td.path().join("probe-calls"))
            .args([
                "--exact",
                "nvim_availability_adapter_child",
                "--ignored",
                "--nocapture",
            ])
            .timeout(Duration::from_secs(7))
            .assert()
            .success();
    }
}
