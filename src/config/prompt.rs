//! Explicit layout presets; ordinary theme changes only recolor the chosen layout.
use super::{flags, ConfigManager};
use crate::{
    env::SlateEnv,
    error::{Result, SlateError},
    theme::ThemeVariant,
};
use serde::Serialize;
use toml_edit::{DocumentMut, Item};

mod change;
mod style;
pub(crate) use change::{PreparedPrompt, PromptPreview};
pub(crate) use style::matches_layout;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptStyle {
    Rainbow,
    Minimal,
    Compact,
    Classic,
    Focus,
    Branch,
}

impl PromptStyle {
    pub const ALL: [Self; 6] = [
        Self::Rainbow,
        Self::Minimal,
        Self::Compact,
        Self::Classic,
        Self::Focus,
        Self::Branch,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Rainbow => "rainbow",
            Self::Minimal => "minimal",
            Self::Compact => "compact",
            Self::Classic => "classic",
            Self::Focus => "focus",
            Self::Branch => "branch",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Rainbow => "Rainbow segments",
            Self::Minimal => "Clean two-line",
            Self::Compact => "Compact one-line",
            Self::Classic => "Classic shell",
            Self::Focus => "Focus one-line",
            Self::Branch => "Branch one-line",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Rainbow => "Colored segments, user, folder and time; icon font recommended",
            Self::Minimal => "Folder and Git on one line, input below; no icon font needed",
            Self::Compact => "Folder, Git and input on a single line; no icon font needed",
            Self::Classic => {
                "User, folder and Git; SSH host by default; two lines, no icon font needed"
            }
            Self::Focus => {
                "Only folder and input; no Git, clock or host display; no icon font needed"
            }
            Self::Branch => "Folder and Git branch only; no change counts, duration or clock; no icon font needed",
        }
    }
    pub fn sample(self) -> &'static str {
        match self {
            Self::Rainbow => "[ user ][ ~/project ][ main ][ 20:44 ]\n> ",
            Self::Minimal => "~/project on main\n> ",
            Self::Compact => "~/project on main > ",
            Self::Classic => "user@host ~/project on main\n$ ",
            Self::Focus => "~/project > ",
            Self::Branch => "~/project on main > ",
        }
    }
}

fn invalid(message: &str) -> SlateError {
    SlateError::InvalidConfig(format!("Prompt style: {message}; file contents omitted"))
}

pub(crate) fn saved_style(env: &SlateEnv) -> Result<Option<PromptStyle>> {
    let Some(doc) = flags::read_document(&env.managed_file("config.toml"))? else {
        return Ok(None);
    };
    let Some(section) = doc.get("prompt") else {
        return Ok(None);
    };
    let section = section
        .as_table_like()
        .ok_or_else(|| invalid("[prompt] must be a table"))?;
    section
        .get("style")
        .map(|value| {
            let id = value
                .as_str()
                .ok_or_else(|| invalid("[prompt].style must be a string"))?;
            PromptStyle::ALL
                .into_iter()
                .find(|style| style.id() == id)
                .ok_or_else(|| {
                    invalid(&format!(
                        "unknown saved style; choose {}",
                        PromptStyle::ALL.map(PromptStyle::id).join(", ")
                    ))
                })
        })
        .transpose()
}

pub(crate) fn plain_content(env: &SlateEnv, theme: &ThemeVariant) -> Result<String> {
    match saved_style(env)? {
        None | Some(PromptStyle::Rainbow) => Ok(
            super::shell_integration::themed_plain_starship_content(theme),
        ),
        Some(style) => style::render("", theme, style),
    }
}

pub(crate) fn starter_content(env: &SlateEnv) -> Result<String> {
    match saved_style(env)? {
        None => Ok(super::shell_integration::starter_starship_content().to_owned()),
        Some(style) => style::layout("", style),
    }
}

fn preference_bytes(original: Option<&[u8]>) -> Result<DocumentMut> {
    let Some(bytes) = original else {
        return Ok(DocumentMut::new());
    };
    let content = std::str::from_utf8(bytes).map_err(|_| invalid("settings are not UTF-8"))?;
    content
        .parse()
        .map_err(|_| invalid("settings contain invalid TOML"))
}

fn remember(doc: &mut DocumentMut, style: PromptStyle) -> Result<()> {
    if !doc.contains_key("prompt") {
        doc.insert("prompt", toml_edit::table());
    }
    let section = doc
        .get_mut("prompt")
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| invalid("[prompt] must be a table"))?;
    flags::set_value(section, "style", style.id().into());
    Ok(())
}

impl ConfigManager {
    pub fn get_prompt_style(&self) -> Result<Option<PromptStyle>> {
        saved_style(self.environment())
    }
}
