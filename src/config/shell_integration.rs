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
    pub prefer_plain_starship: bool,
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
style = "bg:red fg:powerline_fg_red"
format = '[ $symbol ]($style)'

[os.symbols]
Macos = "󰀵"

[username]
show_always = true
style_user = "bg:red fg:powerline_fg_red"
style_root = "bg:red fg:powerline_fg_red"
format = '[$user ]($style)'

[directory]
style = "bg:peach fg:powerline_fg_peach"
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
format = '[[ $symbol $branch ](fg:powerline_fg_yellow bg:yellow)]($style)'

[git_status]
style = "bg:yellow"
format = '[[($all_status$ahead_behind )](fg:powerline_fg_yellow bg:yellow)]($style)'

[python]
symbol = ""
style = "bg:green"
format = '[[ $symbol( $version)(\(#$virtualenv\)) ](fg:powerline_fg_green bg:green)]($style)'
detect_extensions = []
detect_files = []
detect_folders = []

[nodejs]
symbol = ""
style = "bg:green"
format = '[[ $symbol( $version) ](fg:powerline_fg_green bg:green)]($style)'

[rust]
symbol = ""
style = "bg:green"
format = '[[ $symbol( $version) ](fg:powerline_fg_green bg:green)]($style)'

[golang]
symbol = ""
style = "bg:green"
format = '[[ $symbol( $version) ](fg:powerline_fg_green bg:green)]($style)'

[c]
symbol = " "
style = "bg:green"
format = '[[ $symbol( $version) ](fg:powerline_fg_green bg:green)]($style)'

[docker_context]
symbol = ""
style = "bg:sapphire"
format = '[[ $symbol( $context) ](fg:powerline_fg_sapphire bg:sapphire)]($style)'

[time]
disabled = false
time_format = "%R"
style = "bg:lavender"
format = '[[  $time ](fg:powerline_fg_lavender bg:lavender)]($style)'

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

const BASIC_STARSHIP_CONTENT: &str = r#""$schema" = 'https://starship.rs/config-schema.json'

add_newline = true
command_timeout = 1500
palette = "slate"

format = "$username$directory$git_branch$git_status$cmd_duration$line_break$character"

[username]
show_always = true
format = "[$user]($style) "
style_user = "bold red"
style_root = "bold red"

[directory]
format = "[$path]($style) "
style = "bold peach"
truncation_length = 3
truncate_to_repo = false
truncation_symbol = ".../"

[git_branch]
symbol = "git:"
style = "bold yellow"
format = "[$symbol$branch]($style) "

[git_status]
style = "bold yellow"
format = "([$all_status$ahead_behind]($style) )"

[cmd_duration]
min_time = 2000
style = "fg:yellow"
format = "[$duration]($style) "

[character]
success_symbol = "[>](bold fg:green)"
error_symbol = "[x](bold fg:red)"
vimcmd_symbol = "[<](bold fg:green)"

[aws]
disabled = true
[gcloud]
disabled = true
"#;

#[derive(Debug, Clone)]
pub(crate) struct ShellIntegrationFiles {
    pub zsh: String,
    pub bash: String,
    pub fish: String,
}

#[derive(Debug, Clone)]
struct PathEntry {
    raw: String,
    quoted: String,
}

#[derive(Debug, Clone)]
struct SharedShellModel {
    path_entries: Vec<PathEntry>,
    bat_theme: String,
    eza_config_dir: String,
    lg_config_file: String,
    // (D-A6): shell-quoted LS_COLORS / EZA_COLORS strings,
    // rendered from the active palette by `ls_colors::render_strings` and
    // emitted from render_shared_exports (POSIX) / render_fish_shell (fish).
    ls_colors: String,
    eza_colors: String,
    fastfetch_config_path: String,
    plain_starship_path: String,
    active_starship_path: String,
    notify_path: String,
    slate_bin: String,
    zsh_highlighting_plugin_path: Option<String>,
    zsh_highlight_styles_path: String,
    prefer_plain_starship: bool,
    starship_enabled: bool,
    zsh_highlighting_enabled: bool,
    fastfetch_autorun: bool,
    auto_theme_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PosixShell {
    Zsh,
    Bash,
}

impl PosixShell {
    const fn starship_shell(self) -> &'static str {
        match self {
            Self::Zsh => "zsh",
            Self::Bash => "bash",
        }
    }
}

