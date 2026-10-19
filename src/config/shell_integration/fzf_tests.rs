use super::*;
use std::{fs, path::Path, process::Command};

const USER_OPTS: &str = "--layout=reverse --multi --bind=ctrl-a:select-all";

fn sample_options<'a>(root: &'a str) -> ShellIntegrationOptions<'a> {
    ShellIntegrationOptions {
        managed_root: root,
        user_config_root: root,
        lazygit_default_config: root,
        user_local_bin: None,
        plain_starship_path: root,
        active_starship_path: root,
        notify_path: root,
        zsh_highlighting_plugin_path: None,
        homebrew_prefix: None,
        prefer_plain_starship: true,
        starship_enabled: false,
        zsh_highlighting_enabled: false,
        fastfetch_autorun: false,
        auto_theme_enabled: false,
    }
}

fn write_themes(root: &Path, shell: &str) -> (String, String, String, String) {
    let dark = crate::theme::catppuccin::catppuccin_mocha().unwrap();
    let light = crate::theme::catppuccin::catppuccin_latte().unwrap();
    let root_str = root.to_str().unwrap();
    let dark_files = build_shell_integration_files(&dark, &sample_options(root_str));
    let light_files = build_shell_integration_files(&light, &sample_options(root_str));
    let dark_content = match shell {
        "bash" => dark_files.bash,
        "zsh" => dark_files.zsh,
        "fish" => dark_files.fish,
        _ => unreachable!(),
    };
    let light_content = match shell {
        "bash" => light_files.bash,
        "zsh" => light_files.zsh,
        "fish" => light_files.fish,
        _ => unreachable!(),
    };
    let dark_file = root.join(format!("dark.{shell}"));
    let light_file = root.join(format!("light.{shell}"));
    fs::write(&dark_file, dark_content).unwrap();
    fs::write(&light_file, light_content).unwrap();
    (
        dark_file.to_str().unwrap().to_owned(),
        light_file.to_str().unwrap().to_owned(),
        crate::adapter::ls_colors::render_fzf_colors(&dark.palette),
        crate::adapter::ls_colors::render_fzf_colors(&light.palette),
    )
}

#[test]
fn posix_fzf_options_survive_repeated_source_and_palette_changes() {
    for shell in ["bash", "zsh"] {
        let Ok(binary) = which::which(shell) else {
            assert_eq!(shell, "zsh", "Bash is required for this regression test");
            eprintln!("Zsh unavailable; its native fzf integration check was not run");
            continue;
        };
        let temp = tempfile::tempdir().unwrap();
        let (dark, light, dark_color, light_color) = write_themes(temp.path(), shell);
        assert_ne!(dark_color, light_color);

        let mut command = Command::new(binary);
        command
            .env_clear()
            .env("HOME", temp.path())
            .env("PATH", "/usr/bin:/bin");
        if shell == "bash" {
            command.args(["--noprofile", "--norc"]);
        } else {
            command.arg("-f");
        }
        let output = command
            .args([
                "-c",
                r#"
export FZF_DEFAULT_OPTS='--layout=reverse --multi --bind=ctrl-a:select-all'
source "$1" || exit 71
source "$1" || exit 72
FZF_DEFAULT_OPTS="$FZF_DEFAULT_OPTS --border"
source "$2" || exit 73
source "$2" || exit 74
printf '%s\n' "$FZF_DEFAULT_OPTS"
"#,
                "slate-fzf-test",
                &dark,
                &light,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{USER_OPTS} --border {light_color}\n"),
            "{shell}: previous Slate color {dark_color} should be replaced"
        );
    }
}

#[test]
fn fish_fzf_options_survive_repeated_source_and_palette_changes_when_available() {
    let Ok(binary) = which::which("fish") else {
        eprintln!("Fish unavailable; its native fzf integration check was not run");
        return;
    };
    let temp = tempfile::tempdir().unwrap();
    let (dark, light, dark_color, light_color) = write_themes(temp.path(), "fish");
    assert_ne!(dark_color, light_color);
    let output = Command::new(binary)
        .env_clear()
        .env("HOME", temp.path())
        .env("PATH", "/usr/bin:/bin")
        .args([
            "--no-config",
            "--private",
            "-c",
            r#"
set -gx FZF_DEFAULT_OPTS '--layout=reverse --multi --bind=ctrl-a:select-all'
source "$argv[1]"; or exit 71
source "$argv[1]"; or exit 72
set -gx FZF_DEFAULT_OPTS "$FZF_DEFAULT_OPTS --border"
source "$argv[2]"; or exit 73
source "$argv[2]"; or exit 74
printf '%s\n' "$FZF_DEFAULT_OPTS"
"#,
            &dark,
            &light,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fish: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{USER_OPTS} --border {light_color}\n"),
        "fish: previous Slate color {dark_color} should be replaced"
    );
}
