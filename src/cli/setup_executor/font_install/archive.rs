//! Validate all metadata, then decode only fonts into private, flat staging.
//! No archive-provided path is used as a directory and no links are created.
use super::files::{MAX_DEPTH, MAX_FONTS, MAX_FONT_BYTES, MAX_TOTAL_BYTES};
use crate::error::{Result, SlateError};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::{Duration, Instant},
};
use zip::{
    read::{ArchiveOffset, Config},
    ZipArchive,
};

mod envelope;
#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;

pub(super) const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

fn failure(reason: &str) -> SlateError {
    SlateError::Internal(format!("Font archive {reason}; archive contents omitted"))
}

fn normalized_name(name: &str) -> Result<String> {
    // Conventional explicit archive-root directory; never materialized.
    if name == "./" {
        return Ok(".".to_owned());
    }
    if name.len() > 4096 || name.contains(['\\', ':']) || name.chars().any(char::is_control) {
        return Err(failure("has an unsafe member path"));
    }
    let name = name
        .trim_start_matches("./")
        .strip_suffix('/')
        .unwrap_or(name.trim_start_matches("./"));
    let parts: Vec<_> = name.split('/').collect();
    if parts.len() > MAX_DEPTH
        || parts
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == ".." || part.len() > 255)
    {
        return Err(failure("has an unsafe or excessively deep member path"));
    }
    Ok(name.to_owned())
}

struct Member {
    index: usize,
    basename: String,
    size: u64,
}

pub(super) fn extract(archive: &Path, parent: &Path) -> Result<tempfile::TempDir> {
    let before =
        fs::symlink_metadata(archive).map_err(|_| failure("download produced no readable file"))?;
    if !before.is_file() || before.len() > MAX_ARCHIVE_BYTES {
        return Err(failure(
            "download is not a regular file within the 512 MiB limit",
        ));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(archive)
        .map_err(|_| failure("cannot safely open the downloaded file"))?;
    let opened = file.metadata()?;
    if !opened.is_file()
        || (before.dev(), before.ino(), before.len()) != (opened.dev(), opened.ino(), opened.len())
    {
        return Err(failure("download changed before opening"));
    }
    let started = Instant::now();
    let directory = envelope::inspect(&mut file, opened.len())?;
    let mut zip = ZipArchive::with_config(
        Config {
            archive_offset: ArchiveOffset::Known(0),
        },
        file,
    )
    .map_err(|_| failure("has invalid ZIP metadata"))?;
    if zip.len() != directory.names.len() || zip.central_directory_start() != directory.start {
        return Err(failure("has inconsistent ZIP member metadata"));
    }
    let mut members = Vec::new();
    let mut ranges = Vec::with_capacity(directory.names.len());
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for (index, raw_name) in directory.names.iter().enumerate() {
        let member = zip
            .by_index_raw(index)
            .map_err(|_| failure("has invalid ZIP member headers"))?;
        if member.name() != raw_name || member.enclosed_name().is_none() || member.encrypted() {
            return Err(failure("has ambiguous or unsafe ZIP member metadata"));
        }
        let kind = member.unix_mode().unwrap_or(0) & 0o170000;
        if ![0, 0o100000, 0o040000].contains(&kind)
            || (kind == 0o040000 && !member.is_dir())
            || (kind == 0o100000 && member.is_dir())
        {
            return Err(failure("contains a link or special file"));
        }
        let data_start = member
            .data_start()
            .ok_or_else(|| failure("has an unknown ZIP member range"))?;
        let data_end = data_start
            .checked_add(member.compressed_size())
            .filter(|end| *end <= directory.start)
            .ok_or_else(|| failure("has invalid ZIP member ranges"))?;
        if member.header_start() >= data_start {
            return Err(failure("has invalid ZIP member ranges"));
        }
        ranges.push((member.header_start(), data_end));
        let normalized = normalized_name(member.name())?;
        if member.is_dir() {
            continue;
        }
        let basename = normalized.rsplit('/').next().unwrap();
        if !crate::platform::fonts::supported_font_extension(Path::new(basename)) {
            continue;
        }
        if member.size() == 0 || member.size() > MAX_FONT_BYTES || members.len() >= MAX_FONTS {
            return Err(failure("exceeds font size or count limits"));
        }
        total = total
            .checked_add(member.size())
            .filter(|size| *size <= MAX_TOTAL_BYTES)
            .ok_or_else(|| failure("exceeds the 2 GiB expanded family limit"))?;
        if !names.insert(basename.to_lowercase()) {
            return Err(failure("contains conflicting font basenames"));
        }
        members.push(Member {
            index,
            basename: basename.to_owned(),
            size: member.size(),
        });
    }
    if members.is_empty() {
        return Err(failure("contains no supported font files"));
    }
    // Include local headers, not only compressed data, and use sorting rather
    // than a quadratic pairwise scan at the 10000-entry boundary.
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(failure("contains overlapping ZIP members"));
    }
    // TempDir removes partial extraction on any error, before publication can run.
    let extracted = tempfile::Builder::new()
        .prefix("extract-")
        .tempdir_in(parent)?;
    let mut buffer = [0u8; 64 * 1024];
    for member in members {
        let mut input = zip
            .by_index(member.index)
            .map_err(|_| failure("cannot decode a ZIP member"))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(extracted.path().join(member.basename))?;
        let mut actual = 0u64;
        loop {
            if started.elapsed() > Duration::from_secs(60) {
                return Err(failure("exceeded the extraction time budget"));
            }
            // Read to actual EOF for CRC validation; a take(size) reader would
            // silently skip the decoder's final CRC and extra-output check.
            let count = input
                .read(&mut buffer)
                .map_err(|_| failure("has corrupt or truncated font data"))?;
            if count == 0 {
                break;
            }
            actual = actual
                .checked_add(count as u64)
                .filter(|size| *size <= member.size)
                .ok_or_else(|| failure("expanded beyond the declared font size"))?;
            output.write_all(&buffer[..count])?;
        }
        if actual != member.size {
            return Err(failure("has an incorrect expanded font size"));
        }
    }
    Ok(extracted)
}
