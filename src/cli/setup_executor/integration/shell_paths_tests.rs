//! Execute only generated shell configuration in private profiles. Optional
//! native Fish coverage is mandatory in the existing Linux has-fish CI job.
use super::*;
use crate::detection;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

use crate::test_tree as snapshot;

fn fixture(shell: ShellBackend) -> (tempfile::TempDir, SlateEnv, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp
        .path()
        .join(r"profile \\ ' $(literal) 中文")
        .join(r"tail\");
    let env = SlateEnv::with_home(home);
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_font("Private Mono").unwrap();
    config.set_starship_enabled(false).unwrap();
    config.set_zsh_highlighting_enabled(false).unwrap();
    config.set_auto_theme_enabled(false).unwrap();
    config.disable_fastfetch_autorun().unwrap();
    fs::create_dir_all(env.user_local_bin()).unwrap();
    let fastfetch = env.user_local_bin().join("fastfetch");
    fs::write(&fastfetch, "#!/bin/sh\nprintf '%s\\0' \"$@\"\n").unwrap();
    fs::set_permissions(&fastfetch, fs::Permissions::from_mode(0o755)).unwrap();
    let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
    setup_prepared_shell_integration(&theme, &env, &[], shell).unwrap();
    // Generated exports now include only existing files. Seed both privately
    // to keep this fixture exercising the complete comma-separated path list.
    for path in [
        crate::adapter::LazygitAdapter::theme_path(&env),
        env.lazygit_default_config().to_owned(),
    ] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "gui: {}\n").unwrap();
    }
    // The wrapper supplies -c only when its generated configuration exists.
    let fastfetch_config = env.config_dir().join("managed/fastfetch/config.jsonc");
    fs::create_dir_all(fastfetch_config.parent().unwrap()).unwrap();
    fs::write(fastfetch_config, "{}\n").unwrap();
    let loader = match shell {
        ShellBackend::Bash => env.bash_integration_path(),
        ShellBackend::Zsh => env.zshrc_path(),
        ShellBackend::Fish => env.fish_loader_path(),
        ShellBackend::Unsupported => unreachable!(),
    };
    (temp, env, loader)
}

fn command(binary: &Path, env: &SlateEnv) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(binary);
    command
        .env_clear()
        // These fixtures exercise UTF-8 paths, not the shell's ASCII locale.
        .env(
            "LC_ALL",
            if cfg!(target_os = "macos") {
                "en_US.UTF-8"
            } else {
                "C.UTF-8"
            },
        )
        .env("HOME", env.home())
        .env("XDG_CONFIG_HOME", env.xdg_config_home())
        .env("XDG_CACHE_HOME", env.cache_dir())
        .env("XDG_DATA_HOME", env.xdg_data_home())
        .env("PATH", "")
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .env("EXPECTED_BIN", env.user_local_bin())
        .env("ARG_ONE", "one space ' 中文")
        .env("ARG_TWO", r"two\\ $(literal)")
        .current_dir(env.home())
        .timeout(Duration::from_secs(5));
    command
}

fn verify_output(output: &[u8], env: &SlateEnv) {
    let managed = env.config_dir().join("managed");
    let expected = [
        managed.join("eza").to_str().unwrap().to_owned(),
        format!(
            "{}/lazygit/config.yml,{}/lazygit/config.yml",
            managed.display(),
            env.xdg_config_home().display()
        ),
        "-c".into(),
        managed
            .join("fastfetch/config.jsonc")
            .to_str()
            .unwrap()
            .to_owned(),
        "one space ' 中文".into(),
        r"two\\ $(literal)".into(),
    ]
    .into_iter()
    .flat_map(|word| word.into_bytes().into_iter().chain([0]))
    .collect::<Vec<_>>();
    assert_eq!(output, expected);
}

#[test]
fn fish_paths_setup_loader_uses_fish_quoting_not_posix_words() {
    let (_temp, env, loader) = fixture(ShellBackend::Fish);
    let source = env.config_dir().join("managed/shell/env.fish");
    let text = fs::read_to_string(loader).unwrap();
    assert!(text.contains(&crate::platform::shell::fish_quote(
        source.to_str().unwrap()
    )));
    assert!(!text.contains(&detection::shell_quote_path(&source)));
}

#[test]
fn fish_paths_posix_loaders_preserve_exact_paths_and_wrapper_arguments() {
    for (shell, name) in [(ShellBackend::Bash, "bash"), (ShellBackend::Zsh, "zsh")] {
        let Some(binary) = which::which(name).ok() else {
            assert_eq!(
                shell,
                ShellBackend::Zsh,
                "Bash required for this native check"
            );
            eprintln!("Zsh unavailable; its native shell-path check was not run");
            continue;
        };
        let (_temp, env, loader) = fixture(shell);
        let before = snapshot::tree(env.home());
        let mut process = command(&binary, &env);
        if shell == ShellBackend::Bash {
            process.args(["--noprofile", "--norc"]);
        } else {
            process.arg("-f");
        }
        let output = process
            .args([
                "-c",
                r#"
source "$1" || exit 71
source "$1" || exit 72
[ "${PATH%%:*}" = "$EXPECTED_BIN" ] || exit 73
printf '%s\0' "$EZA_CONFIG_DIR" "$LG_CONFIG_FILE"
fastfetch "$ARG_ONE" "$ARG_TWO"
"#,
                "slate-shell-fixture",
            ])
            .arg(loader)
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        verify_output(&output.stdout, &env);
        assert_eq!(snapshot::tree(env.home()), before);
    }
}

#[cfg(feature = "has-fish")]
#[test]
fn fish_paths_native_loader_preserves_exact_paths_and_wrapper_arguments() {
    let binary = std::env::var_os("SLATE_TEST_FISH")
        .map(PathBuf::from)
        .or_else(|| which::which("fish").ok())
        .expect("has-fish requires Fish on PATH or SLATE_TEST_FISH");
    let binary = fs::canonicalize(binary).unwrap();
    let (_temp, env, loader) = fixture(ShellBackend::Fish);
    // Fish can seed its own private caches even with --no-config. Establish a
    // no-Slate-code control before checking that repeated loading changes none.
    command(&binary, &env)
        .args(["--no-config", "--private", "-c", "true"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
    let before = snapshot::tree(env.home());
    let output = command(&binary, &env)
        .args([
            "--no-config",
            "--private",
            "-c",
            r#"
source "$argv[1]"; or exit 71
source "$argv[1]"; or exit 72
test "$PATH[1]" = "$EXPECTED_BIN"; or exit 73
printf '%s\0' "$EZA_CONFIG_DIR" "$LG_CONFIG_FILE"
fastfetch "$ARG_ONE" "$ARG_TWO"
"#,
        ])
        .arg(loader)
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    verify_output(&output.stdout, &env);
    assert_eq!(snapshot::tree(env.home()), before);
}
