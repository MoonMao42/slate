//! eza adapter with managed YAML theme file and EnvironmentVariable strategy.
//! eza uses YAML theme files, not TOML. The adapter writes
//! a managed theme.yml to ~/.config/slate/managed/eza/ and expects EZA_CONFIG_DIR
//! environment variable to be exported by shell init.

use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::{Result, SlateError};
use crate::theme::ThemeVariant;
use std::path::PathBuf;

/// eza adapter implementing the ToolAdapter trait.
pub struct EzaAdapter;

impl EzaAdapter {
    pub fn theme_path(env: &SlateEnv) -> PathBuf {
        env.managed_file("managed/eza/theme.yml")
    }
    /// Render Palette into eza YAML theme structure.
    /// Mapping guided by eza color semantics:
    /// foreground/background: text and background colors
    /// ANSI colors: map to directory/file/permission categories
    pub(crate) fn render_eza_yaml(theme: &ThemeVariant) -> String {
        use crate::cli::picker::preview_panel::SemanticColor;
        let p = &theme.palette;
        let directory = p.resolve(SemanticColor::FileDir);
        let link = p.resolve(SemanticColor::FileSymlink);
        let executable = p.resolve(SemanticColor::FileExec);
        let special = p.resolve(SemanticColor::FileConfig);
        let muted = p.resolve(SemanticColor::Muted);
        // Native eza theme schema: style objects below named role groups.
        // A generic `colors:` map is silently ignored by eza 0.23.5.
        // Do not set icons, backgrounds, file associations or listing layout.
        let mut out = String::from("# Managed by Slate: eza foreground colors.\n");
        for (group, fields) in [
            (
                "filekinds",
                vec![
                    ("normal", p.foreground.as_str()),
                    ("directory", &directory),
                    ("symlink", &link),
                    ("pipe", &special),
                    ("block_device", &special),
                    ("char_device", &special),
                    ("socket", &executable),
                    ("special", &special),
                    ("executable", &executable),
                    ("mount_point", &directory),
                ],
            ),
            (
                "perms",
                vec![
                    ("user_read", &p.green),
                    ("user_write", &p.yellow),
                    ("user_execute_file", &p.red),
                    ("user_execute_other", &p.red),
                    ("group_read", &p.green),
                    ("group_write", &p.yellow),
                    ("group_execute", &p.red),
                    ("other_read", &p.green),
                    ("other_write", &p.yellow),
                    ("other_execute", &p.red),
                    ("special_user_file", &p.magenta),
                    ("special_other", &p.magenta),
                    ("attribute", &muted),
                ],
            ),
            (
                "size",
                vec![
                    ("major", &p.yellow),
                    ("minor", &p.yellow),
                    ("number_byte", &p.foreground),
                    ("number_kilo", &p.green),
                    ("number_mega", &p.yellow),
                    ("number_giga", &p.red),
                    ("number_huge", &p.magenta),
                    ("unit_byte", &muted),
                    ("unit_kilo", &muted),
                    ("unit_mega", &muted),
                    ("unit_giga", &muted),
                    ("unit_huge", &muted),
                ],
            ),
            (
                "links",
                vec![("normal", &muted), ("multi_link_file", &p.magenta)],
            ),
            (
                "users",
                vec![
                    ("user_you", &p.foreground),
                    ("user_root", &p.red),
                    ("user_other", &muted),
                    ("group_yours", &p.foreground),
                    ("group_root", &p.red),
                    ("group_other", &muted),
                ],
            ),
            (
                "git",
                vec![
                    ("new", &p.green),
                    ("modified", &p.yellow),
                    ("deleted", &p.red),
                    ("renamed", &p.blue),
                    ("ignored", &muted),
                    ("conflicted", &p.red),
                ],
            ),
        ] {
            out.push_str(&format!("{group}:\n"));
            for (key, color) in fields {
                out.push_str(&format!("  {key}: {{foreground: '{color}'}}\n"));
            }
        }
        for (key, color) in [
            ("date", &muted),
            ("inode", &muted),
            ("blocks", &muted),
            ("punctuation", &muted),
            ("header", &p.foreground),
            ("octal", &special),
            ("flags", &muted),
            ("control_char", &p.red),
            ("broken_symlink", &p.red),
            ("broken_path_overlay", &p.red),
        ] {
            out.push_str(&format!("{key}: {{foreground: '{color}'}}\n"));
        }
        out
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        theme.palette.validate()?;

        let yaml_content = Self::render_eza_yaml(theme);
        super::managed_fragment::write(
            env,
            &Self::theme_path(env),
            yaml_content.as_bytes(),
            "eza",
        )?;

        // eza picks up EZA_CONFIG_DIR at process launch — already-running
        // shells won't see the new theme until they re-exec.
        Ok(ApplyOutcome::applied_needs_new_shell())
    }
}

