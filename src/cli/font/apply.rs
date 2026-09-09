//! One application lifecycle for explicit names, picker selections and imports.
//! Presentation may differ during download, but failures and publication do not.
use super::{commit::FontCommit, ResolvedFontChoice};
use crate::{
    brand::{
        events::{dispatch, BrandEvent, FailureKind, SuccessKind},
        roles::Roles,
    },
    env::SlateEnv,
    error::{Result, SlateError},
    platform::fonts::FontCacheRefresh,
};

#[derive(Clone, Copy)]
pub(super) struct Options {
    pub auto: bool,
    pub quiet: bool,
    pub snapshot: bool,
}

pub(super) fn apply_choice(
    selection: &ResolvedFontChoice,
    env: &SlateEnv,
    roles: Option<&Roles<'_>>,
    options: Options,
    picker: bool,
) -> Result<()> {
    let cache = apply_with(
        selection,
        env,
        options.snapshot,
        || {
            if options.snapshot {
                crate::brand::SoundSink::install(env, options.auto, options.quiet);
            }
        },
        |family| download(family, env, roles, options.quiet, picker),
    )?;
    emit_success(selection.font_name(), env, roles, options, cache);
    dispatch(BrandEvent::ApplyComplete);
    Ok(())
}

/// Injectable installation only: tests still prepare, checkpoint, verify and
/// publish real private files. Readiness runs after successful preflight so even
/// sound initialization cannot precede the recovery boundary.
fn apply_with(
    selection: &ResolvedFontChoice,
    env: &SlateEnv,
    snapshot: bool,
    ready: impl FnOnce(),
    install: impl FnOnce(&str) -> std::result::Result<FontCacheRefresh, String>,
) -> Result<FontCacheRefresh> {
    let family = selection.font_name();
    let commit = FontCommit::prepare(env, family, snapshot)?;
    ready();
    let cache = match selection {
        ResolvedFontChoice::Installed(_) => FontCacheRefresh::NotRequested,
        ResolvedFontChoice::Catalog(_) => install(family).map_err(|reason| {
            SlateError::InvalidConfig(format!(
                "Could not finish font selection '{}': {}. Font configuration application was not attempted; installer font-file or cache changes may remain.",
                super::choices::terminal_text(family),
                super::choices::terminal_text(&reason),
            ))
        })?,
    };
    commit.apply()?;
    Ok(cache)
}

fn download(
    family: &str,
    env: &SlateEnv,
    roles: Option<&Roles<'_>>,
    quiet: bool,
    picker: bool,
) -> std::result::Result<FontCacheRefresh, String> {
    let spinner = (picker && !quiet).then(cliclack::spinner);
    let message = format!("Downloading {}...", super::choices::terminal_text(family));
    if let Some(spinner) = &spinner {
        spinner.start(message);
    } else if !quiet {
        eprintln!("{message}");
    }
    let result = super::download_catalog_font(family, env);
    match &result {
        Ok(cache) => {
            let message = super::format_font_downloaded(roles, family);
            if let Some(spinner) = &spinner {
                spinner.stop(message);
            } else if !quiet {
                eprintln!("{message}");
            }
            if cache.needs_attention() {
                // Keep this on stderr, including in quiet mode and before a
                // configuration commit that may fail independently.
                eprintln!(
                    "Warning: {}",
                    crate::platform::fonts::activation_hint(*cache)
                );
            }
            dispatch(BrandEvent::Success(SuccessKind::FontDownloaded));
        }
        Err(reason) => {
            if let Some(spinner) = &spinner {
                spinner.error(super::format_font_download_failed(
                    roles,
                    &super::choices::terminal_text(reason),
                ));
            }
            dispatch(BrandEvent::Failure(FailureKind::FontDownloadFailed));
        }
    }
    result
}

fn emit_success(
    family: &str,
    env: &SlateEnv,
    roles: Option<&Roles<'_>>,
    options: Options,
    cache: FontCacheRefresh,
) {
    use std::io::IsTerminal;
    if options.quiet {
        return;
    }
    println!("{}", super::format_font_updated(roles, family));
    let report = super::collect_font_apply_report(env);
    if !options.auto && std::io::stdout().is_terminal() && std::io::stderr().is_terminal() {
        println!("{}", super::compact_font_apply_report(&report));
    } else if let Some(text) = super::format_font_apply_report(roles, &report) {
        println!("{text}");
    }
    crate::cli::new_shell_reminder::emit_new_shell_reminder_once(options.auto, options.quiet);
    if super::font_uses_basic_prompt(family) {
        println!("(i) Basic Starship mode enabled for new shells based on the selected family name; glyph coverage is not verified.");
    } else if !cache.needs_attention() {
        println!("{}", crate::platform::fonts::activation_hint(cache));
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