impl SharedShellModel {
    fn new(theme: &ThemeVariant, options: &ShellIntegrationOptions<'_>) -> Self {
        let mut path_entries = Vec::new();

        if let Some(prefix) = options.homebrew_prefix {
            for path in [format!("{}/bin", prefix), format!("{}/sbin", prefix)] {
                path_entries.push(PathEntry {
                    raw: path.clone(),
                    quoted: shell_quote(&path),
                });
            }
        }

        if let Some(local_bin) = options.user_local_bin {
            path_entries.push(PathEntry {
                raw: local_bin.to_string(),
                quoted: shell_quote(local_bin),
            });
        }

        // D-A6: project the active palette into shell-ready
        // LS_COLORS / EZA_COLORS strings. The renderer lives in
        // `src/adapter/ls_colors.rs`; we shell-quote the results once so the
        // two env var exports can be interpolated into POSIX `export X={}` /
        // fish `set -gx X {}` lines without further escaping.
        let (raw_ls, raw_eza) = crate::adapter::ls_colors::render_strings(&theme.palette);

        Self {
            path_entries,
            bat_theme: shell_quote(
                theme
                    .tool_refs
                    .get("bat")
                    .map(|value| value.as_str())
                    .unwrap_or("Catppuccin Mocha"),
            ),
            eza_config_dir: shell_quote(&format!("{}/eza", options.managed_root)),
            lg_config_file: shell_quote(&format!(
                "{}/lazygit/config.yml:{}/lazygit/config.yml",
                options.managed_root, options.user_config_root
            )),
            ls_colors: shell_quote(&raw_ls),
            eza_colors: shell_quote(&raw_eza),
            fastfetch_config_path: shell_quote(&format!(
                "{}/fastfetch/config.jsonc",
                options.managed_root
            )),
            plain_starship_path: shell_quote(options.plain_starship_path),
            active_starship_path: shell_quote(options.active_starship_path),
            notify_path: shell_quote(options.notify_path),
            slate_bin: shell_quote(options.slate_bin),
            zsh_highlighting_plugin_path: options.zsh_highlighting_plugin_path.map(shell_quote),
            zsh_highlight_styles_path: shell_quote(&format!(
                "{}/zsh/highlight-styles.sh",
                options.managed_root
            )),
            prefer_plain_starship: options.prefer_plain_starship,
            starship_enabled: options.starship_enabled,
            zsh_highlighting_enabled: options.zsh_highlighting_enabled,
            fastfetch_autorun: options.fastfetch_autorun,
            auto_theme_enabled: options.auto_theme_enabled,
        }
    }
}

pub(crate) fn build_shell_integration_files(
    theme: &ThemeVariant,
    options: &ShellIntegrationOptions<'_>,
) -> ShellIntegrationFiles {
    let model = SharedShellModel::new(theme, options);

    ShellIntegrationFiles {
        zsh: render_posix_shell(&model, PosixShell::Zsh),
        bash: render_posix_shell(&model, PosixShell::Bash),
        fish: render_fish_shell(&model),
    }
}

fn render_posix_path_entries(content: &mut String, model: &SharedShellModel) {
    for entry in &model.path_entries {
        content.push_str(&format!(
            "if [ -d {quoted} ]; then\n  case \":$PATH:\" in\n    *:{raw}:*) ;;\n    *) export PATH={quoted}:\"$PATH\" ;;\n  esac\nfi\n",
            quoted = entry.quoted,
            raw = entry.raw
        ));
    }
}

fn render_shared_exports(content: &mut String, model: &SharedShellModel) {
    content.push_str(&format!("export BAT_THEME={}\n", model.bat_theme));
    content.push_str(&format!("export EZA_CONFIG_DIR={}\n", model.eza_config_dir));
    content.push_str(&format!("export LG_CONFIG_FILE={}\n", model.lg_config_file));
    content.push_str(&format!("export LS_COLORS={}\n", model.ls_colors));
    content.push_str(&format!("export EZA_COLORS={}\n", model.eza_colors));
}

