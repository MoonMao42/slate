use slate_cli::{env::SlateEnv, theme::ThemeRegistry};
use std::fs::{self, File};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::{fs::PermissionsExt, net::UnixStream};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;

#[path = "support/tree.rs"]
mod tree_snapshot;
use tree_snapshot::tree;
#[path = "support/redirected_output.rs"]
mod redirected_output;

#[cfg(feature = "has-fish")]
#[path = "completions/fish.rs"]
mod native_fish;

fn generate(home: Option<&Path>, shell: &str) -> Vec<u8> {
    let mut command = assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command.env_clear().env("PATH", "");
    if let Some(home) = home {
        command.env("SLATE_HOME", home);
    }
    let output = command
        .args(["completions", shell])
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    assert!(!output.stdout.contains(&0x1b));
    output.stdout
}

#[test]
fn completions_are_static_read_only_and_available_without_a_valid_profile() {
    use assert_cmd::assert::OutputAssertExt;
    let td = TempDir::new().unwrap();
    let before = tree(td.path());
    let scripts: Vec<_> = ["bash", "zsh", "fish"]
        .into_iter()
        .map(|shell| (shell, generate(Some(td.path()), shell)))
        .collect();
    assert_eq!(tree(td.path()), before);
    let launches_slate =
        regex::Regex::new(r"(?m)^\s*(?:command\s+)?slate(?:\s|$)|\$?\(\s*slate\s|`\s*slate\s")
            .unwrap();
    for (shell, script) in &scripts {
        assert_eq!(generate(None, shell), *script, "HOME affected generation");
        let text = String::from_utf8(script.clone()).unwrap();
        assert!(text.len() > 1_000);
        for id in ThemeRegistry::new().unwrap().list_ids() {
            assert!(text.contains(&id), "{shell}: missing {id}");
        }
        for value in [
            "about",
            "theme",
            "recover",
            "completions",
            "appearance",
            "light",
            "dark",
            "json",
            "ids",
            "opencode",
            "check-version",
            "files-only",
        ] {
            assert!(text.contains(value), "{shell}: missing {value}");
        }
        assert!(!text.contains("__watch-auto-theme") && !text.contains("__subcmd__reset"));
        // Help descriptions contain phrases like 'slate list commands'; only
        // actual command invocation/substitution syntax would launch Slate.
        assert!(!launches_slate.is_match(&text), "{shell} launches Slate");
        assert!(!text.contains(td.path().to_str().unwrap()));
    }
    let env = SlateEnv::with_home(td.path().to_owned());
    fs::create_dir_all(env.config_dir()).unwrap();
    fs::create_dir_all(env.slate_cache_dir()).unwrap();
    let config = std::ffi::CString::new(
        env.managed_file("config.toml")
            .as_os_str()
            .as_encoded_bytes(),
    )
    .unwrap();
    assert_eq!(unsafe { libc::mkfifo(config.as_ptr(), 0o600) }, 0);
    fs::write(
        env.slate_cache_dir().join("preview-session.json"),
        b"PRIVATE_BAD_RECOVERY",
    )
    .unwrap();
    let lock = File::create(env.slate_cache_dir().join("preview-session.lock")).unwrap();
    lock.set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let before = tree(td.path());
    for (shell, script) in &scripts {
        assert_eq!(generate(Some(td.path()), shell), *script);
    }
    assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
        .env_clear()
        .env("HOME", td.path())
        .env("NVIM_APPNAME", "/invalid")
        .args(["--quiet", "completions", "bash", "--auto"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(scripts[0].1.clone());
    for args in [vec!["completions"], vec!["completions", "unknown-shell"]] {
        assert_cmd::Command::new(assert_cmd::cargo::cargo_bin!("slate"))
            .env_clear()
            .env("SLATE_HOME", td.path())
            .args(&args)
            .timeout(Duration::from_secs(5))
            .assert()
            .failure();
    }
    // A consumer closing stdout must not turn generation into a panic.
    let (consumer, producer) = UnixStream::pair().unwrap();
    drop(consumer);
    let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin!("slate"));
    command
        .env_clear()
        .args(["completions", "bash"])
        .stdout(Stdio::from(OwnedFd::from(producer)))
        .stderr(Stdio::piped());
    redirected_output::run(&mut command)
        .assert()
        .success()
        .stderr("");
    assert_eq!(tree(td.path()), before);
}

#[test]
fn bash_completion_offers_contextual_candidates_without_running_slate() {
    let td = tempfile::Builder::new()
        .prefix("slate completion ' ")
        .tempdir()
        .unwrap();
    let script = td.path().join("slate.bash");
    fs::write(&script, generate(None, "bash")).unwrap();
    let before = tree(td.path());
    assert_cmd::Command::new("/bin/bash")
        .env_clear()
        .env("HOME", td.path())
        .args(["--noprofile", "--norc", "-n"])
        .arg(&script)
        .timeout(Duration::from_secs(5))
        .assert()
        .success();
    for (words, expected) in [
        (vec!["slate", "ab"], vec!["about"]),
        (vec!["slate", "about", "--j"], vec!["--json"]),
        (vec!["slate", "th"], vec!["theme"]),
        (
            vec!["slate", "--quiet", "theme", "catp"],
            vec![
                "catppuccin-mocha",
                "catppuccin-latte",
                "catppuccin-frappe",
                "catppuccin-macchiato",
            ],
        ),
        (
            vec!["slate", "theme", "set", "rose"],
            vec!["rose-pine-main", "rose-pine-moon", "rose-pine-dawn"],
        ),
        (vec!["slate", "list", "--appearance", "li"], vec!["light"]),
        (vec!["slate", "tools", "sy"], vec!["sync"]),
        (vec!["slate", "tools", "sync", "bt"], vec!["btop"]),
        (vec!["slate", "tools", "sync", "ya"], vec!["yazi"]),
        (vec!["slate", "tools", "sync", "ze"], vec!["zellij"]),
        (vec!["slate", "tools", "in"], vec!["info", "install"]),
        (vec!["slate", "tools", "info", "ya"], vec!["yazi"]),
        (vec!["slate", "tools", "info", "ze"], vec!["zellij"]),
        (vec!["slate", "tools", "ins"], vec!["install"]),
        (vec!["slate", "tools", "install", "bt"], vec!["btop"]),
        (vec!["slate", "tools", "install", "ze"], vec!["zellij"]),
        (vec!["slate", "tools", "install", "nv"], vec![]),
        (vec!["slate", "prompt", "comp"], vec!["compact"]),
        (vec!["slate", "prompt", "cla"], vec!["classic"]),
        (vec!["slate", "doctor", "bt"], vec!["btop"]),
        (
            vec!["slate", "doctor", "ghostty", "--files"],
            vec!["--files-only"],
        ),
        (vec!["slate", "doctor", "ya"], vec!["yazi"]),
        (vec!["slate", "doctor", "ze"], vec!["zellij"]),
        (vec!["slate", "doctor", "la"], vec!["lazygit"]),
        (vec!["slate", "doctor", "ez"], vec!["eza"]),
        (vec!["slate", "doctor", "sta"], vec!["starship"]),
        (
            vec!["slate", "config", "pairing", "--light", "catp"],
            vec!["catppuccin-latte"],
        ),
        (
            vec!["slate", "config", "pairing", "--dark", "rose"],
            vec!["rose-pine-main", "rose-pine-moon"],
        ),
        (vec!["slate", "doctor", "op"], vec!["opencode", "opacity"]),
        (
            vec!["slate", "doctor", "--json", "op"],
            vec!["opencode", "opacity"],
        ),
        (vec!["slate", "doctor", "opa"], vec!["opacity"]),
        (vec!["slate", "doctor", "fo"], vec!["font"]),
        (vec!["slate", "doctor", "ba"], vec!["bash"]),
        (vec!["slate", "doctor", "fi"], vec!["fish"]),
        (vec!["slate", "config", "g"], vec!["get"]),
        (vec!["slate", "config", "get", "so"], vec!["sound"]),
        (vec!["slate", "config", "set", "ed"], vec!["editor"]),
        (
            vec!["slate", "config", "get", "--json", "fa"],
            vec!["fastfetch"],
        ),
        (vec!["slate", "config", "list", "--j"], vec!["--json"]),
        (
            vec!["slate", "config", "pairing", "--clear-"],
            vec!["--clear-dark", "--clear-light"],
        ),
        (
            vec![
                "slate",
                "config",
                "pairing",
                "--clear-dark",
                "--light",
                "catp",
            ],
            vec!["catppuccin-latte"],
        ),
        (vec!["slate", "font", "--li"], vec!["--list"]),
        (vec!["slate", "font", "--list", "--j"], vec!["--json"]),
        (vec!["slate", "font", "--list", "--se"], vec!["--search"]),
        (
            vec!["slate", "doctor", "nvim", "--check"],
            vec!["--check-version"],
        ),
        (vec!["slate", "recover", "--dry"], vec!["--dry-run"]),
        (vec!["slate", "clean", "--dry"], vec!["--dry-run"]),
        (vec!["slate", "clean", "--dry-run", "--j"], vec!["--json"]),
        (
            vec!["slate", "restore", "--list", "--a"],
            vec!["--all", "--auto"],
        ),
        (vec!["slate", "completions", "f"], vec!["fish"]),
    ] {
        let output = assert_cmd::Command::new("/bin/bash")
            .env_clear()
            .env("HOME", td.path())
            .env("PATH", "")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                r#"
slate() { printf 'UNEXPECTED SLATE INVOCATION\n' >&2; return 91; }
source "$1"
shift
COMP_WORDS=("$@")
COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
_slate slate "${COMP_WORDS[COMP_CWORD]}" "${COMP_WORDS[COMP_CWORD-1]}"
# printf with no arguments emits a blank line, not an actual empty candidate.
if [ "${#COMPREPLY[@]}" -gt 0 ]; then
    printf '%s\n' "${COMPREPLY[@]}"
fi
"#,
                "completion-fixture",
            ])
            .arg(&script)
            .args(&words)
            .timeout(Duration::from_secs(5))
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(output.stderr.is_empty(), "{words:?}: {:?}", output.stderr);
        let text = String::from_utf8(output.stdout).unwrap();
        let mut actual: Vec<_> = text.lines().collect();
        let mut expected = expected;
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "{words:?}");
    }
    assert_eq!(tree(td.path()), before, "sourcing/completing wrote files");
}

