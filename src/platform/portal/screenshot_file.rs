//! A URI returned by Portal grants access, not ownership of its source file.
use crate::{
    error::{Result, SlateError},
    platform::share::image_file::CapturedImage,
};
use std::path::Path;

pub(super) fn copy_result(uri: &str, output: &Path) -> Result<()> {
    let invalid = || {
        SlateError::PlatformError(
            "Portal screenshot returned an invalid local file URI; response details omitted".into(),
        )
    };
    let url = url::Url::parse(uri).map_err(|_| invalid())?;
    if url.scheme() != "file" || url.query().is_some() || url.fragment().is_some() {
        return Err(invalid());
    }
    let source = url.to_file_path().map_err(|_| invalid())?;
    let image = CapturedImage::read(&source)?.ok_or_else(|| {
        SlateError::PlatformError(
            "Portal screenshot source is missing; response details omitted".into(),
        )
    })?;
    // Never unlink the borrowed URI or write through an existing destination,
    // including when source and destination refer to the same inode.
    image.save_new(output)
}

#[cfg(test)]
#[path = "screenshot_file_tests.rs"]
mod tests;
