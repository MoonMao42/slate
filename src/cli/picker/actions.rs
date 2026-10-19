use crate::cli::ui_language::tr;
use crate::config::pairing::{PreparedPairing, SlotEdit};
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::ThemeAppearance;

use super::state::PickerState;

pub(super) fn quick_save_auto(state: &PickerState, env: &SlateEnv) -> Result<String> {
    let theme = state.get_current_theme()?;
    let theme_id = state.get_current_theme_id();

    let message = match theme.appearance {
        ThemeAppearance::Dark => {
            PreparedPairing::capture_edits(env, SlotEdit::Set(theme_id), SlotEdit::Keep)?
                .save_in_preview()?;
            format!(
                "✓ {}{}",
                tr("深色配对已保存：", "Auto Dark saved: "),
                theme.name
            )
        }
        ThemeAppearance::Light => {
            PreparedPairing::capture_edits(env, SlotEdit::Keep, SlotEdit::Set(theme_id))?
                .save_in_preview()?;
            format!(
                "✓ {}{}",
                tr("浅色配对已保存：", "Auto Light saved: "),
                theme.name
            )
        }
    };

    Ok(message)
}
