use super::*;
use toml_edit::{TableLike, Value};

const CLEAN: &str = r#"
add_newline = true
right_format = ""
format = "$directory$git_branch$git_status$cmd_duration$line_break$character"

[directory]
format = "[$path]($style) "
style = "bold blue"

[git_branch]
symbol = "on "
format = "[$symbol$branch]($style) "
style = "lavender"

[git_status]
format = "([$all_status$ahead_behind]($style) )"
style = "yellow"

[cmd_duration]
format = "[$duration]($style) "
style = "yellow"

[character]
success_symbol = "[>](bold green)"
error_symbol = "[x](bold red)"
vimcmd_symbol = "[<](bold green)"
"#;

// Hostname visibility follows Starship's SSH-only default, or the user's own
// ssh_only/detect_env_vars/disabled rules. Do not overwrite these with a preset.
const CLASSIC: &str = r#"
add_newline = true
right_format = ""
format = "$username$hostname$directory$git_branch$git_status$line_break$character"

[username]
show_always = true
format = "[$user]($style)"
style_user = "bold lavender"
style_root = "bold red"

[hostname]
format = "[@$hostname]($style)"
style = "bold lavender"

[directory]
format = " [$path]($style) "
style = "bold blue"

[git_branch]
symbol = "on "
format = "[$symbol$branch]($style) "
style = "lavender"

[git_status]
format = "([$all_status$ahead_behind]($style) )"
style = "yellow"

[character]
format = "$symbol "
success_symbol = '[\$](bold green)'
error_symbol = '[\$](bold red)'
vimcmd_symbol = '[<](bold green)'
"#;

fn preset(style: PromptStyle) -> DocumentMut {
    let mut doc: DocumentMut = match style {
        PromptStyle::Rainbow => super::super::shell_integration::starter_starship_content(),
        PromptStyle::Minimal | PromptStyle::Compact | PromptStyle::Focus | PromptStyle::Branch => {
            CLEAN
        }
        PromptStyle::Classic => CLASSIC,
    }
    .parse()
    .expect("bundled prompt template");
    doc["right_format"] = toml_edit::value("");
    if style == PromptStyle::Compact {
        doc["add_newline"] = toml_edit::value(false);
        doc["format"] = toml_edit::value("$directory$git_branch$git_status$character");
    }
    if style == PromptStyle::Focus {
        doc["add_newline"] = toml_edit::value(false);
        doc["format"] = toml_edit::value("$directory$character");
    }
    if style == PromptStyle::Branch {
        doc["add_newline"] = toml_edit::value(false);
        doc["format"] = toml_edit::value("$directory$git_branch$character");
    }
    doc
}

// Presets own presentation, not commands, timeouts, detection rules, custom
// modules or project-specific directory substitutions. Palette values are
// handled separately so unrelated strings never enter legacy token rewriting.
fn overlay(target: &mut dyn TableLike, source: &dyn TableLike, root: bool) -> Result<()> {
    for (key, item) in source.iter() {
        if let Some(value) = item.as_value() {
            let controlled = if root {
                matches!(key, "format" | "right_format" | "add_newline")
            } else {
                matches!(
                    key,
                    "format"
                        | "style"
                        | "style_user"
                        | "style_root"
                        | "symbol"
                        | "success_symbol"
                        | "error_symbol"
                        | "vimcmd_symbol"
                        | "show_always"
                        | "disabled"
                )
            };
            if controlled {
                super::super::flags::set_value(target, key, value.clone());
            }
        } else if root {
            if let Some(source_table) = item.as_table_like() {
                if !source
                    .get("format")
                    .and_then(Item::as_str)
                    .is_some_and(|format| format.contains(&format!("${key}")))
                {
                    continue;
                }
                if !target.contains_key(key) {
                    target.insert(key, toml_edit::table());
                }
                let table = target
                    .get_mut(key)
                    .and_then(Item::as_table_like_mut)
                    .ok_or_else(|| invalid("a preset module must be a TOML table"))?;
                overlay(table, source_table, false)?;
            }
        }
    }
    Ok(())
}

pub(super) fn layout(content: &str, style: PromptStyle) -> Result<String> {
    let mut doc: DocumentMut = content
        .parse()
        .map_err(|_| invalid("Starship configuration contains invalid TOML"))?;
    overlay(doc.as_table_mut(), preset(style).as_table(), true)?;
    Ok(doc.to_string())
}

/// Compare only the presentation fields that preset selection owns. Never
/// execute custom commands, rewrite user files, or infer a live prompt match.
pub(crate) fn matches_layout(content: &str, style: PromptStyle, plain: bool) -> Result<bool> {
    let mut original: DocumentMut = content
        .parse()
        .map_err(|_| invalid("Starship configuration contains invalid TOML"))?;
    // An absent right_format has the same empty value as our explicit preset.
    if !original.contains_key("right_format") {
        original["right_format"] = toml_edit::value("");
    }
    let mut expected = if plain && style == PromptStyle::Rainbow {
        super::super::shell_integration::plain_starship_template()
            .parse::<DocumentMut>()
            .expect("bundled plain template")
    } else {
        preset(style)
    };
    expected["right_format"] = toml_edit::value("");
    let mut overlaid = original.clone();
    overlay(overlaid.as_table_mut(), expected.as_table(), true)?;
    let value = |doc: DocumentMut| {
        doc.to_string()
            .parse::<toml::Value>()
            .map_err(|_| invalid("cannot compare presentation fields"))
    };
    Ok(value(original)? == value(overlaid)?)
}

pub(super) fn render(content: &str, theme: &ThemeVariant, style: PromptStyle) -> Result<String> {
    theme.validate()?;
    let mut doc: DocumentMut = layout(content, style)?.parse().expect("serialized layout");
    // Reuse the palette mapper without handing it any personal config values.
    let palette: DocumentMut = crate::adapter::starship::themed_config_from_content("", theme)?
        .parse()
        .expect("serialized palette");
    super::super::flags::set_value(doc.as_table_mut(), "palette", Value::from("slate"));
    if !doc.contains_key("palettes") {
        doc.insert("palettes", toml_edit::table());
    }
    let palettes = doc
        .get_mut("palettes")
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| invalid("[palettes] must be a table"))?;
    if !palettes.contains_key("slate") {
        palettes.insert("slate", toml_edit::table());
    }
    let slate = palettes
        .get_mut("slate")
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| invalid("[palettes.slate] must be a table"))?;
    for (key, value) in palette["palettes"]["slate"]
        .as_table()
        .expect("palette table")
        .iter()
    {
        super::super::flags::set_value(slate, key, value.as_value().expect("color string").clone());
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests;
