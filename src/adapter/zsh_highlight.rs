//! zsh-syntax-highlighting adapter for shell syntax coloring.
//! slate generates a managed shell snippet and lets the
//! central shell integration file source it. This avoids competing marker
//! blocks inside `.zshrc`.

use crate::adapter::palette_renderer::PaletteRenderer;
use crate::adapter::{ApplyOutcome, ApplyStrategy, ToolAdapter};
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeVariant;
use std::path::PathBuf;

/// zsh-syntax-highlighting adapter implementing the ToolAdapter trait.
pub struct ZshHighlightAdapter;

impl ZshHighlightAdapter {
    /// Build semantic map for ZSH_HIGHLIGHT_STYLES
    /// Maps palette colors to zsh-syntax-highlighting token types
    fn build_semantic_map() -> Vec<(&'static str, &'static str)> {
        vec![
            ("magenta", "reserved-word"),
            ("blue", "builtin"),
            ("green", "function"),
            ("bright_black", "comment"),
            ("red", "unknown-token"),
            ("green", "arg0"),
            ("green", "command"),
            ("green", "single-quoted-argument"),
            ("green", "double-quoted-argument"),
            ("green", "dollar-quoted-argument"),
            ("yellow", "single-hyphen-option"),
            ("yellow", "double-hyphen-option"),
            ("yellow", "redirection"),
            ("cyan", "commandseparator"),
            ("magenta", "dollar-double-quoted-argument"),
            ("magenta", "command-substitution-delimiter"),
            ("blue", "path"),
            ("blue", "globbing"),
            ("foreground", "default"),
        ]
    }

    fn render_highlight_styles(theme: &ThemeVariant) -> Result<String> {
        theme.palette.validate()?;
        let mut map = Self::build_semantic_map();
        for (palette_key, token) in &mut map {
            if *token == "comment" {
                *palette_key = Self::comment_color(theme).0;
            }
        }
        let assignments = PaletteRenderer::to_shell_vars_from_pairs(&theme.palette, &map)?;
        let keys = map
            .iter()
            .map(|(_, key)| format!("'{key}'"))
            .collect::<Vec<_>>()
            .join(" ");
        // The theme owns foregrounds, not personal backgrounds or decorations.
        // An anonymous function keeps all bookkeeping local and uses no eval.
        let mut output = format!(
            r#"typeset -gA ZSH_HIGHLIGHT_STYLES
() {{
emulate -L zsh
local _slate_key
local -A _slate_saved
local -a _slate_attrs
for _slate_key in {keys}; do
  _slate_attrs=("${{(@s:,:)ZSH_HIGHLIGHT_STYLES[$_slate_key]}}")
  _slate_attrs=("${{(@)_slate_attrs:#fg=*}}")
  _slate_attrs=("${{(@)_slate_attrs:#none}}")
  _slate_attrs=("${{(@)_slate_attrs:#}}")
  _slate_saved[$_slate_key]="${{(j:,:)_slate_attrs}}"
done
"#
        );
        output.push_str(
            assignments
                .strip_prefix("typeset -gA ZSH_HIGHLIGHT_STYLES\n")
                .expect("shell renderer header"),
        );
        output.push_str(&format!(
            r#"for _slate_key in {keys}; do
  if [[ -n $_slate_saved[$_slate_key] ]]; then
    ZSH_HIGHLIGHT_STYLES[$_slate_key]+=",${{_slate_saved[$_slate_key]}}"
  fi
done
}}
"#
        ));
        Ok(output)
    }

    fn comment_color(theme: &ThemeVariant) -> (&'static str, &str) {
        let p = &theme.palette;
        // Keep the original subdued shade when legible; prefer other native
        // grays before falling back to body text. Transparency is not modeled.
        [
            ("bright_black", Some(&p.bright_black)),
            ("overlay2", p.overlay2.as_ref()),
            ("subtext0", p.subtext0.as_ref()),
            ("subtext1", p.subtext1.as_ref()),
        ]
        .into_iter()
        .find_map(|(key, color)| {
            color
                .filter(|color| crate::wcag::contrast_hex(color, &p.background) >= 4.5)
                .map(|color| (key, color.as_str()))
        })
        .unwrap_or(("foreground", p.foreground.as_str()))
    }

    pub fn theme_path(env: &SlateEnv) -> PathBuf {
        env.managed_file("managed/zsh/highlight-styles.sh")
    }
}

