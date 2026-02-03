use crate::detection::shell_quote;
use crate::theme::ThemeVariant;

pub(crate) struct ShellIntegrationOptions<'a> {
    pub managed_root: &'a str,
    pub user_config_root: &'a str,
    pub user_local_bin: Option<&'a str>,
    pub plain_starship_path: &'a str,
    pub active_starship_path: &'a str,
    pub notify_path: &'a str,
    pub slate_bin: &'a str,
    pub zsh_highlighting_plugin_path: Option<&'a str>,
    pub homebrew_prefix: Option<&'a str>,
    pub starship_enabled: bool,
    pub zsh_highlighting_enabled: bool,
    pub fastfetch_autorun: bool,
    pub auto_theme_enabled: bool,
}

const LEGACY_PLAIN_STARSHIP_CONTENT: &str = r#"format = "$username$directory$git_branch$git_status$cmd_duration$line_break$character"

[username]
show_always = true
format = "[$user]($style) "
style_user = "bold green"

[directory]
format = "[$path]($style) "
style = "bold cyan"
truncation_length = 3

[git_branch]
format = "[$symbol$branch]($style) "
symbol = ""
style = "bold purple"

[git_status]
format = "([$all_status$ahead_behind]($style) )"
style = "bold red"

[cmd_duration]
format = "[$duration]($style) "
style = "bold yellow"

[character]
success_symbol = "[>](bold green)"
error_symbol = "[>](bold red)"
"#;

const LEGACY_STARTER_STARSHIP_CONTENT: &str = r#"format = "$username$directory$git_branch$git_status$cmd_duration$line_break$character"
palette = "slate"

[username]
show_always = true
format = "[$user]($style) "
style_user = "bold green"

[directory]
format = "[$path]($style) "
style = "bold sapphire"
truncation_length = 3

[git_branch]
format = "[$symbol$branch]($style) "
symbol = ""
style = "bold lavender"

[git_status]
format = "([$all_status$ahead_behind]($style) )"
style = "bold red"

[cmd_duration]
format = "[$duration]($style) "
style = "bold peach"

[character]
success_symbol = "[>](bold green)"
error_symbol = "[>](bold red)"
"#;

const DEFAULT_STARSHIP_CONTENT: &str = r#""$schema" = 'https://starship.rs/config-schema.json'

add_newline = true
command_timeout = 1500
palette = "slate"

format = """
[](red)$os$username[](bg:peach fg:red)$directory[](bg:yellow fg:peach)$git_branch$git_status[](fg:yellow bg:green)$python$nodejs$rust$golang$c[](fg:green bg:sapphire)$docker_context[](fg:sapphire bg:lavender)$time[](fg:lavender)$fill$cmd_duration$line_break$character"""

[os]
disabled = false
style = "bg:red fg:powerline_fg"
format = '[ $symbol ]($style)'

[os.symbols]
Macos = "󰀵"

[username]
show_always = true
style_user = "bg:red fg:powerline_fg"
style_root = "bg:red fg:powerline_fg"
format = '[$user ]($style)'

[directory]
style = "bg:peach fg:powerline_fg"
format = '[ $path ]($style)'
truncation_length = 3
truncate_to_repo = false
truncation_symbol = "…/"

[directory.substitutions]
"Documents" = "󰈙 "
"Downloads" = " "
"Music" = "󰝚 "
"Pictures" = " "
"Developer" = "󰲻 "

[fill]
symbol = ' '

[git_branch]
symbol = ""
style = "bg:yellow"
format = '[[ $symbol $branch ](fg:powerline_fg bg:yellow)]($style)'

[git_status]
style = "bg:yellow"
format = '[[($all_status$ahead_behind )](fg:powerline_fg bg:yellow)]($style)'

[python]
symbol = ""
style = "bg:green"
format = '[[ $symbol( $version)(\(#$virtualenv\)) ](fg:powerline_fg bg:green)]($style)'
detect_extensions = []
detect_files = []
detect_folders = []

[nodejs]
symbol = ""
style = "bg:green"
format = '[[ $symbol( $version) ](fg:powerline_fg bg:green)]($style)'

