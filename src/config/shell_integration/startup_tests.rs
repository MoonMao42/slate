//! Native shells load only generated fixture files; every optional tool is a
//! private stub. No real prompt/plugin/watcher or user startup file is executed.
use super::*;
use crate::env::SlateEnv;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::test_tree as snapshot;

struct Fixture {
    _temp: tempfile::TempDir,
    env: SlateEnv,
    probes: PathBuf,
    starship: bool,
}

impl Fixture {
    fn new(starship: bool) -> Self {
        Self::with_autorun(starship, true)
    }

    fn with_autorun(starship: bool, fastfetch_autorun: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().join("private home ' 中文"));
        let managed = env.config_dir().join("managed");
        let bin = env.user_local_bin();
        let active = env.xdg_config_home().join("starship.toml");
        let plain = managed.join("starship/plain.toml");
        let notify = managed.join("bin/slate-dark-mode-notify");
        let plugin = env.home().join("private-highlighting.zsh");
        let probes = temp.path().join("probes");
        fs::create_dir(&probes).unwrap();
        write(&bin.join("fastfetch"), "#!/bin/sh\nprintf 'called\\n' >> \"$SLATE_TEST_PROBES/fastfetch\"\nprintf 'AUTORUN_NOISE\\n'\n", true);
        write(&bin.join("starship"), "#!/bin/sh\nprintf 'called\\n' >> \"$SLATE_TEST_PROBES/starship\"\nif [ \"$2\" = fish ]; then\n  printf 'set -g _SLATE_TEST_PROMPT_LOADED yes\\n'\nelse\n  printf '_SLATE_TEST_PROMPT_LOADED=yes\\n'\nfi\n", true);
        write(
            &notify,
            "#!/bin/sh\nprintf 'called\\n' >> \"$SLATE_TEST_PROBES/notify\"\n",
            true,
        );
        write(
            &plugin,
            "_SLATE_TEST_PLUGIN_RUNS=$(( ${_SLATE_TEST_PLUGIN_RUNS:-0} + 1 ))\n",
            false,
        );
        write(
            &managed.join("zsh/highlight-styles.sh"),
            "_SLATE_TEST_STYLES_RUNS=$(( ${_SLATE_TEST_STYLES_RUNS:-0} + 1 ))\n",
            false,
        );
        write(&active, "# private active config\n", false);
        let files = build_shell_integration_files(
            &crate::theme::catppuccin::catppuccin_mocha().unwrap(),
            &ShellIntegrationOptions {
                managed_root: managed.to_str().unwrap(),
                user_config_root: env.xdg_config_home().to_str().unwrap(),
                lazygit_default_config: env.lazygit_default_config().to_str().unwrap(),
                user_local_bin: Some(bin.to_str().unwrap()),
                plain_starship_path: plain.to_str().unwrap(),
                active_starship_path: active.to_str().unwrap(),
                notify_path: notify.to_str().unwrap(),
                zsh_highlighting_plugin_path: Some(plugin.to_str().unwrap()),
                homebrew_prefix: None,
                prefer_plain_starship: false,
                starship_enabled: starship,
                zsh_highlighting_enabled: true,
                fastfetch_autorun,
                auto_theme_enabled: true,
            },
        );
        for (name, content) in [
            ("bash", files.bash),
            ("zsh", files.zsh),
            ("fish", files.fish),
        ] {
            write(&managed.join(format!("shell/env.{name}")), &content, false);
        }
        Self {
            _temp: temp,
            env,
            probes,
            starship,
        }
    }

    fn source(&self, name: &str) -> PathBuf {
        self.env
            .config_dir()
            .join(format!("managed/shell/env.{name}"))
    }

    fn command(&self, binary: &Path) -> assert_cmd::Command {
        let mut command = assert_cmd::Command::new(binary);
        command
            .env_clear()
            .env(
                "LC_ALL",
                if cfg!(target_os = "macos") {
                    "en_US.UTF-8"
                } else {
                    "C.UTF-8"
                },
            )
            .env("HOME", self.env.home())
            .env("XDG_CONFIG_HOME", self.env.xdg_config_home())
            .env("XDG_CACHE_HOME", self.env.cache_dir())
            .env("XDG_DATA_HOME", self.env.xdg_data_home())
            .env("PATH", self.env.user_local_bin())
            .env("TERM", "dumb")
            .env("TERM_PROGRAM", "Ghostty")
            .env("USER", "fixture")
            .env("HISTFILE", "/dev/null")
            .env("HISTSIZE", "0")
            .env("SAVEHIST", "0")
            .env("SLATE_TEST_PROBES", &self.probes)
            .env("EXPECTED_EZA", self.env.config_dir().join("managed/eza"))
            .env("EXPECTED_PATH", self.env.user_local_bin())
            .env("STARSHIP_CONFIG", "PRIVATE_STARSHIP")
            .env(
                "EXPECTED_STARSHIP",
                if self.starship {
                    self.env.xdg_config_home().join("starship.toml")
                } else {
                    PathBuf::from("PRIVATE_STARSHIP")
                },
            )
            .current_dir(self.env.home())
            .timeout(Duration::from_secs(5));
        command
    }

    fn check_calls(&self, interactive: bool) {
        // Disowned Zsh helpers are intentionally not covered by shell `wait`.
        // Wait for the private stub receipt, not for any host process.
        if interactive {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while fs::read_to_string(self.probes.join("notify")).unwrap_or_default()
                != "called\n".repeat(2)
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        for name in ["fastfetch", "notify", "starship"] {
            let expected = if interactive && (name != "starship" || self.starship) {
                2
            } else {
                0
            };
            let path = self.probes.join(name);
            if expected == 0 {
                assert!(!path.exists(), "unexpected {name} call");
            } else {
                assert_eq!(
                    fs::read_to_string(path).unwrap(),
                    "called\n".repeat(expected)
                );
            }
        }
    }
}