fn render_posix_fastfetch_wrapper(content: &mut String, model: &SharedShellModel) {
    content.push_str(&format!(
        "fastfetch() {{ command fastfetch -c {} \"$@\"; }}\n",
        model.fastfetch_config_path
    ));
}

fn render_zsh_highlighting(content: &mut String, model: &SharedShellModel) {
    if !model.zsh_highlighting_enabled {
        return;
    }

    if let Some(plugin_path) = &model.zsh_highlighting_plugin_path {
        content.push_str(&format!(
            "if [ -f {plugin} ]; then\n  source {plugin}\nfi\n",
            plugin = plugin_path
        ));
    }

    content.push_str(&format!(
        "if [ -f {styles} ]; then\n  source {styles}\nfi\n",
        styles = model.zsh_highlight_styles_path
    ));
}

fn render_posix_fastfetch_autorun(content: &mut String) {
    content.push_str("if command -v fastfetch >/dev/null 2>&1; then\n");
    content.push_str("  fastfetch\n");
    content.push_str("fi\n");
}

fn render_posix_auto_theme(content: &mut String, model: &SharedShellModel) {
    content.push_str(&format!(
        "if [ \"${{TERM_PROGRAM:-}}\" = \"ghostty\" ] || [ \"${{TERM_PROGRAM:-}}\" = \"Ghostty\" ]; then\n  if [ -x {notify} ]; then\n    if ! pgrep -f \"slate-dark-mode-notify\" >/dev/null 2>&1; then\n      {notify} {slate_bin} theme --auto --quiet >/dev/null 2>&1 &\n    fi\n  fi\nfi\n",
        notify = model.notify_path,
        slate_bin = model.slate_bin
    ));
}

fn render_posix_starship(content: &mut String, model: &SharedShellModel, shell: PosixShell) {
    if model.prefer_plain_starship {
        content.push_str(&format!(
            "export STARSHIP_CONFIG={}\n",
            model.plain_starship_path
        ));
    } else {
        content.push_str(&format!(
            "if [ \"${{TERM_PROGRAM:-}}\" = \"Apple_Terminal\" ]; then\n  export STARSHIP_CONFIG={plain}\nelif [ -f {active} ]; then\n  export STARSHIP_CONFIG={active}\nelse\n  export STARSHIP_CONFIG={plain}\nfi\n",
            active = model.active_starship_path,
            plain = model.plain_starship_path
        ));
    }

    content.push_str("\nif command -v starship >/dev/null 2>&1; then\n");
    content.push_str(&format!(
        "  eval \"$(starship init {})\"\n",
        shell.starship_shell()
    ));
    content.push_str("fi\n");
}

fn render_posix_shell(model: &SharedShellModel, shell: PosixShell) -> String {
    let mut content = String::new();

    render_posix_path_entries(&mut content, model);
    render_shared_exports(&mut content, model);
    render_posix_fastfetch_wrapper(&mut content, model);

    if shell == PosixShell::Zsh {
        render_zsh_highlighting(&mut content, model);
    }

    if model.fastfetch_autorun {
        render_posix_fastfetch_autorun(&mut content);
    }

    if model.auto_theme_enabled {
        render_posix_auto_theme(&mut content, model);
    }

    if model.starship_enabled {
        render_posix_starship(&mut content, model, shell);
    } else {
        content.push_str("\n# Minimal prompt (starship disabled)\n");
        match shell {
            PosixShell::Zsh => content.push_str("PROMPT=$'%n\\n❯ '\n"),
            PosixShell::Bash => content.push_str("PS1=$'\\u\\n❯ '\n"),
        }
    }

    content
}

fn render_fish_path_entries(content: &mut String, model: &SharedShellModel) {
    for entry in &model.path_entries {
        content.push_str(&format!(
            "if test -d {quoted}\n  if not contains -- {quoted} $PATH\n    set -gx PATH {quoted} $PATH\n  end\nend\n",
            quoted = entry.quoted
        ));
    }
}

