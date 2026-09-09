//! Bound the ZIP directory BEFORE ZipArchive allocates from its entry count.
//! This is an outer-record guard, not another decompressor: ZIP/ZIP64 members,
//! descriptors, Deflate and CRC are still read by the zip crate.
use super::{failure, normalized_name};
use crate::error::Result;
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
};

const MAX_DIRECTORY: u64 = 8 * 1024 * 1024;
const END: &[u8] = b"PK\x05\x06";
const LOCATOR: &[u8] = b"PK\x06\x07";
const END64: &[u8] = b"PK\x06\x06";

pub(super) struct Directory {
    pub start: u64,
    pub names: Vec<String>,
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn read_at(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| failure("cannot seek ZIP metadata"))?;
    let mut bytes = vec![0; len];
    file.read_exact(&mut bytes)
        .map_err(|_| failure("has truncated ZIP metadata"))?;
    Ok(bytes)
}

pub(super) fn inspect(file: &mut File, length: u64) -> Result<Directory> {
    let tail_len = length.min(65535 + 22) as usize;
    let tail_start = length - tail_len as u64;
    let tail = read_at(file, tail_start, tail_len)?;
    // Last signature, not last plausible signature: reject ambiguous comments
    // rather than letting the library select a different end record.
    let end = tail
        .windows(4)
        .rposition(|bytes| bytes == END)
        .ok_or_else(|| failure("is not a supported ZIP archive"))?;
    let record = &tail[end..];
    if record.len() < 22 || record.len() != 22 + u16_at(record, 20) as usize {
        return Err(failure("has an invalid ZIP end record"));
    }
    if u16_at(record, 4) != 0 || u16_at(record, 6) != 0 {
        return Err(failure("uses unsupported split ZIP volumes"));
    }
    let end_offset = tail_start + end as u64;
    let mut entries = u16_at(record, 10) as u64;
    let mut size = u32_at(record, 12) as u64;
    let mut start = u32_at(record, 16) as u64;
    let mut footer = end_offset;
    let needs64 = entries == u16::MAX as u64
        || u16_at(record, 8) == u16::MAX
        || size == u32::MAX as u64
        || start == u32::MAX as u64;
    let locator = if end_offset >= 20 {
        read_at(file, end_offset - 20, 20)?
    } else {
        Vec::new()
    };
    if locator.starts_with(LOCATOR) {
        if u32_at(&locator, 4) != 0 || u32_at(&locator, 16) != 1 {
            return Err(failure("uses unsupported ZIP64 volumes"));
        }
        footer = u64_at(&locator, 8);
        if footer
            .checked_add(56)
            .is_none_or(|end| end > end_offset - 20)
        {
            return Err(failure("has invalid ZIP64 offsets"));
        }
        let record64 = read_at(file, footer, 56)?;
        let remainder = u64_at(&record64, 4);
        if !record64.starts_with(END64)
            || !(44..=44 + 4096).contains(&remainder)
            || footer.checked_add(12 + remainder) != Some(end_offset - 20)
            || u32_at(&record64, 16) != 0
            || u32_at(&record64, 20) != 0
            || u64_at(&record64, 24) != u64_at(&record64, 32)
        {
            return Err(failure("has an invalid ZIP64 end record"));
        }
        let expanded = [
            u64_at(&record64, 32),
            u64_at(&record64, 40),
            u64_at(&record64, 48),
        ];
        for (small, large, sentinel) in [
            (entries, expanded[0], u16::MAX as u64),
            (u16_at(record, 8) as u64, expanded[0], u16::MAX as u64),
            (size, expanded[1], u32::MAX as u64),
            (start, expanded[2], u32::MAX as u64),
        ] {
            if small != sentinel && small != large {
                return Err(failure("has conflicting ZIP64 metadata"));
            }
        }
        [entries, size, start] = expanded;
    } else if needs64 || u16_at(record, 8) as u64 != entries {
        return Err(failure("has incomplete ZIP directory metadata"));
    }
    if entries == 0
        || entries > super::super::files::MAX_ENTRIES as u64
        || size > MAX_DIRECTORY
        || start.checked_add(size) != Some(footer)
    {
        return Err(failure(
            "exceeds ZIP directory limits or has invalid offsets",
        ));
    }
    if read_at(file, 0, 4)? != b"PK\x03\x04" {
        return Err(failure("uses an unsupported prefixed ZIP archive"));
    }
    let directory = read_at(file, start, size as usize)?;
    let mut position = 0usize;
    let mut names = Vec::with_capacity(entries as usize);
    let mut seen = BTreeSet::new();
    for _ in 0..entries {
        let header = directory
            .get(position..position.saturating_add(46))
            .filter(|header| header.starts_with(b"PK\x01\x02"))
            .ok_or_else(|| failure("has an invalid ZIP directory entry"))?;
        let name_len = u16_at(header, 28) as usize;
        let entry_len = 46 + name_len + u16_at(header, 30) as usize + u16_at(header, 32) as usize;
        let name = directory
            .get(position + 46..position + 46 + name_len)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .ok_or_else(|| failure("has a non-UTF-8 ZIP member name"))?;
        let normalized = normalized_name(name)?;
        if !seen.insert(normalized.to_lowercase()) {
            return Err(failure("has duplicate ZIP member paths"));
        }
        // The library's map otherwise silently collapses repeated raw names.
        names.push(name.to_owned());
        if u16_at(header, 8) & ((1 << 0) | (1 << 6) | (1 << 13)) != 0
            || ![0, 8].contains(&u16_at(header, 10))
            || u16_at(header, 34) != 0
        {
            return Err(failure(
                "uses unsupported encryption, compression or volumes",
            ));
        }
        position = position
            .checked_add(entry_len)
            .filter(|end| *end <= directory.len())
            .ok_or_else(|| failure("has truncated ZIP directory entries"))?;
    }
    if position != directory.len() {
        return Err(failure("has unexpected ZIP directory records"));
    }
    Ok(Directory { start, names })
}