[rust]
symbol = ""
style = "bg:green"
format = '[[ $symbol( $version) ](fg:powerline_fg bg:green)]($style)'

[golang]
symbol = ""
style = "bg:green"
format = '[[ $symbol( $version) ](fg:powerline_fg bg:green)]($style)'

[c]
symbol = " "
style = "bg:green"
format = '[[ $symbol( $version) ](fg:powerline_fg bg:green)]($style)'

[docker_context]
symbol = ""
style = "bg:sapphire"
format = '[[ $symbol( $context) ](fg:powerline_fg bg:sapphire)]($style)'

[time]
disabled = false
time_format = "%R"
style = "bg:lavender"
format = '[[  $time ](fg:powerline_fg bg:lavender)]($style)'

[cmd_duration]
min_time = 2000
style = "fg:yellow"
format = ' [󱨨 $duration]($style)'

[character]
success_symbol = '[❯](bold fg:green)'
error_symbol = '[❯](bold fg:red)'
vimcmd_symbol = '[❮](bold fg:green)'

[aws]
disabled = true
[gcloud]
disabled = true
"#;

pub(crate) fn build_shell_integration_content(
    theme: &ThemeVariant,
    options: &ShellIntegrationOptions<'_>,
) -> String {
    let mut content = String::new();
    let managed_root = shell_quote(options.managed_root);
    let user_config_root = shell_quote(options.user_config_root);
    let user_local_bin = options.user_local_bin.map(shell_quote);
    let plain_starship_path = shell_quote(options.plain_starship_path);
    let active_starship_path = shell_quote(options.active_starship_path);
    let notify_path = shell_quote(options.notify_path);
    let slate_bin = shell_quote(options.slate_bin);

    if let Some(prefix) = options.homebrew_prefix {
        let brew_bin = shell_quote(&format!("{}/bin", prefix));
        let brew_sbin = shell_quote(&format!("{}/sbin", prefix));
        content.push_str(&format!(
            "if [ -d {bin} ]; then\n  case \":$PATH:\" in\n    *:{bin}:*) ;;\n    *) export PATH={bin}:\"$PATH\" ;;\n  esac\nfi\n",
            bin = brew_bin
        ));
        content.push_str(&format!(
            "if [ -d {sbin} ]; then\n  case \":$PATH:\" in\n    *:{sbin}:*) ;;\n    *) export PATH={sbin}:\"$PATH\" ;;\n  esac\nfi\n",
            sbin = brew_sbin
        ));
    }

    if let Some(local_bin) = user_local_bin.as_deref() {
        content.push_str(&format!(
            "if [ -d {bin} ]; then\n  case \":$PATH:\" in\n    *:{bin}:*) ;;\n    *) export PATH={bin}:\"$PATH\" ;;\n  esac\nfi\n",
            bin = local_bin
        ));
    }

    content.push_str(&format!(
        "export BAT_THEME={}\n",
        shell_quote(
            theme
                .tool_refs
                .get("bat")
                .map(|s| s.as_str())
                .unwrap_or("Catppuccin Mocha")
        )
    ));

    content.push_str(&format!(
        "export EZA_CONFIG_DIR={managed}/eza\n",
        managed = managed_root
    ));

    content.push_str(&format!(
        "export LG_CONFIG_FILE={managed}/lazygit/config.yml:{xdg}/lazygit/config.yml\n",
        managed = managed_root,
        xdg = user_config_root
    ));

    content.push_str(&format!(
        "fastfetch() {{ command fastfetch -c {managed}/fastfetch/config.jsonc \"$@\"; }}\n",
        managed = managed_root
    ));

    if options.zsh_highlighting_enabled {
        if let Some(plugin_path) = options.zsh_highlighting_plugin_path {
            let plugin_path = shell_quote(plugin_path);
            content.push_str(&format!(
                "if [ -f {plugin} ]; then\n  source {plugin}\nfi\n",
                plugin = plugin_path
            ));
        }

        content.push_str(&format!(
            "if [ -f {managed}/zsh/highlight-styles.sh ]; then\n  source {managed}/zsh/highlight-styles.sh\nfi\n",
            managed = managed_root
        ));
    }

    if options.fastfetch_autorun {
        content.push_str("if command -v fastfetch &> /dev/null; then\n");
        content.push_str("  fastfetch\n");
        content.push_str("fi\n");
    }

    if options.auto_theme_enabled {
        content.push_str(&format!(
            r#"if [[ "${{TERM_PROGRAM:l}}" == "ghostty" ]] && [[ -x {path} ]]; then
    if ! pgrep -qf "slate-dark-mode-notify"; then
      {path} {slate_bin} theme --auto --quiet &!
    fi
  fi
"#,
            path = notify_path,
            slate_bin = slate_bin
        ));
    }

    if options.starship_enabled {
        content.push_str(&format!(
            "if [ -f {active} ]; then\n  export STARSHIP_CONFIG={active}\nelse\n  export STARSHIP_CONFIG={plain}\nfi\n",
            active = active_starship_path,
            plain = plain_starship_path
        ));

        content.push_str("\nif command -v starship &> /dev/null; then\n");
        content.push_str("  eval \"$(starship init zsh)\"\n");
        content.push_str("fi\n");
    }

    content
}