fn render_fish_shell(model: &SharedShellModel) -> String {
    let mut content = String::new();

    render_fish_path_entries(&mut content, model);
    content.push_str(&format!("set -gx BAT_THEME {}\n", model.bat_theme));
    content.push_str(&format!(
        "set -gx EZA_CONFIG_DIR {}\n",
        model.eza_config_dir
    ));
    content.push_str(&format!(
        "set -gx LG_CONFIG_FILE {}\n",
        model.lg_config_file
    ));
    content.push_str(&format!("set -gx LS_COLORS {}\n", model.ls_colors));
    content.push_str(&format!("set -gx EZA_COLORS {}\n", model.eza_colors));
    content.push_str(&format!(
        "function fastfetch\n  command fastfetch -c {} $argv\nend\n",
        model.fastfetch_config_path
    ));

    if model.fastfetch_autorun {
        content.push_str("if command -sq fastfetch\n  fastfetch\nend\n");
    }

    if model.auto_theme_enabled {
        content.push_str(&format!(
            "if test \"$TERM_PROGRAM\" = \"ghostty\"\n  if test -x {notify}\n    if not pgrep -f \"slate-dark-mode-notify\" >/dev/null 2>&1\n      {notify} {slate_bin} theme --auto --quiet >/dev/null 2>&1 &\n    end\n  end\nelse if test \"$TERM_PROGRAM\" = \"Ghostty\"\n  if test -x {notify}\n    if not pgrep -f \"slate-dark-mode-notify\" >/dev/null 2>&1\n      {notify} {slate_bin} theme --auto --quiet >/dev/null 2>&1 &\n    end\n  end\nend\n",
            notify = model.notify_path,
            slate_bin = model.slate_bin
        ));
    }

    if model.starship_enabled {
        if model.prefer_plain_starship {
            content.push_str(&format!(
                "set -gx STARSHIP_CONFIG {}\n",
                model.plain_starship_path
            ));
        } else {
            content.push_str(&format!(
                "if test \"$TERM_PROGRAM\" = \"Apple_Terminal\"\n  set -gx STARSHIP_CONFIG {plain}\nelse if test -f {active}\n  set -gx STARSHIP_CONFIG {active}\nelse\n  set -gx STARSHIP_CONFIG {plain}\nend\n",
                active = model.active_starship_path,
                plain = model.plain_starship_path
            ));
        }

        content.push_str("\nif command -sq starship\n  starship init fish | source\nend\n");
    } else {
        content.push_str("\n# Minimal prompt (starship disabled)\n");
        content.push_str("function fish_prompt\n  printf '%s\\n❯ ' $USER\nend\n");
    }

    content
}

fn default_powerline_fg(theme: &ThemeVariant) -> String {
    let p = &theme.palette;
    if theme.appearance == crate::theme::ThemeAppearance::Light {
        crate::wcag::pick_light_powerline_fg(p)
    } else {
        p.bg_darkest.clone().unwrap_or_else(|| p.black.clone())
    }
}

fn powerline_fg_for_bg(theme: &ThemeVariant, bg: &str) -> String {
    let p = &theme.palette;
    let bg_darkest = p.bg_darkest.as_deref().unwrap_or(&p.black);
    let default_fg = default_powerline_fg(theme);
    crate::wcag::pick_accessible_fg_for_bg(
        &[
            default_fg.as_str(),
            bg_darkest,
            p.foreground.as_str(),
            p.background.as_str(),
            p.black.as_str(),
            p.white.as_str(),
        ],
        bg,
    )
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
        "powerline_fg" => default_powerline_fg(theme),
        "powerline_fg_red" => powerline_fg_for_bg(theme, &p.red),
        "powerline_fg_peach" => {
            let bg = starship_palette_value(theme, "peach");
            powerline_fg_for_bg(theme, &bg)
        }
        "powerline_fg_yellow" => powerline_fg_for_bg(theme, &p.yellow),
        "powerline_fg_green" => powerline_fg_for_bg(theme, &p.green),
        "powerline_fg_sapphire" => {
            let bg = starship_palette_value(theme, "sapphire");
            powerline_fg_for_bg(theme, &bg)
        }
        "powerline_fg_lavender" => {
            let bg = starship_palette_value(theme, "lavender");
            powerline_fg_for_bg(theme, &bg)
        }
        _ => String::new(),
    }
}