#[test]
fn zsh_helper_startup_has_no_job_notices_in_a_real_pty() {
    let (Ok(zsh), Ok(python)) = (which::which("zsh"), which::which("python3")) else {
        eprintln!("Zsh/Python unavailable; native helper PTY check not run");
        return;
    };
    let fixture = Fixture::with_autorun(false, false);
    let source = fixture.source("zsh");
    let fixed = fs::read_to_string(&source).unwrap();
    let run = || {
        fixture
            .command(&python)
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/zsh_helper_pty.py"
            ))
            .arg(&zsh)
            .arg(&source)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    // Prove the driver detects the original bug, not just a quiet non-TTY shell.
    fs::write(&source, fixed.replace("2>&1 &!\n", "2>&1 &\n")).unwrap();
    let original = String::from_utf8(run()).unwrap();
    assert!(
        regex::Regex::new(r"\[\d+\]").unwrap().is_match(&original),
        "{original}"
    );
    fs::remove_file(fixture.probes.join("notify")).unwrap();
    fs::write(&source, fixed).unwrap();
    let before = snapshot::tree(fixture.env.home());
    let repaired = String::from_utf8(run()).unwrap();
    assert_eq!(repaired.trim(), "SLATE_PTY_DONE");
    assert_eq!(
        fs::read_to_string(fixture.probes.join("notify")).unwrap(),
        "called\n"
    );
    assert_eq!(snapshot::tree(fixture.env.home()), before);
}

#[test]
fn zsh_failed_noisy_helper_does_not_pollute_interactive_startup() {
    let (Ok(zsh), Ok(python)) = (which::which("zsh"), which::which("python3")) else {
        eprintln!("Zsh/Python unavailable; native failure-noise PTY check not run");
        return;
    };
    let fixture = Fixture::with_autorun(false, false);
    write(
        &fixture.env.config_dir().join("managed/bin/slate-dark-mode-notify"),
        "#!/bin/sh\nprintf 'called\\n' >> \"$SLATE_TEST_PROBES/notify\"\nprintf 'HELPER_STDOUT\\n'\nprintf 'HELPER_STDERR\\n' >&2\nexit 7\n",
        true,
    );
    let before = snapshot::tree(fixture.env.home());
    let result = fixture
        .command(&python)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/zsh_helper_pty.py"
        ))
        .arg(&zsh)
        .arg(fixture.source("zsh"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(String::from_utf8(result).unwrap().trim(), "SLATE_PTY_DONE");
    assert_eq!(
        fs::read_to_string(fixture.probes.join("notify")).unwrap(),
        "called\n"
    );
    assert_eq!(snapshot::tree(fixture.env.home()), before);
}

fn write(path: &Path, content: &str, executable: bool) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if executable { 0o700 } else { 0o600 }),
    )
    .unwrap();
}

