use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::platform::share::{capture_interactive, ShareCaptureResult};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

/// Handle `slate share` — screenshot current terminal + export code.
/// 1. Print the export URI
/// 2. Capture the terminal window via the platform share backend
/// 3. Add watermark if ImageMagick is available
/// 4. Save the image path for sharing
pub fn handle_share() -> Result<()> {
    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;

    // Generate export URI
    let uri = build_export_uri(&config)?;

    // Determine output path
    let output_path = output_path(&env);

    // Print URI first so it's visible in the screenshot
    println!("{}", share_intro_text(&uri));

    let capture_result = capture_interactive(&output_path)?;
    if !capture_result.captured {
        if let Some(message) = capture_fallback_text(&capture_result) {
            println!("{}", message);
        }
        return Ok(());
    }

    // Try to add watermark with ImageMagick
    if has_imagemagick() {
        let _ = add_watermark(&output_path, &uri);
    }

    println!("{}", share_saved_text(&output_path));

    Ok(())
}

fn build_export_uri(config: &ConfigManager) -> Result<String> {
    let theme = config
        .get_current_theme()?
        .unwrap_or_else(|| "none".to_string());

    let font = config
        .get_current_font()?
        .unwrap_or_else(|| "none".to_string())
        .replace(' ', "-");

    let opacity = config
        .get_current_opacity()?
        .unwrap_or_else(|| "solid".to_string())
        .to_lowercase();

    let mut tools = Vec::new();
    if config.is_starship_enabled()? {
        tools.push("s");
    }
    if config.is_zsh_highlighting_enabled()? {
        tools.push("h");
    }
    if config.has_fastfetch_autorun()? {
        tools.push("f");
    }
    let tools_str = if tools.is_empty() {
        "none".to_string()
    } else {
        tools.join(",")
    };

    Ok(format!(
        "slate://{}/{}/{}/{}",
        theme, font, opacity, tools_str
    ))
}

fn output_path(env: &SlateEnv) -> PathBuf {
    // Prefer $XDG_PICTURES_DIR (Linux user-dirs), fall back to ~/Desktop if present,
    // otherwise drop the file at the home root so the user can still find it.
    if let Ok(pictures) = std::env::var("XDG_PICTURES_DIR") {
        if !pictures.is_empty() {
            return PathBuf::from(pictures).join("slate-share.png");
        }
    }

    let desktop = env.home().join("Desktop");
    if desktop.is_dir() {
        return desktop.join("slate-share.png");
    }

    env.home().join("slate-share.png")
}

fn share_intro_text(uri: &str) -> String {
    format!("\n  {}\n\n  Click your terminal window to capture it.", uri)
}

fn capture_fallback_text(capture_result: &ShareCaptureResult) -> Option<String> {
    capture_result
        .reason
        .as_ref()
        .map(|reason| format!("  {}", reason))
}

fn share_saved_text(output_path: &Path) -> String {
    format!("\n  ✓ Saved to {}\n", output_path.display())
}

fn has_imagemagick() -> bool {
    Command::new("magick")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn add_watermark(image_path: &Path, uri: &str) -> std::result::Result<(), ()> {
    // Add "✦ slate" watermark + URI at bottom-right.
    // Skip silently if the path isn't valid UTF-8 — the watermark is optional polish, not
    // correctness-critical, and magick won't accept non-UTF-8 args anyway.
    let path_str = image_path.to_str().ok_or(())?;
    let watermark_text = format!("✦ slate  ·  {}", uri);

    let status = Command::new("magick")
        .args([
            path_str,
            "-gravity",
            "SouthEast",
            "-pointsize",
            "14",
            "-fill",
            "rgba(255,255,255,0.5)",
            "-annotate",
            "+20+12",
            &watermark_text,
            path_str,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| ())?;

    if status.success() {
        Ok(())
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_intro_text_keeps_uri_visible_before_capture() {
        let intro = share_intro_text("slate://catppuccin-mocha/JetBrainsMono/solid/s,h");

        assert!(intro.contains("slate://catppuccin-mocha/JetBrainsMono/solid/s,h"));
        assert!(intro.contains("Click your terminal window to capture it."));
    }

    #[test]
    fn test_capture_fallback_text_returns_backend_reason() {
        let message = capture_fallback_text(&ShareCaptureResult {
            captured: false,
            reason: Some(
                "No supported screenshot backend was found. Share URI export is still available."
                    .to_string(),
            ),
        })
        .expect("fallback message should be rendered");

        assert!(message.contains("Share URI export is still available"));
    }

    #[test]
    fn test_share_saved_text_includes_output_path() {
        let message = share_saved_text(Path::new("/tmp/slate-share.png"));
        assert!(message.contains("/tmp/slate-share.png"));
        assert!(message.contains("Saved"));
    }
}
