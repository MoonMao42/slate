//! Inventory of direct production event dispatches. Review changes alongside
//! event behavior tests: shared font handling removes duplicate sites, and
//! setup completion/failure uses injected callbacks tested in setup outcomes.

use std::fs;
use std::path::{Path, PathBuf};

const EXPECTED_DISPATCH_COUNT: usize = 35;

/// Recursively walk `dir`, returning every `.rs` file under it. Skips
/// directories whose name starts with `.` (e.g. `.git`, `.cargo`).
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() {
            collect_rs_files(&path, out)?;
        } else if ft.is_file() && path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[test]
fn direct_brand_dispatch_inventory_matches_reviewed_routes() {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_rs_files(Path::new("src"), &mut files).expect("walk src/ for .rs files");

    let mut count = 0;
    let mut per_file: Vec<(String, usize)> = Vec::new();
    for path in &files {
        // Skip modules that are part of the brand plumbing rather than
        // call sites:
        // - `events.rs` defines `dispatch` itself.
        // - `sound_sink.rs` is the sink implementation — its
        // `#[cfg(test)] mod tests` uses `dispatch(BrandEvent::...)`
        // to exercise the sink. Those are test-only event synthesis,
        // not Phase-18-planted production call sites, and must not
        // inflate the freeze count.
        if path
            .file_name()
            .is_some_and(|n| n == "events.rs" || n == "sound_sink.rs")
        {
            continue;
        }
        let content = fs::read_to_string(path).unwrap_or_default();
        let file_count = content.matches("dispatch(BrandEvent::").count();
        if file_count > 0 {
            per_file.push((path.display().to_string(), file_count));
            count += file_count;
        }
    }

    assert_eq!(
        count,
        EXPECTED_DISPATCH_COUNT,
        " freeze: expected {} dispatch sites, found {}. \
         Per-file breakdown:\n{}",
        EXPECTED_DISPATCH_COUNT,
        count,
        per_file
            .iter()
            .map(|(p, n)| format!("  {p}: {n}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
