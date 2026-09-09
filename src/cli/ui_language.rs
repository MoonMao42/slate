use crate::{
    config::ui_language::{self, UiLanguage},
    env::SlateEnv,
    error::{Result, SlateError},
};
use std::cell::Cell;
use std::io::IsTerminal;

thread_local! { static CURRENT: Cell<UiLanguage> = const { Cell::new(UiLanguage::Chinese) }; }

pub(super) fn tr(zh: &'static str, en: &'static str) -> &'static str {
    CURRENT.with(|current| crate::config::ui_language::Text { zh, en }.get(current.get()))
}

pub(super) fn current() -> UiLanguage {
    CURRENT.with(Cell::get)
}

/// Display policy only. Never use this to grant non-interactive consent.
pub(crate) fn output_language() -> UiLanguage {
    language_for_output(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        current(),
    )
}

fn language_for_output(input_tty: bool, output_tty: bool, saved: UiLanguage) -> UiLanguage {
    if input_tty && output_tty {
        saved
    } else {
        UiLanguage::English
    }
}

/// Select UI wording without first-run consent or persistence. Broken profile
/// settings remain available to diagnostics using the existing default wording.
pub fn load_saved_ui_language(env: &SlateEnv) -> Result<()> {
    let language = selected_language(env)?;
    CURRENT.with(|current| current.set(language.unwrap_or(UiLanguage::Chinese)));
    Ok(())
}

fn selected_language(env: &SlateEnv) -> Result<Option<UiLanguage>> {
    if let Some(value) = std::env::var_os("SLATE_LANGUAGE") {
        Ok(Some(UiLanguage::parse(value.to_str().unwrap_or(""))?))
    } else {
        Ok(ui_language::read(env).ok().flatten())
    }
}

/// Reconcile after restoration or edits elsewhere without opening a picker or
/// writing preferences. Older/broken profiles retain this session's language.
pub(super) fn refresh(env: &SlateEnv) -> Result<()> {
    if let Some(language) = selected_language(env)? {
        CURRENT.with(|current| current.set(language));
    }
    Ok(())
}

pub(super) fn initialize(env: &SlateEnv) -> Result<bool> {
    if let Some(value) = std::env::var_os("SLATE_LANGUAGE") {
        let language = UiLanguage::parse(value.to_str().unwrap_or(""))?;
        CURRENT.with(|current| current.set(language));
        return Ok(true);
    }
    match ui_language::read(env) {
        Ok(Some(language)) => {
            CURRENT.with(|current| current.set(language));
            Ok(true)
        }
        Ok(None) => choose(env),
        // A broken general config must remain inspectable from the hub. Never
        // turn a failed read into permission to replace it with a new language.
        Err(_) => Ok(true),
    }
}

pub(super) fn choose(env: &SlateEnv) -> Result<bool> {
    let choice = ui_language::prepare_choice(env)?;
    let saved = choice.selected();
    let selected = super::menu::select("语言 / Language")
        .item(Some(UiLanguage::Chinese), "中文", "")
        .item(Some(UiLanguage::English), "English", "")
        .item(None, "返回 / Back", "")
        .initial_value(saved.or(Some(CURRENT.with(Cell::get))))
        .escape_value(None)
        .interact()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::Interrupted {
                SlateError::UserCancelled
            } else {
                error.into()
            }
        })?;
    let Some(language) = selected else {
        return Ok(false);
    };
    choice.save(language)?;
    CURRENT.with(|current| current.set(language));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirected_output_does_not_inherit_interactive_language() {
        for saved in [UiLanguage::Chinese, UiLanguage::English] {
            for (input, output) in [(false, false), (true, false), (false, true)] {
                assert_eq!(
                    language_for_output(input, output, saved),
                    UiLanguage::English
                );
            }
            assert_eq!(language_for_output(true, true, saved), saved);
        }
    }
}
