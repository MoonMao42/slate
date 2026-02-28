use crate::config::ConfigManager;
use std::process::Command;

const SOUND_FILE: &str = "/System/Library/Sounds/Pop.aiff";

/// Play a subtle feedback sound if sound is enabled in config.
/// Runs asynchronously (non-blocking) so it doesn't slow down the CLI.
pub fn play_feedback() {
    let config = match ConfigManager::new() {
        Ok(c) => c,
        Err(_) => return,
    };

    if !config.is_sound_enabled().unwrap_or(false) {
        return;
    }

    // Fire and forget — don't block the CLI
    let _ = Command::new("afplay")
        .arg(SOUND_FILE)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