fn with_slate_palette(base: &str, theme: &ThemeVariant) -> String {
    let mut content = base.to_string();
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
        "powerline_fg_red",
        "powerline_fg_peach",
        "powerline_fg_yellow",
        "powerline_fg_green",
        "powerline_fg_sapphire",
        "powerline_fg_lavender",
    ] {
        content.push_str(&format!(
            "{key} = \"{}\"\n",
            starship_palette_value(theme, key)
        ));
    }
    content
}

#[allow(dead_code)]
pub(crate) fn themed_starship_content(theme: &ThemeVariant) -> String {
    with_slate_palette(DEFAULT_STARSHIP_CONTENT, theme)
}

pub(crate) fn themed_plain_starship_content(theme: &ThemeVariant) -> String {
    with_slate_palette(BASIC_STARSHIP_CONTENT, theme)
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
            prefer_plain_starship: false,
            starship_enabled: true,
            zsh_highlighting_enabled: true,
            fastfetch_autorun: true,
            auto_theme_enabled: false,
        }
    }

    fn sample_files() -> ShellIntegrationFiles {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        build_shell_integration_files(&theme, &sample_options())
    }

    #[test]
    fn test_shell_integration_prepends_homebrew_paths() {
        let files = sample_files();

        assert!(files
            .zsh
            .contains("export PATH='/tmp/.local/bin':\"$PATH\""));
        assert!(files
            .zsh
            .contains("export PATH='/opt/homebrew/bin':\"$PATH\""));
        assert!(files
            .zsh
            .contains("export PATH='/opt/homebrew/sbin':\"$PATH\""));
    }

    #[test]
    fn test_shell_integration_exports_active_starship_config() {
        let files = sample_files();

        assert!(files
            .zsh
            .contains("if [ \"${TERM_PROGRAM:-}\" = \"Apple_Terminal\" ]; then"));
        assert!(files
            .zsh
            .contains("elif [ -f '/tmp/.config/starship.toml' ]; then"));
        assert!(files
            .zsh
            .contains("export STARSHIP_CONFIG='/tmp/.config/starship.toml'"));
        assert!(files
            .zsh
            .contains("export STARSHIP_CONFIG='/tmp/slate/managed/starship/plain.toml'"));
    }

    #[test]
    fn test_shell_integration_uses_plain_starship_in_terminal_app() {
        let files = sample_files();

        assert!(files
            .zsh
            .contains("if [ \"${TERM_PROGRAM:-}\" = \"Apple_Terminal\" ]; then"));
        assert!(files
            .zsh
            .contains("export STARSHIP_CONFIG='/tmp/slate/managed/starship/plain.toml'"));
    }

    #[test]
    fn test_shell_integration_can_disable_starship() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let mut options = sample_options();
        options.starship_enabled = false;

        let files = build_shell_integration_files(&theme, &options);

        assert!(!files.zsh.contains("starship init zsh"));
        assert!(!files.bash.contains("starship init bash"));
        assert!(!files.fish.contains("starship init fish | source"));
        assert!(!files.zsh.contains("export STARSHIP_CONFIG="));
    }

    #[test]
    fn test_shell_integration_can_disable_zsh_highlighting() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let mut options = sample_options();
        options.zsh_highlighting_enabled = false;

        let files = build_shell_integration_files(&theme, &options);

        assert!(!files.zsh.contains("zsh-syntax-highlighting.zsh"));
        assert!(!files.zsh.contains("highlight-styles.sh"));
        assert!(!files.bash.contains("zsh-syntax-highlighting.zsh"));
        assert!(!files.fish.contains("zsh-syntax-highlighting.zsh"));
    }

    #[test]
    fn test_shell_integration_can_force_plain_starship_profile() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let mut options = sample_options();
        options.prefer_plain_starship = true;

        let files = build_shell_integration_files(&theme, &options);

        assert!(files
            .zsh
            .contains("export STARSHIP_CONFIG='/tmp/slate/managed/starship/plain.toml'"));
        assert!(!files
            .zsh
            .contains("elif [ -f '/tmp/.config/starship.toml' ]; then"));
    }

    #[test]
    fn test_bash_renderer_uses_bash_specific_starship_init() {
        let files = sample_files();

        assert!(files.bash.contains("starship init bash"));
        assert!(!files.bash.contains("starship init zsh"));
        assert!(!files.bash.contains("zsh-syntax-highlighting"));
        assert!(!files.bash.contains("${TERM_PROGRAM:l}"));
        assert!(!files.bash.contains("&!"));
    }

    #[test]
    fn test_fish_renderer_uses_fish_exports_and_starship_init() {
        let files = sample_files();

        assert!(files.fish.contains("set -gx BAT_THEME "));
        assert!(files.fish.contains("set -gx EZA_CONFIG_DIR "));
        assert!(files.fish.contains("set -gx LS_COLORS "));
        assert!(files.fish.contains("set -gx EZA_COLORS "));
        assert!(files.fish.contains("function fastfetch"));
        assert!(files.fish.contains("starship init fish | source"));
    }

    // - tests: LS_COLORS / EZA_COLORS env-var wiring ---

    fn sample_theme() -> crate::theme::ThemeVariant {
        crate::theme::catppuccin::catppuccin_mocha().unwrap()
    }

    fn sample_model() -> SharedShellModel {
        SharedShellModel::new(&sample_theme(), &sample_options())
    }

    /// Strip the surrounding single quotes that `shell_quote` adds, so tests
    /// can inspect the raw rendered `LS_COLORS` / `EZA_COLORS` body.
    fn strip_shell_quotes(value: &str) -> &str {
        value
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .unwrap_or(value)
    }

    #[test]
    fn shared_shell_model_has_ls_colors_field() {
        let model = sample_model();
        let raw = strip_shell_quotes(&model.ls_colors);
        assert!(
            !raw.is_empty(),
            "ls_colors should be populated (got empty string)",
        );
        assert!(
            raw.contains("rs=0"),
            "ls_colors should start with `rs=0` sentinel; got={raw}",
        );
    }

    #[test]
    fn shared_shell_model_has_eza_colors_field() {
        let model = sample_model();
        let raw = strip_shell_quotes(&model.eza_colors);
        assert!(
            raw.starts_with("reset"),
            "eza_colors should start with `reset` sentinel; got={raw}",
        );
    }

    #[test]
    fn render_shared_exports_includes_ls_colors_line() {
        let mut content = String::new();
        let model = sample_model();
        render_shared_exports(&mut content, &model);
        assert!(
            content.contains("export LS_COLORS="),
            "expected `export LS_COLORS=` line; got content={content}",
        );
    }

    #[test]
    fn render_shared_exports_includes_eza_colors_line() {
        let mut content = String::new();
        let model = sample_model();
        render_shared_exports(&mut content, &model);
        assert!(
            content.contains("export EZA_COLORS="),
            "expected `export EZA_COLORS=` line; got content={content}",
        );
    }

    #[test]
    fn render_shared_exports_ls_colors_is_shell_quoted() {
        let mut content = String::new();
        let model = sample_model();
        render_shared_exports(&mut content, &model);
        // Match `export LS_COLORS='<body>'\n` — single-quoted value + newline.
        let pattern = regex::Regex::new(r"export LS_COLORS='[^']+'\n").expect("regex");
        assert!(
            pattern.is_match(&content),
            "LS_COLORS export must be single-quoted; content={content}",
        );
        let eza_pattern = regex::Regex::new(r"export EZA_COLORS='[^']+'\n").expect("regex");
        assert!(
            eza_pattern.is_match(&content),
            "EZA_COLORS export must be single-quoted; content={content}",
        );
    }

    #[test]
    fn render_fish_shell_uses_set_gx_for_ls_colors() {
        let fish = render_fish_shell(&sample_model());
        assert!(
            fish.contains("set -gx LS_COLORS "),
            "fish renderer must emit `set -gx LS_COLORS ` line; got fish={fish}",
        );
    }

    #[test]
    fn render_fish_shell_uses_set_gx_for_eza_colors() {
        let fish = render_fish_shell(&sample_model());
        assert!(
            fish.contains("set -gx EZA_COLORS "),
            "fish renderer must emit `set -gx EZA_COLORS ` line; got fish={fish}",
        );
    }

    #[test]
    fn fish_does_not_use_export_syntax_for_ls_eza() {
        let fish = render_fish_shell(&sample_model());
        assert!(
            !fish.contains("export LS_COLORS"),
            "fish renderer must not use POSIX `export LS_COLORS` syntax (D-A2)",
        );
        assert!(
            !fish.contains("export EZA_COLORS"),
            "fish renderer must not use POSIX `export EZA_COLORS` syntax (D-A2)",
        );
    }

    #[test]
    fn ls_colors_string_contains_truecolor_code() {
        // Confirms the end-to-end truecolor pipeline: palette → render_strings
        // → shell-quoted field → ready-to-emit string.
        let model = sample_model();
        let raw_ls = strip_shell_quotes(&model.ls_colors);
        let raw_eza = strip_shell_quotes(&model.eza_colors);
        let pattern = regex::Regex::new(r"38;2;\d+;\d+;\d+").expect("regex");
        assert!(
            pattern.is_match(raw_ls),
            "ls_colors must contain 24-bit truecolor code; got={raw_ls}",
        );
        assert!(
            pattern.is_match(raw_eza),
            "eza_colors must contain 24-bit truecolor code; got={raw_eza}",
        );
    }

    #[test]
    fn test_fish_renderer_does_not_use_posix_export_syntax() {
        let files = sample_files();

        assert!(!files.fish.contains("export BAT_THEME="));
        assert!(!files
            .fish
            .contains("source '/opt/homebrew/share/zsh-syntax-highlighting"));
    }

    #[test]
    fn test_auto_theme_renderer_avoids_zsh_only_syntax_for_bash_and_fish() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let mut options = sample_options();
        options.auto_theme_enabled = true;

        let files = build_shell_integration_files(&theme, &options);

        assert!(files.zsh.contains("theme --auto --quiet >/dev/null 2>&1 &"));
        assert!(files
            .bash
            .contains("theme --auto --quiet >/dev/null 2>&1 &"));
        assert!(files
            .fish
            .contains("theme --auto --quiet >/dev/null 2>&1 &"));
        assert!(!files.bash.contains("${TERM_PROGRAM:l}"));
        assert!(!files.fish.contains("${TERM_PROGRAM:l}"));
        assert!(!files.bash.contains("&!"));
        assert!(!files.fish.contains("&!"));
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
        assert!(content.contains("powerline_fg_red = "));
        assert!(content.contains("peach = "));
    }

    #[test]
    fn test_default_starship_content_uses_per_segment_powerline_fgs() {
        let content = starter_starship_content();

        assert!(content.contains("bg:red fg:powerline_fg_red"));
        assert!(content.contains("fg:powerline_fg_yellow bg:yellow"));
        assert!(content.contains("fg:powerline_fg_green bg:green"));
        assert!(content.contains("fg:powerline_fg_sapphire bg:sapphire"));
        assert!(content.contains("fg:powerline_fg_lavender bg:lavender"));
    }

    #[test]
    fn test_themed_plain_starship_content_is_ascii_only() {
        let theme = crate::theme::catppuccin::catppuccin_mocha().unwrap();
        let content = themed_plain_starship_content(&theme);

        assert!(content.contains("symbol = \"git:\""));
        assert!(content.contains("success_symbol = \"[>](bold fg:green)\""));
        assert!(!content.contains("󰀵"));
        assert!(!content.contains(""));
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
