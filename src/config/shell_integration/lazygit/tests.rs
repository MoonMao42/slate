use super::*;
use crate::{
    adapter::{LazygitAdapter, ToolAdapter},
    env::SlateEnv,
    theme::ThemeRegistry,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::test_tree as snapshot;

struct Fixture {
    _temp: tempfile::TempDir,
    env: SlateEnv,
    source: PathBuf,
    managed: PathBuf,
    user: PathBuf,
    fish: bool,
}

impl Fixture {
    fn new(fish: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().join(r"home \\ ' $(literal) 中文:profile"));
        fs::create_dir_all(env.home()).unwrap();
        let managed_root = env.managed_file("managed");
        let managed = LazygitAdapter::theme_path(&env);
        let user = env.lazygit_default_config().to_owned();
        let source = env.home().join("env-source");
        let options = ShellIntegrationOptions {
            managed_root: managed_root.to_str().unwrap(),
            user_config_root: env.xdg_config_home().to_str().unwrap(),
            lazygit_default_config: user.to_str().unwrap(),
            user_local_bin: None,
            plain_starship_path: "",
            active_starship_path: "",
            notify_path: "",
            zsh_highlighting_plugin_path: None,
            homebrew_prefix: None,
            prefer_plain_starship: false,
            starship_enabled: false,
            zsh_highlighting_enabled: false,
            fastfetch_autorun: false,
            auto_theme_enabled: false,
        };
        let quote = if fish {
            crate::platform::shell::fish_quote
        } else {
            crate::detection::shell_quote
        };
        let config = Config::new(&options, quote);
        let mut text = String::new();
        if fish {
            config.fish(&mut text)
        } else {
            config.posix(&mut text)
        }
        fs::write(&source, text).unwrap();
        Self {
            _temp: temp,
            env,
            source,
            managed,
            user,
            fish,
        }
    }

    fn load(&self, binary: &Path, initial: Option<&str>) -> String {
        let mut cmd = assert_cmd::Command::new(binary);
        cmd.env_clear()
            .env("HOME", self.env.home())
            .env("PATH", "/usr/bin:/bin")
            .env("XDG_CONFIG_HOME", self.env.xdg_config_home())
            .env("XDG_DATA_HOME", self.env.xdg_data_home())
            .env("XDG_CACHE_HOME", self.env.cache_dir())
            .env("TERM", "dumb")
            .current_dir(self.env.home())
            .timeout(Duration::from_secs(8));
        if let Some(initial) = initial {
            cmd.env("LG_CONFIG_FILE", initial);
        }
        if self.fish {
            cmd.args([
                "--no-config",
                "--private",
                "-c",
                r#"
source "$argv[1]"; or exit 71
source "$argv[1]"; or exit 72
if set -q LG_CONFIG_FILE[1]
  printf '%s' "$LG_CONFIG_FILE"
else
  printf '__UNSET__'
end
"#,
            ]);
        } else {
            if binary.file_name().unwrap() == "bash" {
                cmd.args(["--noprofile", "--norc"]);
            } else {
                cmd.arg("-f");
            }
            cmd.args(["-c", r#"source "$1" || exit 71; source "$1" || exit 72; printf '%s' "${LG_CONFIG_FILE-__UNSET__}""#, "slate-lazygit-fixture"]);
        }
        let output = cmd
            .arg(&self.source)
            .assert()
            .success()
            .stderr("")
            .get_output()
            .stdout
            .clone();
        String::from_utf8(output).unwrap()
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn exercise(binary: &Path, fish: bool) {
    let f = Fixture::new(fish);
    // Establish private native-shell caches before observing read-only loading.
    assert_eq!(f.load(binary, None), "__UNSET__");
    for state in 0..5 {
        match state {
            1 => write(&f.managed, "gui: {}\n"),
            2 => write(&f.user, "gui:\n  scrollHeight: 7\n"),
            3 => fs::remove_file(&f.managed).unwrap(),
            4 => {
                write(&f.managed, "gui: {}\n");
                fs::remove_file(&f.user).unwrap();
                std::os::unix::fs::symlink(f.env.home().join("missing.yml"), &f.user).unwrap();
            }
            _ => (),
        }
        let expected = match state {
            1 => f.managed.display().to_string(),
            2 => format!("{},{}", f.managed.display(), f.user.display()),
            _ => "__UNSET__".into(),
        };
        let before = snapshot::tree(f.env.home());
        let legacy = format!("{}:{}", f.managed.display(), f.user.display());
        let merged = format!("{},{}", f.managed.display(), f.user.display());
        for initial in [
            None,
            Some(""),
            f.managed.to_str(),
            Some(legacy.as_str()),
            Some(merged.as_str()),
        ] {
            assert_eq!(
                f.load(binary, initial),
                expected,
                "state {state}, {initial:?}"
            );
        }
        for custom in [
            "/tmp/my:profile.yml",
            "/tmp/one.yml,/tmp/two.yml",
            "custom,missing.yml",
        ] {
            assert_eq!(f.load(binary, Some(custom)), custom);
        }
        assert_eq!(snapshot::tree(f.env.home()), before);
    }
}

#[test]
fn lazygit_posix_startup_handles_missing_files_legacy_lists_and_custom_overrides() {
    for name in ["bash", "zsh"] {
        let binary = match which::which(name) {
            Ok(path) => path,
            Err(_) if name == "zsh" => {
                eprintln!("Zsh unavailable: native coverage not run");
                continue;
            }
            Err(err) => panic!("Bash is required: {err}"),
        };
        exercise(&binary, false);
    }
}

#[cfg(feature = "has-fish")]
#[test]
fn lazygit_fish_startup_handles_missing_files_legacy_lists_and_custom_overrides() {
    let binary = std::env::var_os("SLATE_TEST_FISH")
        .map(PathBuf::from)
        .or_else(|| which::which("fish").ok())
        .expect("has-fish needs a native Fish binary");
    exercise(&fs::canonicalize(binary).unwrap(), true);
}

#[test]
#[ignore = "requires explicit SLATE_LAZYGIT_BINARY; native parser only, no live UI"]
fn lazygit_native_loads_every_generated_palette_and_shell_chain() {
    let binary = fs::canonicalize(
        std::env::var_os("SLATE_LAZYGIT_BINARY").expect("set native binary explicitly"),
    )
    .unwrap();
    let f = Fixture::new(false);
    let bash = which::which("bash").unwrap();
    let personal = "gui:\n  scrollHeight: 7\ngit:\n  autoFetch: false\n";
    write(&f.user, personal);
    let run = |chain: &str| {
        let mut cmd = assert_cmd::Command::new(&binary);
        cmd.env_clear()
            .env("HOME", f.env.home())
            .env("PATH", "/usr/bin:/bin")
            .env("XDG_CONFIG_HOME", f.env.xdg_config_home())
            .env("TERM", "dumb")
            .env("LG_CONFIG_FILE", chain)
            .current_dir(f.env.home())
            .timeout(Duration::from_secs(8));
        let output = cmd.assert().code(1).get_output().clone();
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    };
    // Negative control: the old scalar fragment must fail in the native parser.
    write(
        &f.managed,
        "gui:\n  theme:\n    activeBorderColor: '#89b4fa'\n",
    );
    let bad = run(&f.load(&bash, None));
    assert!(
        bad.contains("cannot unmarshal") && bad.contains("[]string"),
        "{bad}"
    );
    for theme in ThemeRegistry::new().unwrap().all() {
        LazygitAdapter.apply_theme_with_env(theme, &f.env).unwrap();
        let result = run(&f.load(&bash, None));
        assert!(
            !result.contains("couldn't be parsed"),
            "{}: {result}",
            theme.id
        );
        assert!(
            result.contains("Not in a git repository"),
            "{}: {result}",
            theme.id
        );
        assert_eq!(fs::read_to_string(&f.user).unwrap(), personal);
        assert!(!f.env.home().join(".git").exists());
    }
    fs::remove_file(&f.user).unwrap();
    let result = run(&f.load(&bash, None));
    assert!(result.contains("Not in a git repository"), "{result}");
}
