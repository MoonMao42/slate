//! A missing Slate preset must not prevent using fastfetch's native defaults.
//! Paths arrive pre-quoted for the target shell; never execute anything at source time.
const INFORMATION_FLAGS: &[&str] = &[
    "-h",
    "--help",
    "-v",
    "--version",
    "--version-raw",
    "--list-config-paths",
    "--list-data-paths",
    "--list-logos",
    "--list-modules",
    "--list-presets",
    "--list-features",
];

pub(super) fn posix(content: &mut String, path: &str) {
    let information = INFORMATION_FLAGS.join("|");
    content.push_str(&format!(
        "fastfetch() {{\n  local _slate_ff_arg\n  for _slate_ff_arg in \"$@\"; do\n    case \"$_slate_ff_arg\" in\n      --) break ;;\n      -c|-c?*|--config|--config=*|{information}) command fastfetch \"$@\"; return ;;\n    esac\n  done\n  if [ -f {path} ] && [ -r {path} ] && [ ! -L {path} ]; then\n    command fastfetch -c {path} \"$@\"\n  else\n    command fastfetch \"$@\"\n  fi\n}}\n"
    ));
}

pub(super) fn fish(content: &mut String, path: &str) {
    let information = INFORMATION_FLAGS.join(" ");
    content.push_str(&format!(
        "function fastfetch\n  for _slate_ff_arg in $argv\n    switch $_slate_ff_arg\n      case --\n        break\n      case '-c*' --config '--config=*' {information}\n        command fastfetch $argv\n        return\n    end\n  end\n  if test -f {path}; and test -r {path}; and not test -L {path}\n    command fastfetch -c {path} $argv\n  else\n    command fastfetch $argv\n  end\nend\n"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        process::Command,
    };

    #[test]
    fn fastfetch_posix_wrapper_falls_back_and_preserves_arguments_and_exit_status() {
        for shell in ["/bin/bash", "/bin/zsh"] {
            verify_wrapper(shell, false);
        }
    }

    #[test]
    #[ignore = "requires explicit SLATE_TEST_FISH"]
    fn fastfetch_fish_wrapper_falls_back_and_preserves_arguments_and_exit_status() {
        verify_wrapper(
            &std::env::var("SLATE_TEST_FISH").expect("set Fish path explicitly"),
            true,
        );
    }

    #[test]
    #[ignore = "requires explicit SLATE_FASTFETCH_BINARY; fixed custom modules only"]
    fn fastfetch_native_explicit_config_can_override_the_wrapper_preset() {
        let binary = fs::canonicalize(
            std::env::var_os("SLATE_FASTFETCH_BINARY").expect("set native fastfetch explicitly"),
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("preset.jsonc");
        fs::write(
            &preset,
            r#"{"logo":{"type":"none"},"modules":[{"type":"custom","format":"PresetFixture"}]}"#,
        )
        .unwrap();
        let mut wrapper = String::new();
        posix(
            &mut wrapper,
            &crate::detection::shell_quote(preset.to_str().unwrap()),
        );
        let output = assert_cmd::Command::new("/bin/bash")
            .env_clear().env("HOME", temp.path()).env("PATH", binary.parent().unwrap()).current_dir(temp.path())
            .args(["--noprofile", "--norc", "-c", &format!("{wrapper}\nfastfetch --config - --pipe true")])
            .write_stdin(r#"{"logo":{"type":"none"},"modules":[{"type":"custom","format":"PersonalFixture"}]}"#)
            .timeout(std::time::Duration::from_secs(5)).assert().success().stderr("")
            .get_output().stdout.clone();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("PersonalFixture"), "{output:?}");
        assert!(!output.contains("PresetFixture"), "{output:?}");
    }

    #[test]
    #[ignore = "requires explicit SLATE_FASTFETCH_BINARY; native help and version only"]
    fn fastfetch_native_help_survives_a_broken_slate_preset() {
        let binary = fs::canonicalize(
            std::env::var_os("SLATE_FASTFETCH_BINARY").expect("set native fastfetch explicitly"),
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("broken.jsonc");
        fs::write(&preset, "{invalid fixture").unwrap();
        // Demonstrate the old injected-config failure before checking the fix.
        assert_cmd::Command::new(&binary)
            .env_clear()
            .env("HOME", temp.path())
            .arg("-c")
            .arg(&preset)
            .arg("--help")
            .timeout(std::time::Duration::from_secs(5))
            .assert()
            .failure();
        let mut wrapper = String::new();
        posix(
            &mut wrapper,
            &crate::detection::shell_quote(preset.to_str().unwrap()),
        );
        for flag in ["--help", "-h", "--version", "-v"] {
            let result = assert_cmd::Command::new("/bin/bash")
                .env_clear()
                .env("HOME", temp.path())
                .env("PATH", binary.parent().unwrap())
                .args([
                    "--noprofile",
                    "--norc",
                    "-c",
                    &format!("{wrapper}\nfastfetch {flag}"),
                ])
                .timeout(std::time::Duration::from_secs(5))
                .assert()
                .success()
                .stderr("");
            assert!(!result.get_output().stdout.is_empty());
        }
        assert_eq!(fs::read_to_string(&preset).unwrap(), "{invalid fixture");
    }

    fn verify_wrapper(shell: &str, is_fish: bool) {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("fastfetch");
        fs::write(&binary, "#!/bin/sh\nprintf '<%s>\\n' \"$@\"\nexit 23\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let path = temp.path().join("preset ' space.jsonc");
        let mut wrapper = String::new();
        if is_fish {
            fish(
                &mut wrapper,
                &crate::platform::shell::fish_quote(path.to_str().unwrap()),
            );
        } else {
            posix(
                &mut wrapper,
                &crate::detection::shell_quote(path.to_str().unwrap()),
            );
        }
        let run = |script: &str| {
            Command::new(shell)
                .env_clear()
                .env("HOME", temp.path())
                .env("PATH", temp.path())
                .args([if is_fish { "--no-config" } else { "-f" }, "-c", script])
                .output()
                .unwrap()
        };
        let source_only = run(&wrapper);
        assert!(source_only.status.success());
        assert!(source_only.stdout.is_empty());
        assert!(source_only.stderr.is_empty());
        let script = format!("{wrapper}\nfastfetch 'arg with spaces' ''");
        for state in ["missing", "regular", "directory", "symlink"] {
            match state {
                "regular" => fs::write(&path, "{}").unwrap(),
                "directory" => {
                    fs::remove_file(&path).unwrap();
                    fs::create_dir(&path).unwrap();
                }
                "symlink" => {
                    fs::remove_dir(&path).unwrap();
                    symlink(&binary, &path).unwrap();
                }
                _ => {}
            }
            let output = run(&script);
            assert_eq!(output.status.code(), Some(23), "{shell} {state}");
            assert!(output.stderr.is_empty());
            let prefix = if state == "regular" {
                format!("<-c>\n<{}>\n", path.display())
            } else {
                String::new()
            };
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                format!("{prefix}<arg with spaces>\n<>\n")
            );
            for flag in ["--config", "-c", "--config=personal", "-cpersonal"]
                .into_iter()
                .chain(INFORMATION_FLAGS.iter().copied())
            {
                let explicit = run(&format!(
                    "{wrapper}\nfastfetch {flag} 'personal file.jsonc'"
                ));
                assert_eq!(explicit.status.code(), Some(23));
                assert!(explicit.stderr.is_empty());
                assert_eq!(
                    String::from_utf8(explicit.stdout).unwrap(),
                    format!("<{flag}>\n<personal file.jsonc>\n")
                );
            }
        }
    }
}
