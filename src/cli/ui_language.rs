use crate::{
    config::ui_language::{self, UiLanguage},
    env::SlateEnv,
    error::{Result, SlateError},
};
use std::cell::Cell;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicU8, Ordering};

const LANG_CHINESE: u8 = 0;
const LANG_ENGLISH: u8 = 1;

static GLOBAL_CURRENT: AtomicU8 = AtomicU8::new(LANG_CHINESE);

const fn encode_language(language: UiLanguage) -> u8 {
    match language {
        UiLanguage::Chinese => LANG_CHINESE,
        UiLanguage::English => LANG_ENGLISH,
    }
}

const fn decode_language(raw: u8) -> UiLanguage {
    match raw {
        LANG_ENGLISH => UiLanguage::English,
        _ => UiLanguage::Chinese,
    }
}

thread_local! {
    static CURRENT: Cell<UiLanguage> = const { Cell::new(UiLanguage::Chinese) };
    #[cfg(test)]
    static INITIALIZED: Cell<bool> = const { Cell::new(false) };
}

fn set_current(language: UiLanguage) {
    GLOBAL_CURRENT.store(encode_language(language), Ordering::Relaxed);
    CURRENT.with(|current| current.set(language));
    #[cfg(test)]
    INITIALIZED.with(|initialized| initialized.set(true));
}

pub(crate) fn is_interactive_output() -> bool {
    let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    #[cfg(test)]
    {
        is_tty && INITIALIZED.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        is_tty
    }
}

pub(crate) fn tr(zh: &'static str, en: &'static str) -> &'static str {
    crate::config::ui_language::Text { zh, en }.get(current())
}

pub(crate) fn tr_output(zh: &'static str, en: &'static str) -> &'static str {
    crate::config::ui_language::Text { zh, en }.get(output_language())
}

pub(crate) fn current() -> UiLanguage {
    #[cfg(test)]
    {
        CURRENT.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        decode_language(GLOBAL_CURRENT.load(Ordering::Relaxed))
    }
}

/// Display policy only. Never use this to grant non-interactive consent.
pub(crate) fn output_language() -> UiLanguage {
    if is_interactive_output() {
        current()
    } else {
        UiLanguage::English
    }
}

#[cfg(test)]
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
    set_current(language.unwrap_or(UiLanguage::Chinese));
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
        set_current(language);
    }
    Ok(())
}

pub(super) fn initialize(env: &SlateEnv) -> Result<bool> {
    if let Some(value) = std::env::var_os("SLATE_LANGUAGE") {
        let language = UiLanguage::parse(value.to_str().unwrap_or(""))?;
        set_current(language);
        return Ok(true);
    }
    match ui_language::read(env) {
        Ok(Some(language)) => {
            set_current(language);
            Ok(true)
        }
        Ok(None) => choose(env),
        // A broken general config must remain inspectable from the hub. Never
        // turn a failed read into permission to replace it with a new language.
        Err(_) => {
            #[cfg(test)]
            INITIALIZED.with(|initialized| initialized.set(true));
            Ok(true)
        }
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
    set_current(language);
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

    #[test]
    fn atomic_language_encoding_round_trips_both_variants() {
        for language in [UiLanguage::Chinese, UiLanguage::English] {
            assert_eq!(decode_language(encode_language(language)), language);
        }
        assert_eq!(decode_language(u8::MAX), UiLanguage::Chinese);
    }
}
