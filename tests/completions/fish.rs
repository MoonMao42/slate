use super::{generate, tree};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn fish_binary() -> PathBuf {
    let path = std::env::var_os("SLATE_TEST_FISH")
        .map(PathBuf::from)
        .or_else(|| which::which("fish").ok())
        .expect("has-fish requires Fish on PATH or an explicit SLATE_TEST_FISH executable");
    fs::canonicalize(path).expect("resolve the native Fish executable")
}

fn command(binary: &Path, home: &Path) -> assert_cmd::Command {
    let mut command = assert_cmd::Command::new(binary);
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env(
            "SLATE_COMPLETION_MARKER",
            home.join("unexpected-slate-invocation"),
        )
        .args(["--no-config", "--private"])
        .timeout(Duration::from_secs(5));
    command
}

fn candidates(binary: &Path, home: &Path, script: &Path, input: &str) -> Vec<String> {
    let output = command(binary, home)
        .args([
            "-c",
            r#"
function slate
    printf 'UNEXPECTED SLATE INVOCATION\n' >> "$SLATE_COMPLETION_MARKER"
    return 91
end
source "$argv[1]"
source "$argv[1]"
complete --do-complete "$argv[2]"
"#,
        ])
        .arg(script)
        .arg(input)
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        output.stderr.is_empty(),
        "{input:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !home.join("unexpected-slate-invocation").exists(),
        "completion invoked Slate"
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut values: Vec<_> = text
        .lines()
        .map(|line| line.split('\t').next().unwrap().to_owned())
        .collect();
    values.sort();
    values
}

#[test]
fn fish_completion_executes_contexts_without_launching_slate_or_writing_config() {
    let binary = fish_binary();
    let td = tempfile::Builder::new()
        .prefix("slate fish ' ")
        .tempdir()
        .unwrap();
    // Fish itself seeds config/cache directories on first startup, even with
    // --no-config. Establish that control baseline before loading any Slate code.
    command(&binary, td.path())
        .args(["-c", "true"])
        .assert()
        .success()
        .stdout("")
        .stderr("");
    let script = td.path().join("slate.fish");
    fs::write(&script, generate(None, "fish")).unwrap();
    let before = tree(td.path());
    command(&binary, td.path())
        .arg("--no-execute")
        .arg(&script)
        .assert()
        .success();
    for (input, expected) in [
        ("slate th", vec!["theme"]),
        (
            "slate theme catp",
            vec![
                "catppuccin-mocha",
                "catppuccin-latte",
                "catppuccin-frappe",
                "catppuccin-macchiato",
            ],
        ),
        (
            "slate --quiet theme set ro",
            vec!["rose-pine-main", "rose-pine-moon", "rose-pine-dawn"],
        ),
        (
            "slate theme --quiet set ro",
            vec!["rose-pine-main", "rose-pine-moon", "rose-pine-dawn"],
        ),
        (
            "slate set ro",
            vec!["rose-pine-main", "rose-pine-moon", "rose-pine-dawn"],
        ),
        ("slate list --appearance li", vec!["light"]),
        ("slate list --appearance=li", vec!["--appearance=light"]),
        (
            "slate list --appearance light catp",
            vec![
                "catppuccin-mocha",
                "catppuccin-latte",
                "catppuccin-frappe",
                "catppuccin-macchiato",
            ],
        ),
        ("slate completions f", vec!["fish"]),
        ("slate recover --dry", vec!["--dry-run"]),
    ] {
        let mut expected = expected;
        expected.sort();
        assert_eq!(
            candidates(&binary, td.path(), &script, input),
            expected,
            "{input:?}"
        );
    }
    let theme_ids = slate_cli::theme::ThemeRegistry::new().unwrap().list_ids();
    for input in [
        "slate theme nord ",
        "slate theme set nord ",
        "slate theme --list ",
        "slate theme --auto ",
        "slate set --auto ",
        "slate --auto theme ",
        "slate --auto set ",
        "slate theme set --auto ",
        "slate theme --list set ",
        "slate list --appearance ",
    ] {
        let offered = candidates(&binary, td.path(), &script, input);
        assert!(
            !offered.iter().any(|item| theme_ids.contains(item)),
            "{input:?}: unexpected theme IDs {offered:?}"
        );
    }
    let after = tree(td.path());
    let changed: std::collections::BTreeSet<_> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect();
    assert!(
        changed.is_empty(),
        "Fish completion changed fixture paths: {changed:?}"
    );
}
