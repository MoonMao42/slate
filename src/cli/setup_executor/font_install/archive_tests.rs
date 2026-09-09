use super::*;
use std::{
    fs::File,
    io::Cursor,
    os::unix::{
        ffi::OsStringExt,
        fs::{symlink, PermissionsExt},
    },
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const FONT: &[u8] = b"\0\x01\0\0private-fixture-not-a-real-font";

fn fixture(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(method);
    for (name, bytes) in entries {
        if name.ends_with('/') {
            writer.add_directory(*name, options).unwrap();
        } else {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

fn offsets(bytes: &[u8], signature: &[u8]) -> Vec<usize> {
    bytes
        .windows(signature.len())
        .enumerate()
        .filter_map(|(index, part)| (part == signature).then_some(index))
        .collect()
}

fn zip64(mut bytes: Vec<u8>) -> Vec<u8> {
    let end = offsets(&bytes, b"PK\x05\x06")[0];
    let old = bytes.split_off(end);
    let count = u16::from_le_bytes(old[10..12].try_into().unwrap()) as u64;
    let size = u32::from_le_bytes(old[12..16].try_into().unwrap()) as u64;
    let start = u32::from_le_bytes(old[16..20].try_into().unwrap()) as u64;
    bytes.extend_from_slice(b"PK\x06\x06");
    bytes.extend_from_slice(&44u64.to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    for value in [count, count, size, start] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(b"PK\x06\x07");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(end as u64).to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    let mut footer = old;
    footer[8..20].fill(255);
    bytes.extend_from_slice(&footer);
    bytes
}

fn reject(bytes: &[u8], reason: &str) {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("release.zip");
    fs::write(&archive, bytes).unwrap();
    let error = extract(&archive, temp.path()).unwrap_err().to_string();
    assert!(error.contains(reason), "expected {reason:?}, got {error}");
    assert_eq!(
        fs::read_dir(temp.path()).unwrap().count(),
        1,
        "partial extraction leaked"
    );
}

#[test]
fn font_install_archive_stored_deflate_and_zip64_stage_only_flat_private_fonts() {
    for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let raw = fixture(
            &[
                ("./", b""),
                ("nested/", b""),
                ("nested/A.ttf", FONT),
                ("./B.OTF", b"OTTOprivate-font-fixture"),
                ("字.ttc", b"ttcfprivate-font-collection"),
                ("D.OTC", b"ttcfprivate-opentype-collection"),
                ("LICENSE", b"untouched"),
            ],
            method,
        );
        for bytes in [raw.clone(), zip64(raw)] {
            let temp = tempfile::tempdir().unwrap();
            let archive = temp.path().join("release.zip");
            fs::write(&archive, &bytes).unwrap();
            let extracted = extract(&archive, temp.path()).unwrap();
            assert_eq!(fs::read_dir(extracted.path()).unwrap().count(), 4);
            assert_eq!(fs::read(extracted.path().join("A.ttf")).unwrap(), FONT);
            assert_eq!(
                fs::metadata(extracted.path().join("B.OTF"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert!(!extracted.path().join("nested").exists());
            assert!(!extracted.path().join("LICENSE").exists());
            assert_eq!(fs::read(&archive).unwrap(), bytes);
            let path = extracted.path().to_owned();
            drop(extracted);
            assert!(!path.exists());
        }
    }
}

#[test]
fn font_install_archive_rejects_unsafe_paths_even_in_ignored_members() {
    for name in [
        "../escape.txt",
        "/absolute.txt",
        "C:/drive.txt",
        "a\\b.txt",
        "a/../b.txt",
        "a//b.txt",
        "a/./b.txt",
        "a\u{1b}[31m.txt",
        "a\0b.txt",
    ] {
        reject(
            &fixture(
                &[("Good.ttf", FONT), (name, b"PRIVATE")],
                CompressionMethod::Stored,
            ),
            "unsafe",
        );
    }
    let deep = format!("{}bad.ttf", "level/".repeat(MAX_DEPTH));
    reject(
        &fixture(&[(&deep, FONT)], CompressionMethod::Stored),
        "deep",
    );
    let long = format!("{}.ttf", "a".repeat(256));
    reject(
        &fixture(&[(&long, FONT)], CompressionMethod::Stored),
        "unsafe",
    );
    let mut invalid_utf8 = fixture(&[("A.ttf", FONT)], CompressionMethod::Stored);
    let central = offsets(&invalid_utf8, b"PK\x01\x02")[0];
    invalid_utf8[central + 46] = 255;
    reject(&invalid_utf8, "non-UTF-8");
}

#[test]
fn font_install_archive_rejects_links_special_files_and_basename_collisions() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("Good.ttf", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(FONT).unwrap();
    writer
        .add_symlink("ignored.txt", "../secret", SimpleFileOptions::default())
        .unwrap();
    reject(&writer.finish().unwrap().into_inner(), "special file");
    for mode in [0o010644u32, 0o020644, 0o060644, 0o140644] {
        let mut bytes = fixture(&[("A.ttf", FONT)], CompressionMethod::Stored);
        let central = offsets(&bytes, b"PK\x01\x02")[0];
        bytes[central + 5] = 3; // UNIX host
        bytes[central + 38..central + 42].copy_from_slice(&(mode << 16).to_le_bytes());
        reject(&bytes, "special file");
    }
    reject(
        &fixture(
            &[("a/A.ttf", FONT), ("b/a.TTF", FONT)],
            CompressionMethod::Stored,
        ),
        "basenames",
    );
    reject(
        &fixture(
            &[("A.ttf", FONT), ("./A.ttf", FONT)],
            CompressionMethod::Stored,
        ),
        "duplicate",
    );
    let mut bytes = fixture(
        &[("A.ttf", FONT), ("B.ttf", FONT)],
        CompressionMethod::Stored,
    );
    let second = offsets(&bytes, b"PK\x01\x02")[1];
    bytes[second + 46] = b'A'; // Duplicate raw names must not be silently deduped.
    reject(&bytes, "duplicate");
}

#[test]
fn font_install_archive_rejects_encryption_codecs_and_invalid_ranges() {
    for flags in [1u16, 1 << 6, 1 << 13] {
        let mut bytes = fixture(&[("A.ttf", FONT)], CompressionMethod::Stored);
        let central = offsets(&bytes, b"PK\x01\x02")[0];
        bytes[central + 8..central + 10].copy_from_slice(&flags.to_le_bytes());
        reject(&bytes, "unsupported encryption");
    }
    let raw = fixture(&[("A.ttf", FONT)], CompressionMethod::Stored);
    let central = offsets(&raw, b"PK\x01\x02")[0];
    let mut bytes = raw.clone();
    bytes[central + 10..central + 12].copy_from_slice(&12u16.to_le_bytes());
    reject(&bytes, "unsupported encryption, compression");
    let mut bytes = raw.clone();
    bytes[central + 20..central + 24].copy_from_slice(&u32::MAX.to_le_bytes());
    reject(&bytes, "invalid ZIP member ranges");
    let mut bytes = fixture(
        &[("A.ttf", FONT), ("B.ttf", FONT)],
        CompressionMethod::Stored,
    );
    let second = offsets(&bytes, b"PK\x01\x02")[1];
    bytes[second + 42..second + 46].copy_from_slice(&0u32.to_le_bytes());
    reject(&bytes, "overlapping");
}

#[test]
fn font_install_archive_crc_truncation_and_false_expanded_sizes_leave_no_staging() {
    for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let raw = fixture(&[("A.ttf", FONT), ("B.ttf", FONT)], method);
        let central = offsets(&raw, b"PK\x01\x02")[1];
        let mut bytes = raw.clone();
        bytes[central + 16] ^= 1; // Bad CRC only in the second member.
        reject(&bytes, "corrupt");
        for size in [1, FONT.len() as u32 + 1] {
            let mut bytes = raw.clone();
            bytes[central + 24..central + 28].copy_from_slice(&size.to_le_bytes());
            reject(
                &bytes,
                if size == 1 {
                    "declared font size"
                } else {
                    "incorrect expanded"
                },
            );
        }
        let mut bytes = raw.clone();
        let size = (MAX_FONT_BYTES + 1) as u32;
        bytes[central + 24..central + 28].copy_from_slice(&size.to_le_bytes());
        reject(&bytes, "font size");
        reject(&raw[..raw.len() - 4], "end record");
    }
    reject(b"not zip PRIVATE_SECRET", "not a supported ZIP");
    reject(
        &fixture(&[("LICENSE", b"legal")], CompressionMethod::Stored),
        "no supported font",
    );
}

#[test]
fn font_install_archive_directory_guards_reject_forged_counts_before_library_allocation() {
    let raw = fixture(&[("A.ttf", FONT)], CompressionMethod::Stored);
    let mut bytes = zip64(raw.clone());
    let end64 = offsets(&bytes, b"PK\x06\x06")[0];
    bytes[end64 + 24..end64 + 32].copy_from_slice(&u64::MAX.to_le_bytes());
    bytes[end64 + 32..end64 + 40].copy_from_slice(&u64::MAX.to_le_bytes());
    reject(&bytes, "directory limits");
    let mut bytes = raw.clone();
    let end = offsets(&bytes, b"PK\x05\x06")[0];
    bytes[end + 8..end + 12].fill(255);
    reject(&bytes, "incomplete ZIP");
    let mut bytes = raw.clone();
    bytes[end + 12..end + 16].copy_from_slice(&(9u32 * 1024 * 1024).to_le_bytes());
    reject(&bytes, "directory limits");
    let mut bytes = raw.clone();
    bytes[end + 8..end + 10].copy_from_slice(&2u16.to_le_bytes());
    reject(&bytes, "incomplete ZIP");
    let mut bytes = raw.clone();
    bytes[end + 4] = 1;
    reject(&bytes, "split ZIP");
    let mut bytes = raw;
    bytes[end + 20..end + 22].copy_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(b"PK\x05\x06");
    reject(&bytes, "end record");
}

#[test]
fn font_install_archive_count_and_family_limits_need_no_large_payloads() {
    let names: Vec<_> = (0..=MAX_FONTS)
        .map(|index| format!("{index}.ttf"))
        .collect();
    let entries: Vec<_> = names.iter().map(|name| (name.as_str(), FONT)).collect();
    reject(
        &fixture(&entries, CompressionMethod::Stored),
        "count limits",
    );
    let mut bytes = fixture(&entries[..33], CompressionMethod::Stored);
    for central in offsets(&bytes, b"PK\x01\x02") {
        bytes[central + 24..central + 28].copy_from_slice(&(MAX_FONT_BYTES as u32).to_le_bytes());
    }
    reject(&bytes, "2 GiB");
}

#[test]
fn font_install_archive_input_must_be_bounded_regular_file_and_paths_keep_os_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let raw = fixture(&[("A.ttf", FONT)], CompressionMethod::Stored);
    // APFS rejects invalid UTF-8 filenames; Linux also exercises raw path bytes.
    let native_name = if cfg!(target_os = "linux") {
        b"release-\xff.zip".as_slice()
    } else {
        "release-字 space.zip".as_bytes()
    };
    let archive = temp
        .path()
        .join(std::ffi::OsString::from_vec(native_name.to_vec()));
    fs::write(&archive, raw).unwrap();
    assert!(extract(&archive, temp.path()).is_ok());
    let linked = temp.path().join("linked.zip");
    symlink(&archive, &linked).unwrap();
    assert!(extract(&linked, temp.path())
        .unwrap_err()
        .to_string()
        .contains("regular file"));
    let large = temp.path().join("sparse.zip");
    File::create(&large)
        .unwrap()
        .set_len(MAX_ARCHIVE_BYTES + 1)
        .unwrap();
    assert!(extract(&large, temp.path())
        .unwrap_err()
        .to_string()
        .contains("512 MiB"));
    assert!(extract(temp.path(), temp.path()).is_err());
}