#[test]
fn shell_startup_disabled_fastfetch_stays_quiet_when_sourced_twice_and_remains_callable() {
    for name in ["bash", "zsh"] {
        let Some(binary) = which::which(name).ok() else {
            assert_eq!(name, "zsh", "Bash is required");
            eprintln!("Zsh unavailable; disabled-autorun check was not run for Zsh");
            continue;
        };
        let fixture = Fixture::with_autorun(false, false);
        let before = snapshot::tree(fixture.env.home());
        let mut command = fixture.command(&binary);
        if name == "bash" {
            command.args(["--noprofile", "--norc"]);
        } else {
            command.arg("-f");
        }
        let output = command
            .args([
                "-i",
                "-c",
                r#"
source "$1" || exit 71
source "$1" || exit 72
wait
[ ! -e "$SLATE_TEST_PROBES/fastfetch" ] || exit 73
printf 'manual-start\n'
fastfetch --help || exit 74
"#,
                "private-shell-startup",
            ])
            .arg(fixture.source(name))
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "manual-start\nAUTORUN_NOISE\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.probes.join("fastfetch")).unwrap(),
            "called\n"
        );
        assert_eq!(snapshot::tree(fixture.env.home()), before);
    }
}

#[test]
#[ignore = "requires explicit SLATE_TEST_FISH; generated integration and private stubs only"]
fn shell_startup_disabled_fastfetch_fish_stays_quiet_and_remains_callable() {
    let binary =
        fs::canonicalize(std::env::var_os("SLATE_TEST_FISH").expect("set Fish explicitly"))
            .unwrap();
    let fixture = Fixture::with_autorun(false, false);
    let run = || {
        let mut command = fixture.command(&binary);
        command.args(["--no-config", "--private", "-i"]);
        command
    };
    run().args(["-c", "true"]).assert().success();
    let before = snapshot::tree(fixture.env.home());
    let output = run()
        .args([
            "-c",
            r#"
source "$argv[1]"; or exit 71
source "$argv[1]"; or exit 72
wait
test ! -e "$SLATE_TEST_PROBES/fastfetch"; or exit 73
printf 'manual-start\n'
fastfetch --help; or exit 74
"#,
        ])
        .arg(fixture.source("fish"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "manual-start\nAUTORUN_NOISE\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.probes.join("fastfetch")).unwrap(),
        "called\n"
    );
    assert_eq!(snapshot::tree(fixture.env.home()), before);
}

#[test]
fn shell_startup_noninteractive_loads_are_quiet_but_interactive_features_still_run() {
    for name in ["bash", "zsh"] {
        let Some(binary) = which::which(name).ok() else {
            assert_eq!(name, "zsh", "Bash is required");
            eprintln!("Zsh unavailable; its native startup check was not run");
            continue;
        };
        for interactive in [false, true] {
            for starship in [false, true] {
                let fixture = Fixture::new(starship);
                let before = snapshot::tree(fixture.env.home());
                let mut command = fixture.command(&binary);
                if name == "bash" {
                    command.args(["--noprofile", "--norc"]);
                } else {
                    command.arg("-f");
                }
                if interactive {
                    command.arg("-i");
                }
                let output = command.args(["-c", r#"
PS1=PRIVATE_PROMPT; PROMPT=PRIVATE_PROMPT
source "$1" || exit 71
source "$1" || exit 72
wait
[ "$EZA_CONFIG_DIR" = "$EXPECTED_EZA" ] || exit 73
[ "$PATH" = "$EXPECTED_PATH" ] || exit 74
[ "$STARSHIP_CONFIG" = "$EXPECTED_STARSHIP" ] || exit 75
if [ "$PS1" = PRIVATE_PROMPT ]; then result=kept; else result=changed; fi
printf 'state:%s/%s/%s/%s\n' "${_SLATE_TEST_PROMPT_LOADED:-no}" "${_SLATE_TEST_PLUGIN_RUNS:-0}" "${_SLATE_TEST_STYLES_RUNS:-0}" "$result"
"#, "private-shell-startup"]).arg(fixture.source(name)).assert().success().get_output().clone();
                let loaded = if interactive && starship { "yes" } else { "no" };
                let highlighting = if interactive && name == "zsh" { 2 } else { 0 };
                let prompt = if interactive && !starship {
                    "changed"
                } else {
                    "kept"
                };
                let state = format!("state:{loaded}/{highlighting}/{highlighting}/{prompt}\n");
                let stdout = String::from_utf8(output.stdout).unwrap();
                assert_eq!(
                    stdout,
                    format!(
                        "{}{state}",
                        "AUTORUN_NOISE\n".repeat(if interactive { 2 } else { 0 })
                    ),
                    "{name}/{interactive}/{starship}"
                );
                if !interactive {
                    assert!(
                        output.stderr.is_empty(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                fixture.check_calls(interactive);
                assert_eq!(snapshot::tree(fixture.env.home()), before);
            }
        }
    }
}

#[test]
fn shell_startup_renderers_keep_shared_config_outside_interactive_guards() {
    let fixture = Fixture::new(true);
    for (name, guard) in [
        ("bash", "case $- in\n*i*)"),
        ("zsh", "case $- in\n*i*)"),
        ("fish", "if status is-interactive"),
    ] {
        let file = fs::read_to_string(fixture.source(name)).unwrap();
        let guard = file.find(guard).unwrap();
        for shared in [
            "EZA_CONFIG_DIR",
            "STARSHIP_CONFIG",
            "fastfetch()",
            "function fastfetch",
        ] {
            if let Some(position) = file.find(shared) {
                assert!(position < guard, "{name}/{shared}");
            }
        }
        for effect in ["starship init", "  fastfetch\n", " >/dev/null 2>&1 &"] {
            assert!(file.find(effect).unwrap() > guard, "{name}/{effect}");
        }
        // Wrapper functions before this guard legitimately return. The startup
        // guard itself must not terminate the caller when sourced.
        let startup = &file[guard..];
        assert!(!startup.contains("return\n") && !startup.contains("exit\n"));
    }
}

#[cfg(feature = "has-fish")]
#[test]
fn shell_startup_native_fish_keeps_scripts_quiet_and_interactive_features_enabled() {
    let binary = std::env::var_os("SLATE_TEST_FISH")
        .map(PathBuf::from)
        .or_else(|| which::which("fish").ok())
        .expect("has-fish requires Fish or SLATE_TEST_FISH");
    let binary = fs::canonicalize(binary).unwrap();
    for interactive in [false, true] {
        for starship in [false, true] {
            let fixture = Fixture::new(starship);
            let run = || {
                let mut command = fixture.command(&binary);
                command.args(["--no-config", "--private"]);
                if interactive {
                    command.arg("-i");
                }
                command
            };
            // Fish may create private caches without loading any user config.
            run().args(["-c", "true"]).assert().success();
            let before = snapshot::tree(fixture.env.home());
            let output = run()
                .args([
                    "-c",
                    r#"
function fish_prompt; printf PRIVATE_PROMPT; end
source "$argv[1]"; or exit 71
source "$argv[1]"; or exit 72
wait
test "$EZA_CONFIG_DIR" = "$EXPECTED_EZA"; or exit 73
test "$PATH[1]" = "$EXPECTED_PATH"; or exit 74
test "$STARSHIP_CONFIG" = "$EXPECTED_STARSHIP"; or exit 75
set -l loaded no
if set -q _SLATE_TEST_PROMPT_LOADED; set loaded $_SLATE_TEST_PROMPT_LOADED; end
set -l result changed
set -l actual_prompt (fish_prompt | string collect)
if test "$actual_prompt" = PRIVATE_PROMPT; set result kept; end
printf 'state:%s/%s\n' "$loaded" "$result"
"#,
                ])
                .arg(fixture.source("fish"))
                .assert()
                .success()
                .get_output()
                .clone();
            let loaded = if interactive && starship { "yes" } else { "no" };
            let prompt = if interactive && !starship {
                "changed"
            } else {
                "kept"
            };
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                format!(
                    "{}state:{loaded}/{prompt}\n",
                    "AUTORUN_NOISE\n".repeat(if interactive { 2 } else { 0 })
                )
            );
            if !interactive {
                assert!(
                    output.stderr.is_empty(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            fixture.check_calls(interactive);
            assert_eq!(snapshot::tree(fixture.env.home()), before);
        }
    }
}