#[test]
#[cfg(target_os = "macos")]
fn zsh_completion_parses_and_registers_without_startup_files_or_cache_writes() {
    let td = TempDir::new().unwrap();
    let script = td.path().join("_slate");
    fs::write(&script, generate(None, "zsh")).unwrap();
    let before = tree(td.path());
    assert_cmd::Command::new("/bin/zsh")
        .env_clear()
        .env("HOME", td.path())
        .args(["-f", "-n"])
        .arg(&script)
        .timeout(Duration::from_secs(5))
        .assert()
        .success();
    assert_cmd::Command::new("/bin/zsh")
        .env_clear()
        .env("HOME", td.path())
        .env("ZDOTDIR", td.path())
        .env("PATH", "")
        .args([
            "-f",
            "-c",
            r#"
slate() { print -u2 'UNEXPECTED SLATE INVOCATION'; return 91; }
autoload -Uz compinit
compinit -D -i
source "$1"
source "$1"
[[ "${_comps[slate]}" == _slate ]] || exit 92
(( $+functions[_slate] )) || exit 93
"#,
            "completion-fixture",
        ])
        .arg(&script)
        .timeout(Duration::from_secs(8))
        .assert()
        .success()
        .stdout("")
        .stderr("");
    assert_eq!(tree(td.path()), before);
}
