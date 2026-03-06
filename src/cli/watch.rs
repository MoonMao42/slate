use crate::error::Result;

pub fn handle_auto_theme_watch() -> Result<()> {
    crate::platform::dark_mode_notify::run_watcher_loop()
}