impl ToolAdapter for EzaAdapter {
    fn tool_name(&self) -> &'static str {
        "eza"
    }

    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Ok(SlateEnv::from_process()?.eza_config_home().to_owned())
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("eza")
        } else {
            PathBuf::from(".config/slate/managed/eza")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::EnvironmentVariable
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        EzaAdapter::apply_theme_with_env(self, theme, env)
    }

    fn reload(&self) -> Result<()> {
        // eza doesn't support hot-reload; manual restart required
        Err(SlateError::ReloadFailed(
            "eza".to_string(),
            "eza does not support hot-reload. Restart your terminal to apply theme.".to_string(),
        ))
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eza_paths_capture_overrides_without_leaking_into_isolated_profiles() {
        use std::{cell::RefCell, ffi::OsString, os::unix::ffi::OsStringExt};
        let temp = tempfile::tempdir().unwrap();
        let custom = RefCell::new(None::<OsString>);
        let resolve = |isolated| {
            SlateEnv::from_vars(|name| match name {
                "HOME" => Some(temp.path().as_os_str().to_owned()),
                "SLATE_HOME" if isolated => Some(temp.path().join("isolated").into_os_string()),
                "XDG_CONFIG_HOME" => Some(temp.path().join("xdg").into_os_string()),
                "EZA_CONFIG_DIR" => custom.borrow().clone(),
                _ => None,
            })
            .unwrap()
        };
        assert_eq!(
            resolve(false).eza_config_home(),
            temp.path().join("xdg/eza")
        );
        for value in [
            OsString::from("relative-palette"),
            OsString::from(""),
            temp.path().join("custom palette").into_os_string(),
            OsString::from_vec(b"/private/palette-\xff".to_vec()),
        ] {
            *custom.borrow_mut() = Some(value.clone());
            let captured = resolve(false);
            let isolated = resolve(true);
            *custom.borrow_mut() = Some(OsString::from("changed-later"));
            assert_eq!(captured.eza_config_home().as_os_str(), value);
            assert_eq!(
                isolated.eza_config_home(),
                temp.path().join("isolated/.config/eza")
            );
        }
        let injected = SlateEnv::with_home(temp.path().to_owned());
        assert_eq!(injected.eza_config_home(), temp.path().join(".config/eza"));
        assert_eq!(
            EzaAdapter.is_installed_with_env(&injected).unwrap(),
            detection::detect_tool_presence_with_env("eza", &injected).installed
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    #[ignore = "requires explicit SLATE_EZA_BINARY; native long listing in a private directory"]
    fn eza_native_long_listing_uses_palette_permissions_and_byte_size() {
        use std::{fs, os::unix::fs::PermissionsExt, time::Duration};
        let binary = fs::canonicalize(
            std::env::var_os("SLATE_EZA_BINARY").expect("set native eza explicitly"),
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let file = temp.path().join("sample");
        fs::write(&file, "1234567").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o754)).unwrap();
        for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
            EzaAdapter.apply_theme_with_env(theme, &env).unwrap();
            let result = assert_cmd::Command::new(&binary)
                .env_clear()
                .env("HOME", env.home())
                .env("EZA_CONFIG_DIR", env.managed_file("managed/eza"))
                .args([
                    "--color=always",
                    "--icons=never",
                    "--long",
                    "--bytes",
                    "--no-user",
                    "--no-time",
                ])
                .arg(&file)
                .timeout(Duration::from_secs(5))
                .assert()
                .success()
                .stderr("");
            let output = String::from_utf8_lossy(&result.get_output().stdout);
            assert!(output.contains("sample"), "listing must render the fixture");
            for (text, color) in [
                ("r", &theme.palette.green),
                ("w", &theme.palette.yellow),
                ("x", &theme.palette.red),
                ("7", &theme.palette.foreground),
            ] {
                let (r, g, b) =
                    crate::adapter::palette_renderer::PaletteRenderer::hex_to_rgb(color).unwrap();
                let pattern = format!(r"\x1b\[[0-9;]*38;2;{r};{g};{b}(?:;[0-9]+)*m{text}");
                assert!(
                    regex::Regex::new(&pattern).unwrap().is_match(&output),
                    "{} {text}: {output:?}",
                    theme.id
                );
            }
        }
    }

    #[test]
    #[ignore = "requires explicit SLATE_EZA_BINARY; native colors in a private directory"]
    fn eza_native_output_uses_every_generated_palette_and_respects_color_overrides() {
        use crate::cli::picker::preview_panel::SemanticColor;
        use std::{
            fs,
            os::unix::fs::{symlink, PermissionsExt},
            time::Duration,
        };
        let binary = fs::canonicalize(
            std::env::var_os("SLATE_EZA_BINARY").expect("set native eza explicitly"),
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(temp.path().to_owned());
        let files = temp.path().join("files");
        fs::create_dir_all(files.join("folder")).unwrap();
        fs::write(files.join("plain"), "private fixture").unwrap();
        fs::write(files.join("executable"), "never execute this").unwrap();
        fs::set_permissions(files.join("executable"), fs::Permissions::from_mode(0o755)).unwrap();
        symlink("plain", files.join("linked")).unwrap();
        let managed = env.managed_file("managed/eza");
        let run = |override_colors: Option<&str>| {
            let mut cmd = assert_cmd::Command::new(&binary);
            cmd.env_clear()
                .env("HOME", env.home())
                .env("EZA_CONFIG_DIR", &managed)
                .current_dir(&files)
                .args(["--color=always", "--icons=never", "-1"])
                .arg(&files)
                .timeout(Duration::from_secs(5));
            if let Some(colors) = override_colors {
                cmd.env("EZA_COLORS", colors);
            }
            String::from_utf8(
                cmd.assert()
                    .success()
                    .stderr("")
                    .get_output()
                    .stdout
                    .clone(),
            )
            .unwrap()
        };
        fs::create_dir_all(&managed).unwrap();
        fs::write(managed.join("theme.yml"), "colors:\n  info: '#123456'\n").unwrap();
        let control = run(None);
        assert!(
            control.contains("folder"),
            "negative control must render the fixture"
        );
        assert!(
            !control.contains("38;2;18;52;86m"),
            "negative control: generic colors are ignored"
        );
        let personal = env.xdg_config_home().join("eza/theme.yml");
        fs::create_dir_all(personal.parent().unwrap()).unwrap();
        fs::write(&personal, "# PRIVATE PERSONAL ICONS\n").unwrap();
        let ansi = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
            EzaAdapter.apply_theme_with_env(theme, &env).unwrap();
            let output = run(None);
            for (name, color) in [
                ("folder", theme.palette.resolve(SemanticColor::FileDir)),
                ("executable", theme.palette.resolve(SemanticColor::FileExec)),
                ("linked", theme.palette.resolve(SemanticColor::FileSymlink)),
                ("plain", theme.palette.foreground.clone()),
            ] {
                let (r, g, b) =
                    crate::adapter::palette_renderer::PaletteRenderer::hex_to_rgb(&color).unwrap();
                let line = output
                    .lines()
                    .find(|line| {
                        let visible = ansi.replace_all(line, "");
                        visible == name || visible.starts_with(&format!("{name} -> "))
                    })
                    .unwrap_or_else(|| panic!("{} missing {name}: {output:?}", theme.id));
                assert!(
                    line.contains(&format!("38;2;{r};{g};{b}m")),
                    "{} {name}: {line:?}",
                    theme.id
                );
            }
            assert_eq!(
                fs::read_to_string(&personal).unwrap(),
                "# PRIVATE PERSONAL ICONS\n"
            );
        }
        let overridden = run(Some("di=38;2;1;2;3"));
        let directory = overridden
            .lines()
            .find(|line| line.contains("folder"))
            .unwrap();
        assert!(
            directory.contains("38;2;1;2;3m"),
            "environment colors take precedence: {directory:?}"
        );
    }

    #[test]
    fn test_tool_name() {
        let adapter = EzaAdapter;
        assert_eq!(adapter.tool_name(), "eza");
    }

    #[test]
    fn test_is_installed_checks_binary_and_config_dir() {
        let adapter = EzaAdapter;
        let result = adapter.is_installed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_integration_config_path_resolves_eza_config_dir_or_default() {
        // Custom directories need not contain the tool's name.
        assert_eq!(
            EzaAdapter.integration_config_path().unwrap(),
            SlateEnv::from_process().unwrap().eza_config_home()
        );
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = EzaAdapter;
        let path = adapter.managed_config_path();

        assert!(path.to_string_lossy().contains(".config/slate/managed/eza"));
    }

    #[test]
    fn test_apply_strategy_returns_environment_variable() {
        let adapter = EzaAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::EnvironmentVariable);
    }

    #[test]
    fn test_apply_theme_writes_managed_yaml_theme() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = EzaAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        let result = adapter.apply_theme_with_env(&theme, &env);
        assert!(result.is_ok());
    }

    #[test]
    fn test_trait_apply_theme_with_env_writes_inside_injected_env() {
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let adapter = EzaAdapter;
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();

        let result = ToolAdapter::apply_theme_with_env(&adapter, &theme, &env);
        assert!(result.is_ok());
        assert!(tempdir
            .path()
            .join(".config/slate/managed/eza/theme.yml")
            .exists());
    }

    #[test]
    fn test_render_eza_yaml_produces_valid_yaml_structure() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let yaml = EzaAdapter::render_eza_yaml(&theme);

        assert!(yaml.contains("filekinds:\n"));
        assert!(yaml.contains("directory: {foreground: '"));
        assert!(yaml.contains("users:\n"));
        assert!(yaml.contains("git:\n"));
        assert!(yaml.contains("perms:\n"));
        assert!(yaml.contains("size:\n"));
        assert!(yaml.contains("links:\n"));
        assert!(!yaml.contains("colors:\n"));
        assert!(!yaml.contains("background:"));
        assert!(!yaml.contains("icon:"));
    }

    #[test]
    fn test_reload_returns_error() {
        let adapter = EzaAdapter;
        let result = adapter.reload();
        assert!(result.is_err());
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = EzaAdapter;
        let result = adapter.get_current_theme();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