impl ToolAdapter for ZshHighlightAdapter {
    fn tool_name(&self) -> &'static str {
        "zsh-syntax-highlighting"
    }

    fn is_installed(&self) -> Result<bool> {
        self.is_installed_with_env(&SlateEnv::from_process()?)
    }

    fn is_installed_with_env(&self, env: &SlateEnv) -> Result<bool> {
        Ok(detection::detect_tool_presence_with_env(self.tool_name(), env).installed)
    }

    fn integration_config_path(&self) -> Result<PathBuf> {
        Ok(SlateEnv::from_process()?.zshrc_path())
    }

    fn managed_config_path(&self) -> PathBuf {
        let env = SlateEnv::from_process().ok();
        if let Some(env) = env.as_ref() {
            env.config_dir().join("managed").join("zsh")
        } else {
            PathBuf::from(".config/slate/managed/zsh")
        }
    }

    fn apply_strategy(&self) -> ApplyStrategy {
        ApplyStrategy::SourceScript
    }

    fn apply_theme(&self, theme: &ThemeVariant) -> Result<ApplyOutcome> {
        let env = SlateEnv::from_process()?;
        self.apply_theme_with_env(theme, &env)
    }

    fn apply_theme_with_env(&self, theme: &ThemeVariant, env: &SlateEnv) -> Result<ApplyOutcome> {
        // Step 1: Build semantic map for ZSH_HIGHLIGHT_STYLES
        let highlight_styles = Self::render_highlight_styles(theme)?;

        super::managed_fragment::write(
            env,
            &Self::theme_path(env),
            highlight_styles.as_bytes(),
            "Zsh highlighting",
        )?;

        // zsh-syntax-highlighting styles are sourced during shell init;
        // already-running shells won't pick up new colors until restart.
        Ok(ApplyOutcome::applied_needs_new_shell())
    }

    fn reload(&self) -> Result<()> {
        // zsh-syntax-highlighting requires shell restart or explicit reload
        // For now, return Ok() and let user restart terminal
        Ok(())
    }

    fn get_current_theme(&self) -> Result<Option<String>> {
        // feature; not implemented yet
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn zsh_foreground_sync_preserves_decorations_and_has_no_bookkeeping_leaks() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("styles.zsh");
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        fs::write(
            &path,
            ZshHighlightAdapter::render_highlight_styles(&theme).unwrap(),
        )
        .unwrap();
        let output = assert_cmd::Command::new("/bin/zsh")
            .env_clear().env("HOME", temp.path()).env("PATH", temp.path()).current_dir(temp.path())
            .args(["-f", "-c", r#"
typeset -A ZSH_HIGHLIGHT_STYLES
ZSH_HIGHLIGHT_STYLES[path]='fg=red,underline,bg=#101010'
ZSH_HIGHLIGHT_STYLES[unknown-token]='none,fg=yellow,bold'
ZSH_HIGHLIGHT_STYLES[default]=none
ZSH_HIGHLIGHT_STYLES[function]='fg=red,$(print BAD > MARKER)'
ZSH_HIGHLIGHT_STYLES[custom-fixture]=italic
ZSH_HIGHLIGHT_HIGHLIGHTERS=(main brackets)
_slate_key=PRIVATE_KEY
typeset -A _slate_saved; _slate_saved[fixture]=PRIVATE_SAVED
_slate_attrs=(PRIVATE_ATTR)
source "$1" || exit 71
first="${(kv)ZSH_HIGHLIGHT_STYLES}"
source "$1" || exit 72
[[ "$first" == "${(kv)ZSH_HIGHLIGHT_STYLES}" ]] || exit 73
[[ $_slate_key == PRIVATE_KEY && $_slate_saved[fixture] == PRIVATE_SAVED && $_slate_attrs[1] == PRIVATE_ATTR ]] || exit 74
[[ $ZSH_HIGHLIGHT_STYLES[custom-fixture] == italic && "$ZSH_HIGHLIGHT_HIGHLIGHTERS" == 'main brackets' ]] || exit 75
print -r -- $ZSH_HIGHLIGHT_STYLES[path]
print -r -- $ZSH_HIGHLIGHT_STYLES[unknown-token]
print -r -- $ZSH_HIGHLIGHT_STYLES[default]
print -r -- $ZSH_HIGHLIGHT_STYLES[function]
"#, "private-styles"]).arg(&path).timeout(std::time::Duration::from_secs(5))
            .assert().success().stderr("").get_output().stdout.clone();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!(
                "fg={},underline,bg=#101010\nfg={},bold\nfg={}\nfg={},$(print BAD > MARKER)\n",
                theme.palette.blue,
                theme.palette.red,
                theme.palette.foreground,
                theme.palette.green
            )
        );
        assert!(!temp.path().join("MARKER").exists());
    }

    #[test]
    fn zsh_comments_remain_legible_against_each_opaque_theme_background() {
        let mut failures = Vec::new();
        for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
            let styles = ZshHighlightAdapter::render_highlight_styles(theme).unwrap();
            let line = styles
                .lines()
                .find(|line| line.starts_with("ZSH_HIGHLIGHT_STYLES[comment]="))
                .unwrap();
            let color = line.split_once("fg=").unwrap().1.trim_end_matches('\'');
            let contrast = crate::wcag::contrast_hex(color, &theme.palette.background);
            if contrast < 4.5 {
                failures.push(format!("{}: {contrast:.2}", theme.id));
            }
        }
        assert!(
            failures.is_empty(),
            "Low-contrast comments: {}",
            failures.join(", ")
        );
    }

    #[test]
    #[ignore = "requires explicit SLATE_ZSH_HIGHLIGHT_PLUGIN; private non-executed input buffers"]
    fn zsh_native_highlighter_uses_palette_for_builtin_quotes_and_unknown_commands() {
        let plugin = fs::canonicalize(
            std::env::var_os("SLATE_ZSH_HIGHLIGHT_PLUGIN").expect("set plugin explicitly"),
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let styles = temp.path().join("styles.zsh");
        for theme in crate::theme::ThemeRegistry::new().unwrap().all() {
            let rendered = ZshHighlightAdapter::render_highlight_styles(theme).unwrap();
            for (_, key) in ZshHighlightAdapter::build_semantic_map() {
                assert!(
                    rendered.contains(&format!("ZSH_HIGHLIGHT_STYLES[{key}]=")),
                    "{} missing {key}",
                    theme.id
                );
            }
            fs::write(&styles, rendered).unwrap();
            let output = assert_cmd::Command::new("/bin/zsh")
                .env_clear()
                .env("HOME", temp.path())
                .env("PATH", temp.path())
                .current_dir(temp.path())
                .args([
                    "-f",
                    "-c",
                    r#"
source "$1" || exit 71
ZSH_HIGHLIGHT_STYLES[custom-fixture]=bold
source "$2" || exit 72
[[ $ZSH_HIGHLIGHT_STYLES[custom-fixture] == bold ]] || exit 73
PREBUFFER=''; CONTEXT=start
BUFFER='print "hello"'; CURSOR=$#BUFFER; region_highlight=()
_zsh_highlight_highlighter_main_paint
print -rl -- $region_highlight
BUFFER='slate_missing_fixture'; CURSOR=$#BUFFER; region_highlight=()
_zsh_highlight_highlighter_main_paint
print -rl -- $region_highlight
setopt interactivecomments
# The normal dispatcher snapshots options before calling the painter. Mirror
# that input here because this fixture calls the native painter directly.
typeset -A zsyh_user_options
zsyh_user_options=("${(kv)options[@]}")
BUFFER='# note'; CURSOR=$#BUFFER; region_highlight=()
_zsh_highlight_highlighter_main_paint
print -rl -- $region_highlight
"#,
                    "private-highlight",
                ])
                .arg(&plugin)
                .arg(&styles)
                .timeout(std::time::Duration::from_secs(5))
                .assert()
                .success()
                .stderr("")
                .get_output()
                .stdout
                .clone();
            let output = String::from_utf8(output).unwrap();
            assert!(
                output.contains(&format!(
                    "0 6 fg={}",
                    ZshHighlightAdapter::comment_color(theme).1.to_lowercase()
                )),
                "{} comment: {output:?}",
                theme.id
            );
            for (span, color) in [
                ("0 5", &theme.palette.blue),
                ("6 13", &theme.palette.green),
                ("0 21", &theme.palette.red),
            ] {
                assert!(
                    output.contains(&format!("{span} fg={}", color.to_lowercase())),
                    "{} {span}: {output:?}",
                    theme.id
                );
            }
        }
    }

    #[test]
    fn test_tool_name() {
        let adapter = ZshHighlightAdapter;
        assert_eq!(adapter.tool_name(), "zsh-syntax-highlighting");
    }

    #[test]
    fn test_apply_strategy_returns_source_script() {
        let adapter = ZshHighlightAdapter;
        assert_eq!(adapter.apply_strategy(), ApplyStrategy::SourceScript);
    }

    #[test]
    fn test_managed_config_path_returns_correct_directory() {
        let adapter = ZshHighlightAdapter;
        let path = adapter.managed_config_path();
        assert!(path.to_string_lossy().contains(".config/slate/managed/zsh"));
    }

    #[test]
    fn test_integration_config_path_returns_zshrc() {
        let adapter = ZshHighlightAdapter;
        let result = adapter.integration_config_path();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().contains(".zshrc"));
    }

    #[test]
    fn test_is_installed_returns_false_when_not_installed() {
        // This test may vary by environment; we verify the logic exists
        let adapter = ZshHighlightAdapter;
        let result = adapter.is_installed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_semantic_map_has_expected_keys() {
        let map = ZshHighlightAdapter::build_semantic_map();
        assert!(!map.is_empty());
        let has_keyword = map.iter().any(|(_, token)| *token == "reserved-word");
        assert!(has_keyword);
    }

    #[test]
    fn test_semantic_map_preserves_duplicate_palette_keys() {
        let map = ZshHighlightAdapter::build_semantic_map();
        let red_count = map
            .iter()
            .filter(|(palette_key, _)| *palette_key == "red")
            .count();
        let green_count = map
            .iter()
            .filter(|(palette_key, _)| *palette_key == "green")
            .count();

        assert_eq!(red_count, 1);
        assert_eq!(green_count, 6);
    }

    #[test]
    fn test_render_highlight_styles_preserves_duplicate_shell_tokens() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let styles = ZshHighlightAdapter::render_highlight_styles(&theme).unwrap();

        assert!(styles.contains("ZSH_HIGHLIGHT_STYLES[unknown-token]='fg=#"));
        assert!(styles.contains("ZSH_HIGHLIGHT_STYLES[arg0]='fg=#"));
        assert!(styles.contains("ZSH_HIGHLIGHT_STYLES[function]='fg=#"));
        assert!(styles.contains("ZSH_HIGHLIGHT_STYLES[single-quoted-argument]='fg=#"));
    }

    #[test]
    fn zsh_apply_writes_only_the_injected_snippet_and_preserves_startup() {
        use std::os::unix::fs::MetadataExt;
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let startup = env.zshrc_path();
        fs::write(&startup, "# PRIVATE STARTUP\n").unwrap();
        let original = fs::metadata(&startup).unwrap();
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let result = ToolAdapter::apply_theme_with_env(&ZshHighlightAdapter, &theme, &env).unwrap();
        assert!(matches!(
            result,
            ApplyOutcome::Applied {
                requires_new_shell: true
            }
        ));
        assert_eq!(
            fs::read_to_string(ZshHighlightAdapter::theme_path(&env)).unwrap(),
            ZshHighlightAdapter::render_highlight_styles(&theme).unwrap()
        );
        assert_eq!(fs::read_to_string(&startup).unwrap(), "# PRIVATE STARTUP\n");
        assert_eq!(fs::metadata(&startup).unwrap().ino(), original.ino());
        assert!(!env.managed_file("current").exists());
        assert!(!env.managed_file("config.toml").exists());
        assert!(!env.managed_file("managed/shell").exists());
        assert!(!env.slate_cache_dir().exists());
    }

    #[test]
    fn zsh_apply_refuses_linked_oversized_and_invalid_inputs_without_overwriting() {
        use std::os::unix::fs::{symlink, MetadataExt};
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let path = ZshHighlightAdapter::theme_path(&env);
        let personal = env.zshrc_path();
        fs::write(&personal, "# PRIVATE STARTUP\n").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(&personal, &path).unwrap();
        assert!(ZshHighlightAdapter
            .apply_theme_with_env(&theme, &env)
            .is_err());
        assert!(fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_to_string(&personal).unwrap(),
            "# PRIVATE STARTUP\n"
        );
        fs::remove_file(&path).unwrap();
        let size = crate::config::file_read::MAX_TOOL_CONFIG_BYTES + 1;
        fs::File::create(&path).unwrap().set_len(size).unwrap();
        let original = fs::metadata(&path).unwrap();
        assert!(ZshHighlightAdapter
            .apply_theme_with_env(&theme, &env)
            .is_err());
        assert_eq!(fs::metadata(&path).unwrap().len(), size);
        assert_eq!(fs::metadata(&path).unwrap().ino(), original.ino());
        fs::remove_file(&path).unwrap();
        let mut invalid = theme;
        invalid.palette.foreground = "not a color".into();
        assert!(ZshHighlightAdapter
            .apply_theme_with_env(&invalid, &env)
            .is_err());
        assert!(!path.exists());
        assert!(!env.slate_cache_dir().exists());
    }

    #[test]
    fn test_reload_returns_ok() {
        let adapter = ZshHighlightAdapter;
        let result = adapter.reload();
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_current_theme_returns_none() {
        let adapter = ZshHighlightAdapter;
        let result = adapter.get_current_theme();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
