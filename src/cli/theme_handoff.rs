//! Shared consent boundary for entering the global picker from a narrower page.
use super::ui_language::tr;
use crate::{
    config::{
        file_read::{self, Links, MAX_STATE_BYTES},
        recovery_paths,
    },
    env::SlateEnv,
    error::{Result, SlateError},
};

pub(super) enum Origin {
    PromptLayout,
    Tool,
}

pub(super) fn open(env: &SlateEnv, origin: Origin) -> Result<()> {
    let scope = match origin {
        Origin::PromptLayout => tr(
            "本次不保存刚才选择的提示符样式。",
            "This does not save the prompt style you just selected.",
        ),
        Origin::Tool => tr(
            "本次不安装工具，也不运行完整设置。",
            "This does not install tools or run full setup.",
        ),
    };
    super::file_output::write_required(&format!(
        "\n  {}\n  {}\n  {scope}\n\n",
        tr(
            "预览会临时修改检测到的工具配置，不只当前页面对应的工具。",
            "Preview temporarily changes detected tools, not just this page's tool."
        ),
        tr(
            "进入后：Enter 保存主题和透明度；Esc 恢复预览前的状态。",
            "In the picker: Enter saves theme and opacity; Esc restores the preview."
        ),
    ))?;
    let confirmed = super::menu::select(tr("要进入全局主题预览吗？", "Open Global Theme Preview?"))
        .escape_value(false)
        .initial_value(false)
        .item(false, tr("暂不进入", "Cancel"), "")
        .item(true, tr("进入预览", "Open Preview"), "")
        .interact()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::Interrupted {
                SlateError::UserCancelled
            } else {
                error.into()
            }
        })?;
    if !confirmed {
        return Ok(());
    }
    // An unreadable record is not an absent theme. Check afresh after consent,
    // without initializing ConfigManager or probing unrelated tool inventory.
    let path = env.managed_file("current");
    let readable = || -> std::result::Result<(), ()> {
        recovery_paths::validate_file_path(env, &path, "theme state").map_err(|_| ())?;
        if let Some(source) =
            file_read::read(&path, MAX_STATE_BYTES, Links::Reject).map_err(|_| ())?
        {
            std::str::from_utf8(&source.bytes).map_err(|_| ())?;
        }
        Ok(())
    };
    readable().map_err(|()| SlateError::InvalidConfig("Saved theme state changed or is unreadable; no preview was opened. Inspect it before retrying.".into()))?;
    // The picker retains its own writer/recovery guards. It returns on save or
    // cancel; the originating page refreshes itself without an implicit apply.
    super::picker::launch_picker(env)
}