fn starship_palette_value(theme: &ThemeVariant, key: &str) -> String {
    let p = &theme.palette;
    match key {
        "red" => p.red.clone(),
        "yellow" => p.yellow.clone(),
        "green" => p.green.clone(),
        "blue" => p.blue.clone(),
        "white" => p.white.clone(),
        "foreground" => p.foreground.clone(),
        "background" => p.background.clone(),
        "text" => p.text.clone().unwrap_or_else(|| p.foreground.clone()),
        "peach" => p
            .extras
            .get("peach")
            .or_else(|| p.extras.get("orange"))
            .or_else(|| p.extras.get("rose"))
            .cloned()
            .unwrap_or_else(|| {
                if p.bright_red != p.red && p.bright_red != p.yellow {
                    p.bright_red.clone()
                } else if p.bright_yellow != p.yellow && p.bright_yellow != p.red {
                    p.bright_yellow.clone()
                } else {
                    p.magenta.clone()
                }
            }),
        "sapphire" => p
            .extras
            .get("sapphire")
            .or_else(|| p.extras.get("foam"))
            .cloned()
            .unwrap_or_else(|| {
                if p.bright_blue != p.blue {
                    p.bright_blue.clone()
                } else {
                    p.cyan.clone()
                }
            }),
        "lavender" => p
            .lavender
            .clone()
            .or_else(|| p.extras.get("lavender").cloned())
            .or_else(|| p.extras.get("iris").cloned())
            .or_else(|| p.mauve.clone())
            .unwrap_or_else(|| p.bright_magenta.clone()),
        "teal" => p.cyan.clone(),
        "maroon" => p
            .extras
            .get("maroon")
            .cloned()
            .unwrap_or_else(|| p.bright_red.clone()),
        "sky" => p.bright_cyan.clone(),
        "pink" => p
            .pink
            .clone()
            .or_else(|| p.extras.get("pink").cloned())
            .unwrap_or_else(|| p.bright_magenta.clone()),
        "crust" => p.bg_darkest.clone().unwrap_or_else(|| p.black.clone()),
        "powerline_fg" => {
            if theme.appearance == crate::theme::ThemeAppearance::Light {
                p.foreground.clone()
            } else {
                p.bg_darkest.clone().unwrap_or_else(|| p.black.clone())
            }
        }
        _ => String::new(),
    }
}

pub(crate) fn themed_starship_content(theme: &ThemeVariant) -> String {
    let mut content = DEFAULT_STARSHIP_CONTENT.to_string();
    content.push_str("\n\n[palettes.slate]\n");
    for key in [
        "red",
        "yellow",
        "green",
        "blue",
        "white",
        "foreground",
        "background",
        "text",
        "peach",
        "sapphire",
        "lavender",
        "teal",
        "maroon",
        "sky",
        "pink",
        "crust",
        "powerline_fg",
    ] {
        content.push_str(&format!(
            "{key} = \"{}\"\n",
            starship_palette_value(theme, key)
        ));
    }
    content
}

