use crate::config::ConfigManager;

#[cfg(target_os = "macos")]
const SOUND_FILE: &str = "/System/Library/Sounds/Pop.aiff";

/// Play a subtle feedback sound if sound is enabled in config.
/// Implemented on macOS via `afplay`. On other platforms this is a no-op — the sound
/// setting is still readable for forward compatibility, but nothing plays. Returns
/// `true` if a sound was dispatched, `false` otherwise. Callers generally ignore the
/// return value; it exists mainly for tests.
pub fn play_feedback() -> bool {
    let config = match ConfigManager::new() {
        Ok(c) => c,
        Err(_) => return false,
    };

    if !config.is_sound_enabled().unwrap_or(false) {
        return false;
    }

    play_platform_sound()
}

#[cfg(target_os = "macos")]
fn play_platform_sound() -> bool {
    // Fire and forget — don't block the CLI.
    std::process::Command::new("afplay")
        .arg(SOUND_FILE)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

#[cfg(not(target_os = "macos"))]
fn play_platform_sound() -> bool {
    // No cross-platform audio backend wired up yet; sound preference is advisory on Linux.
    false
}
