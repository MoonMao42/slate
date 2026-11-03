//! Audit serialized tool settings, including inherited backgrounds. No dotfiles
//! or running applications are touched; native loaders have separate smoke tests.
use super::*;
use crate::{theme::ThemeRegistry, wcag::contrast_hex};
use regex::Regex;
use std::collections::BTreeMap;

#[derive(Default)]
struct Audit {
    checked: usize,
    failures: Vec<String>,
}
impl Audit {
    fn pair(&mut self, scope: &str, fg: &str, bg: &str) {
        self.checked += 1;
        let ratio = contrast_hex(fg, bg);
        if ratio < 4.5 {
            self.failures
                .push(format!("{scope}: {fg} on {bg} = {ratio:.3}:1"));
        }
    }
    fn finish(self) {
        eprintln!("Audited {} rendered text/background pairs", self.checked);
        assert!(
            self.failures.is_empty(),
            "{} failing pairs:\n{}",
            self.failures.len(),
            self.failures.join("\n")
        );
    }
}
fn hexes(content: &str) -> BTreeMap<String, String> {
    Regex::new(r#"([\w+-]+)[\] ]*[=:][\s\[\"']*(#[a-fA-F0-9]{6})"#)
        .unwrap()
        .captures_iter(content)
        .map(|c| (c[1].to_owned(), c[2].to_owned()))
        .collect()
}
fn plist_colors(content: &str) -> BTreeMap<String, String> {
    Regex::new(r"<key>(\w+)</key>\s*<string>(#[a-fA-F0-9]{6})</string>")
        .unwrap()
        .captures_iter(content)
        .map(|c| (c[1].to_owned(), c[2].to_owned()))
        .collect()
}

#[test]
fn rendered_tool_colors_meet_text_contrast_for_every_theme() {
    let mut audit = Audit::default();
    let matches = Regex::new(r"#[a-fA-F0-9]{6}").unwrap();
    let rgb = Regex::new(r"38;2;(\d+);(\d+);(\d+)").unwrap();
    let fields = Regex::new(r"(fg|bg) = '(#[a-fA-F0-9]{6})'").unwrap();
    let settings = Regex::new(r"(?s)<key>settings</key>\s*<dict>(.*?)</dict>").unwrap();
    let segment = Regex::new(r"fg = '(#[a-fA-F0-9]{6})', bg = '(#[a-fA-F0-9]{6})'").unwrap();
    let delta_background =
        Regex::new(r#"(?:minus|plus)-style = syntax "(#[a-fA-F0-9]{6})""#).unwrap();
    let swatch = Regex::new(r"\x1b\[48;2;(\d+);(\d+);(\d+)m\x1b\[38;2;(\d+);(\d+);(\d+)m").unwrap();
    for theme in ThemeRegistry::new().unwrap().all() {
        let p = &theme.palette;
        let bg = &p.background;
        let scope = |tool: &str, role: &str| format!("{}/{tool}/{role}", theme.id);

        let preview = crate::cli::picker::preview_panel::render_preview(p);
        assert_eq!(swatch.captures_iter(&preview).count(), 16 + p.extras.len());
        for c in swatch.captures_iter(&preview) {
            let color = |offset: usize| {
                format!(
                    "#{:02x}{:02x}{:02x}",
                    c[offset].parse::<u8>().unwrap(),
                    c[offset + 1].parse::<u8>().unwrap(),
                    c[offset + 2].parse::<u8>().unwrap()
                )
            };
            audit.pair(&scope("picker", "swatch label"), &color(4), &color(1));
        }

        // Alacritty and Kitty: 16 ANSI colors plus selection and block cursor.
        let alacritty: toml::Value = AlacrittyAdapter::render_alacritty_colors(theme)
            .parse()
            .unwrap();
        for group in ["normal", "bright"] {
            for (key, value) in alacritty["colors"][group].as_table().unwrap() {
                audit.pair(&scope("alacritty", key), value.as_str().unwrap(), bg);
            }
        }
        for (group, bg_key) in [
            ("primary", "background"),
            ("selection", "background"),
            ("cursor", "cursor"),
        ] {
            let values = &alacritty["colors"][group];
            let fg_key = if group == "primary" {
                "foreground"
            } else {
                "text"
            };
            audit.pair(
                &scope("alacritty", group),
                values[fg_key].as_str().unwrap(),
                values[bg_key].as_str().unwrap(),
            );
        }
        let kitty: BTreeMap<_, _> = KittyAdapter::render_kitty_colors(theme)
            .lines()
            .filter_map(|l| l.split_once(' ').map(|(k, v)| (k.to_owned(), v.to_owned())))
            .collect();
        for index in 0..16 {
            audit.pair(
                &scope("kitty", &format!("color{index}")),
                &kitty[&format!("color{index}")],
                &kitty["background"],
            );
        }
        for (fg, bg) in [
            ("foreground", "background"),
            ("selection_foreground", "selection_background"),
            ("cursor_text_color", "cursor"),
        ] {
            audit.pair(&scope("kitty", fg), &kitty[fg], &kitty[bg]);
        }

        let home = tempfile::tempdir().unwrap();
        let env = crate::env::SlateEnv::with_home(home.path().into());
        std::fs::create_dir_all(env.xdg_config_home().join("ghostty")).unwrap();
        std::fs::write(env.xdg_config_home().join("ghostty/config"), "").unwrap();
        GhosttyAdapter.apply_theme_with_env(theme, &env).unwrap();
        let ghostty =
            std::fs::read_to_string(env.managed_file("managed/ghostty/theme.conf")).unwrap();
        let ghostty: BTreeMap<_, _> = ghostty
            .lines()
            .filter_map(|line| line.split_once(" = "))
            .filter_map(|(key, value)| {
                if key == "palette" {
                    value
                        .split_once('=')
                        .map(|(index, color)| (format!("palette{index}"), color.to_owned()))
                } else {
                    Some((key.to_owned(), value.to_owned()))
                }
            })
            .collect();
        for index in 0..16 {
            audit.pair(
                &scope("ghostty", &format!("palette{index}")),
                &ghostty[&format!("palette{index}")],
                &ghostty["background"],
            );
        }
        for (fg, bg) in [
            ("foreground", "background"),
            ("cursor-text", "cursor-color"),
            ("selection-foreground", "selection-background"),
        ] {
            audit.pair(&scope("ghostty", fg), &ghostty[fg], &ghostty[bg]);
        }

        let fzf = hexes(&ls_colors::render_fzf_colors(p));
        assert_eq!(fzf.len(), 13);
        for (key, fg) in &fzf {
            if matches!(key.as_str(), "bg" | "bg+") {
                continue;
            }
            let background = if key.ends_with('+') || key == "pointer" {
                &fzf["bg+"]
            } else {
                &fzf["bg"]
            };
            audit.pair(&scope("fzf", key), fg, background);
        }
        audit.pair(
            &scope("fzf", "selected marker"),
            &fzf["marker"],
            &fzf["bg+"],
        );

        let btop = hexes(&btop::palette::render(theme).unwrap());
        assert!(btop.len() >= 45);
        for (key, fg) in &btop {
            if key.ends_with("_bg") {
                continue;
            }
            let background = match key.as_str() {
                "selected_fg" => &btop["selected_bg"],
                "followed_fg" => &btop["followed_bg"],
                "proc_banner_fg" => &btop["proc_banner_bg"],
                _ => &btop["main_bg"],
            };
            audit.pair(&scope("btop", key), fg, background);
        }
        for key in ["proc_pause_bg", "proc_follow_bg"] {
            audit.pair(&scope("btop", key), &btop["proc_banner_fg"], &btop[key]);
        }

        let (yazi, _) = yazi::palette::render(theme).unwrap();
        let yazi: toml::Value = yazi.parse().unwrap();
        let surface = yazi["status"]["overall"]["bg"].as_str().unwrap();
        for (section, values) in yazi.as_table().unwrap() {
            for (key, style) in values.as_table().unwrap() {
                if key.starts_with("marker_") {
                    continue;
                } // Solid glyphs, not text.
                if let Some(fg) = style.get("fg").and_then(toml::Value::as_str) {
                    let background = style
                        .get("bg")
                        .and_then(toml::Value::as_str)
                        .unwrap_or(if section == "status" { surface } else { bg });
                    audit.pair(&scope("yazi", &format!("{section}.{key}")), fg, background);
                }
            }
        }
        for rule in yazi["filetype"]["rules"].as_array().unwrap() {
            audit.pair(&scope("yazi", "filetype"), rule["fg"].as_str().unwrap(), bg);
        }

        let zellij: kdl::KdlDocument = zellij::palette::render(theme).unwrap().parse().unwrap();
        let components = zellij
            .get("themes")
            .unwrap()
            .children()
            .unwrap()
            .get("slate-sync")
            .unwrap()
            .children()
            .unwrap();
        for component in components.nodes() {
            let values = component.children().unwrap();
            let background = values
                .get("background")
                .map(|v| v.entries()[0].value().as_string().unwrap())
                .unwrap_or(bg);
            for value in values.nodes() {
                if value.name().value() != "background" {
                    audit.pair(
                        &scope(
                            "zellij",
                            &format!("{}.{}", component.name().value(), value.name().value()),
                        ),
                        value.entries()[0].value().as_string().unwrap(),
                        background,
                    );
                }
            }
        }

        let lazygit = hexes(&LazygitAdapter.generate_yaml_config(theme).unwrap());
        assert!(lazygit.len() >= 12);
        for (key, fg) in &lazygit {
            if key.ends_with("BgColor") {
                continue;
            }
            let background = match key.as_str() {
                "cherryPickedCommitFgColor" => &lazygit["cherryPickedCommitBgColor"],
                "markedBaseCommitFgColor" => &lazygit["markedBaseCommitBgColor"],
                _ => bg,
            };
            audit.pair(&scope("lazygit", key), fg, background);
        }
        for fg in [
            &p.foreground,
            &p.red,
            &p.green,
            &p.yellow,
            &p.blue,
            &p.magenta,
            &p.cyan,
            &p.bright_black,
        ] {
            audit.pair(
                &scope("lazygit", "selected file/status"),
                fg,
                &lazygit["selectedLineBgColor"],
            );
        }
        for line in TmuxAdapter::render_tmux_colors(theme).lines().skip(1) {
            let colors = hexes(line);
            if let Some(fg) = colors.get("fg") {
                audit.pair(
                    &scope("tmux", line.split_whitespace().nth(2).unwrap()),
                    fg,
                    colors.get("bg").unwrap_or(bg),
                );
            }
        }

        // Foreground-only serializers: metadata, file kinds, syntax and logos.
        for (tool, output) in [
            ("eza", EzaAdapter::render_eza_yaml(theme)),
            (
                "zsh",
                ZshHighlightAdapter::render_highlight_styles(theme).unwrap(),
            ),
        ] {
            assert!(matches.find_iter(&output).count() > 8);
            for fg in matches.find_iter(&output) {
                audit.pair(&scope(tool, "foreground"), fg.as_str(), bg);
            }
        }
        for (tool, output) in [
            ("ls/eza", ls_colors::render_ls_colors(p)),
            ("eza environment", ls_colors::render_eza_colors(p)),
            (
                "fastfetch",
                FastfetchAdapter.generate_jsonc_config(theme).unwrap(),
            ),
        ] {
            assert!(rgb.is_match(&output));
            for c in rgb.captures_iter(&output) {
                let fg = format!(
                    "#{:02x}{:02x}{:02x}",
                    c[1].parse::<u8>().unwrap(),
                    c[2].parse::<u8>().unwrap(),
                    c[3].parse::<u8>().unwrap()
                );
                audit.pair(&scope(tool, "foreground"), &fg, bg);
            }
        }

        let opencode: serde_json::Value =
            serde_json::from_str(&opencode::palette::render(theme).unwrap()).unwrap();
        let colors = opencode["theme"].as_object().unwrap();
        assert_eq!(colors["background"], "none");
        for (key, value) in colors {
            if key.starts_with("background") || key.ends_with("Bg") {
                continue;
            }
            let fg = value.as_str().unwrap();
            if key == "selectedListItemText" {
                audit.pair(
                    &scope("opencode", key),
                    fg,
                    colors["primary"].as_str().unwrap(),
                );
            } else {
                for background in [
                    bg.as_str(),
                    colors["backgroundPanel"].as_str().unwrap(),
                    colors["backgroundElement"].as_str().unwrap(),
                    colors["diffAddedBg"].as_str().unwrap(),
                    colors["diffRemovedBg"].as_str().unwrap(),
                ] {
                    audit.pair(&scope("opencode", key), fg, background);
                }
            }
        }

        // Both Starship palette writers must agree, including text-only layouts.
        let seeded = crate::config::shell_integration::themed_starship_content(theme);
        let adapted = starship::themed_config_from_content(&seeded, theme).unwrap();
        for content in [&seeded, &adapted] {
            let doc: toml::Value = content.parse().unwrap();
            let colors = &doc["palettes"]["slate"];
            for accent in ["red", "peach", "yellow", "green", "sapphire", "lavender"] {
                audit.pair(
                    &scope("starship", accent),
                    colors[&format!("powerline_fg_{accent}")].as_str().unwrap(),
                    colors[accent].as_str().unwrap(),
                );
            }
            for key in [
                "red",
                "yellow",
                "green",
                "blue",
                "white",
                "foreground",
                "text",
                "peach",
                "sapphire",
                "lavender",
                "teal",
                "maroon",
                "sky",
                "pink",
            ] {
                audit.pair(&scope("starship", key), colors[key].as_str().unwrap(), bg);
            }
        }

        // Read the actual emitted editor backgrounds, then check inherited
        // syntax/comment colors under cursor lines, selections and diff lines.
        let nvim = nvim::render_colorscheme(p, &theme.id);
        let mut groups = BTreeMap::new();
        for line in nvim.lines() {
            if let Some((name, _)) = line.trim().split_once(" = {") {
                let colors: BTreeMap<_, _> = fields
                    .captures_iter(line)
                    .map(|c| (c[1].to_owned(), c[2].to_owned()))
                    .collect();
                if !colors.is_empty() {
                    groups.insert(name.to_owned(), colors);
                }
            }
        }
        assert!(groups.len() > 150);
        let overlay_names = [
            "Normal",
            "NormalFloat",
            "CursorLine",
            "Visual",
            "LspReferenceText",
            "DiffAdd",
            "DiffChange",
            "DiffDelete",
        ];
        let backgrounds: Vec<_> = overlay_names
            .iter()
            .map(|name| groups[*name]["bg"].as_str())
            .collect();
        for bg in &backgrounds {
            audit.pair(
                &scope("nvim", "plain text under overlay"),
                &groups["Normal"]["fg"],
                bg,
            );
        }
        for (name, colors) in &groups {
            if let Some(fg) = colors.get("fg") {
                if let Some(bg) = colors.get("bg") {
                    audit.pair(&scope("nvim", name), fg, bg);
                } else {
                    for bg in &backgrounds {
                        audit.pair(&scope("nvim", name), fg, bg);
                    }
                }
            }
        }
        for c in segment.captures_iter(&crate::design::nvim_highlights::lualine_theme(p)) {
            audit.pair(&scope("lualine", "segment"), &c[1], &c[2]);
        }

        let xml = bat::tmtheme::render_tmtheme(p, &theme.id);
        let dicts: Vec<_> = settings
            .captures_iter(&xml)
            .map(|c| plist_colors(&c[1]))
            .collect();
        assert!(dicts.len() > 20);
        let root = &dicts[0];
        audit.pair(
            &scope("bat", "gutter"),
            &root["gutterForeground"],
            &root["gutter"],
        );
        audit.pair(
            &scope("bat", "unpainted gutter"),
            &root["gutterForeground"],
            bg,
        );
        let delta = DeltaAdapter::render_delta_colors(theme).unwrap();
        let delta_colors = hexes(&delta);
        let diff_backgrounds: Vec<_> = delta_background
            .captures_iter(&delta)
            .map(|c| c[1].to_owned())
            .collect();
        assert_eq!(diff_backgrounds.len(), 2);
        for dict in &dicts {
            if let Some(fg) = dict.get("foreground") {
                if let Some(bg) = dict.get("background").filter(|_| dict != &dicts[0]) {
                    audit.pair(&scope("bat", "scope"), fg, bg);
                }
                for bg in [
                    &root["background"],
                    &root["selection"],
                    &root["lineHighlight"],
                ]
                .into_iter()
                .chain(diff_backgrounds.iter())
                {
                    audit.pair(&scope("bat/delta", "syntax"), fg, bg);
                }
            }
        }
        for (key, fg) in &delta_colors {
            audit.pair(&scope("delta", key), fg, bg);
        }
    }
    audit.finish();
}
