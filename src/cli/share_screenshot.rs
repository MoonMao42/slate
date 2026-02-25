use crate::config::ConfigManager;
use crate::env::SlateEnv;
use crate::error::Result;
use std::path::PathBuf;
use std::process::Command;

/// Handle `slate share` — screenshot current terminal + export code.
/// 1. Capture the terminal window via macOS screencapture
/// 2. Add watermark if ImageMagick is available
/// 3. Print the export URI for sharing
pub fn handle_share() -> Result<()> {
    let env = SlateEnv::from_process()?;
    let config = ConfigManager::with_env(&env)?;

    // Generate export URI
    let uri = build_export_uri(&config)?;

    // Determine output path
    let output_path = output_path();

    println!();
    println!("  Click your terminal window to capture it.");
    println!();

    // Capture window screenshot (blocks until user clicks)
    let capture_result = Command::new("screencapture")
        .args(["-w", "-o", output_path.to_str().unwrap()])
        .status();

    match capture_result {
        Ok(status) if status.success() => {}
        _ => {
            eprintln!("  Screenshot cancelled or failed.");
            return Ok(());
        }
    }

    // Try to add watermark with ImageMagick
    if has_imagemagick() {
        let _ = add_watermark(&output_path, &uri);
    }

    println!("  ✓ Saved to {}", output_path.display());
    println!();
    println!("  Share code:");
    println!("  {}", uri);
    println!();

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

    Ok(format!("slate://{}/{}/{}/{}", theme, font, opacity, tools_str))
}

fn output_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Desktop/slate-share.png")
}

fn has_imagemagick() -> bool {
    Command::new("magick")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn add_watermark(image_path: &PathBuf, uri: &str) -> std::result::Result<(), ()> {
    // Add "✦ slate" watermark + URI at bottom-right
    let watermark_text = format!("✦ slate  ·  {}", uri);

    let status = Command::new("magick")
        .args([
            image_path.to_str().unwrap(),
            "-gravity",
            "SouthEast",
            "-pointsize",
            "14",
            "-fill",
            "rgba(255,255,255,0.5)",
            "-annotate",
            "+20+12",
            &watermark_text,
            image_path.to_str().unwrap(),
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
