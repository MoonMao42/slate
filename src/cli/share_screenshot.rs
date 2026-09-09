use crate::env::SlateEnv;
use crate::error::Result;
use crate::platform::share::{capture_draft, CaptureDraft, ShareCaptureResult};
use std::path::Path;
use std::path::PathBuf;

mod watermark;

/// Handle `slate share` — screenshot current terminal + export code.
/// 1. Print the export URI
/// 2. Capture the terminal window via the platform share backend
/// 3. Add watermark if ImageMagick is available
/// 4. Save the image path for sharing
pub fn handle_share() -> Result<()> {
    let env = SlateEnv::from_process()?;
    // Generate export URI
    let uri = crate::cli::share::build_export_uri(&env)?;

    // Print URI first so it's visible in the screenshot
    println!("{}", share_intro_text(&uri));

    let mut image = match capture_draft()? {
        CaptureDraft::Captured(image) => image,
        CaptureDraft::Unavailable(result) => {
            if let Some(message) = capture_fallback_text(&result) {
                println!("{}", message);
            }
            return Ok(());
        }
    };

    match watermark::try_watermark(&image, &uri) {
        Ok(Some(watermarked)) => image = watermarked,
        Ok(None) => {}
        Err(_) => {
            eprintln!("warning: watermark could not be applied; keeping the original capture")
        }
    }
    // Resolve the output directory after interaction/processing, not before a
    // potentially long user dialog where a directory alias may have changed.
    let output_path = image.save_unique(&output_path(&env))?;
    println!("{}", share_saved_text(&output_path));

    Ok(())
}

fn output_path(env: &SlateEnv) -> PathBuf {
    output_path_with_pictures(env, std::env::var_os("XDG_PICTURES_DIR").map(PathBuf::from))
}

fn output_path_with_pictures(env: &SlateEnv, pictures: Option<PathBuf>) -> PathBuf {
    // Resolve directory aliases before the confinement check. Lexical starts_with
    // alone accepts home/../outside and links escaping the injected home.
    let home = std::fs::canonicalize(env.home()).unwrap_or_else(|_| env.home().to_owned());
    for candidate in pictures.into_iter().chain([env.home().join("Desktop")]) {
        if !candidate.is_absolute()
            || candidate
                .components()
                .any(|part| part == std::path::Component::ParentDir)
        {
            continue;
        }
        if let Ok(directory) = std::fs::canonicalize(candidate) {
            if directory.is_dir() && directory.starts_with(&home) {
                return directory.join("slate-share.png");
            }
        }
    }
    home.join("slate-share.png")
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

fn watermark_text(uri: &str) -> String {
    // ImageMagick interprets percent properties in -annotate text; share-code
    // escapes must remain literal rather than expanding into image metadata.
    format!("✦ slate  ·  {}", uri.replace('%', "%%"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_image_output_directory_rejects_parent_traversal_and_escaping_aliases() {
        use std::{fs, os::unix::fs::symlink};
        let td = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(td.path()).unwrap();
        let home = root.join("home");
        let outside = root.join("outside");
        fs::create_dir_all(home.join("Pictures")).unwrap();
        fs::create_dir(&outside).unwrap();
        symlink(&outside, home.join("Desktop")).unwrap();
        symlink(&outside, home.join("escape")).unwrap();
        symlink(home.join("Pictures"), home.join("inside")).unwrap();
        let env = SlateEnv::with_home(home.clone());
        for path in [
            home.join("../outside"),
            home.join("escape"),
            outside,
            home.join("missing"),
            PathBuf::from("relative"),
        ] {
            assert_eq!(
                output_path_with_pictures(&env, Some(path)),
                home.join("slate-share.png")
            );
        }
        assert_eq!(
            output_path_with_pictures(&env, Some(home.join("inside"))),
            home.join("Pictures/slate-share.png")
        );
    }

    #[test]
    fn share_watermark_preserves_uri_percent_escapes_as_literal_text() {
        assert_eq!(
            watermark_text("slate://v1/nord/Mono%20%E5%AD%97/solid/s"),
            "✦ slate  ·  slate://v1/nord/Mono%%20%%E5%%AD%%97/solid/s"
        );
    }

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
