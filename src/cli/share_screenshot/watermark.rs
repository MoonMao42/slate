use crate::{
    error::{Result, SlateError},
    platform::{
        process_output::{self, Completion, Limits},
        share::image_file::CapturedImage,
    },
};
use std::{ffi::OsString, path::Path, process::Command, time::Duration};

const LIMITS: Limits = Limits {
    timeout: Duration::from_secs(10),
    max_output: 64 * 1024,
};

pub(super) fn try_watermark(image: &CapturedImage, uri: &str) -> Result<Option<CapturedImage>> {
    let Some(binary) = crate::detection::command_in_actual_path("magick")
        .or_else(|| crate::detection::command_path("magick"))
    else {
        return Ok(None);
    };
    run(&binary, image, uri, LIMITS).map(Some)
}

fn png_path(path: &Path) -> OsString {
    let mut value = OsString::from("png:");
    value.push(path);
    value
}

fn run(binary: &Path, image: &CapturedImage, uri: &str, limits: Limits) -> Result<CapturedImage> {
    let failure = |reason: &str| {
        SlateError::PlatformError(format!(
            "Screenshot watermark {reason}; native output omitted"
        ))
    };
    let scratch = tempfile::Builder::new()
        .prefix("slate-watermark-")
        .tempdir()?;
    let input = scratch.path().join("input.png");
    let output = scratch.path().join("output.png");
    image.save_new(&input)?;
    let result = process_output::capture(
        Command::new(binary)
            .arg(png_path(&input))
            .args([
                "-gravity",
                "SouthEast",
                "-pointsize",
                "14",
                "-fill",
                "rgba(255,255,255,0.5)",
                "-annotate",
                "+20+12",
            ])
            .arg(super::watermark_text(uri))
            .arg(png_path(&output)),
        limits,
    )
    .map_err(|_| failure("could not complete"))?;
    match result.completion {
        Completion::Exited(status) if status.success() => {}
        Completion::Exited(_) => return Err(failure("command failed")),
        Completion::TimedOut => return Err(failure("timed out")),
        Completion::OutputLimit => return Err(failure("exceeded its output limit")),
    }
    CapturedImage::read(&output)?.ok_or_else(|| failure("produced no image"))
}

#[cfg(test)]
#[path = "watermark_tests.rs"]
mod tests;