#[allow(dead_code)]
pub(crate) fn plain_starship_content() -> &'static str {
    DEFAULT_STARSHIP_CONTENT
}

pub(crate) fn starter_starship_content() -> &'static str {
    DEFAULT_STARSHIP_CONTENT
}

pub(crate) fn should_upgrade_seeded_starship_content(content: &str) -> bool {
    let trimmed = content.trim();
    trimmed.is_empty()
        || trimmed == LEGACY_PLAIN_STARSHIP_CONTENT.trim()
        || trimmed == LEGACY_STARTER_STARSHIP_CONTENT.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_options<'a>() -> ShellIntegrationOptions<'a> {
        ShellIntegrationOptions {
            managed_root: "/tmp/slate/managed",
            user_config_root: "/tmp/.config",
            user_local_bin: Some("/tmp/.local/bin"),
            plain_starship_path: "/tmp/slate/managed/starship/plain.toml",
            active_starship_path: "/tmp/.config/starship.toml",
            notify_path: "/tmp/slate/managed/bin/slate-dark-mode-notify",
            slate_bin: "/tmp/slate/bin/slate",
            zsh_highlighting_plugin_path: Some(
                "/opt/homebrew/share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh",
            ),
            homebrew_prefix: Some("/opt/homebrew"),
            starship_enabled: true,
            zsh_highlighting_enabled: true,
            fastfetch_autorun: true,
            auto_theme_enabled: false,
        }
    }

    #[test]
    fn test_shell_integration_prepends_homebrew_paths() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let content = build_shell_integration_content(&theme, &sample_options());

        assert!(content.contains("export PATH='/tmp/.local/bin':\"$PATH\""));
        assert!(content.contains("export PATH='/opt/homebrew/bin':\"$PATH\""));
        assert!(content.contains("export PATH='/opt/homebrew/sbin':\"$PATH\""));
    }

    #[test]
    fn test_shell_integration_exports_active_starship_config() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let content = build_shell_integration_content(&theme, &sample_options());

        assert!(content.contains("if [ -f '/tmp/.config/starship.toml' ]; then"));
        assert!(content.contains("export STARSHIP_CONFIG='/tmp/.config/starship.toml'"));
        assert!(content.contains("export STARSHIP_CONFIG='/tmp/slate/managed/starship/plain.toml'"));
    }

    #[test]
    fn test_shell_integration_can_disable_starship() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let mut options = sample_options();
        options.starship_enabled = false;

        let content = build_shell_integration_content(&theme, &options);

        assert!(!content.contains("starship init zsh"));
        assert!(!content.contains("export STARSHIP_CONFIG="));
    }

    #[test]
    fn test_shell_integration_can_disable_zsh_highlighting() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let mut options = sample_options();
        options.zsh_highlighting_enabled = false;

        let content = build_shell_integration_content(&theme, &options);

        assert!(!content.contains("zsh-syntax-highlighting.zsh"));
        assert!(!content.contains("highlight-styles.sh"));
    }

    #[test]
    fn test_starter_starship_content_uses_slate_palette() {
        let content = starter_starship_content();

        assert!(content.contains("\"$schema\" = 'https://starship.rs/config-schema.json'"));
        assert!(content.contains("palette = \"slate\""));
        assert!(content.contains("[](red)$os$username"));
        assert!(content.contains("[time]"));
    }

    #[test]
    fn test_themed_starship_content_includes_palette_block() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let content = themed_starship_content(&theme);

        assert!(content.contains("[palettes.slate]"));
        assert!(content.contains("powerline_fg = "));
        assert!(content.contains("peach = "));
    }

    #[test]
    fn test_should_upgrade_legacy_seeded_starship_content() {
        assert!(should_upgrade_seeded_starship_content(""));
        assert!(should_upgrade_seeded_starship_content(
            LEGACY_PLAIN_STARSHIP_CONTENT
        ));
        assert!(should_upgrade_seeded_starship_content(
            LEGACY_STARTER_STARSHIP_CONTENT
        ));
        assert!(!should_upgrade_seeded_starship_content(
            DEFAULT_STARSHIP_CONTENT
        ));
    }
}
