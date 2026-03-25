//! freeze: the count of `dispatch(BrandEvent::...)` call sites
//! in `src/` (excluding `events.rs` which defines `dispatch` itself) is
//! locked at 40 (verified 2026-04-24).
//! MUST NOT add, remove, or modify any dispatch site — the SFX
//! sink intercepts existing dispatches from Waves 0-6. If this
//! test fails the plan is changing source-of-truth; open a new phase.
//! Freeze constant verified by:
//! grep -rn "^\s*dispatch(BrandEvent" src/ | grep -v events.rs | wc -l
//! → 40 on 2026-04-24 (main branch).
//! Implementation note: this test walks `src/` using `std::fs::read_dir`
//! recursively — stdlib only, no additional dev-dep (revision 2026-04-24
//! moved to stdlib-only to honor slate's "no unnecessary deps" posture).

use std::fs;
use std::path::{Path, PathBuf};

const EXPECTED_DISPATCH_COUNT: usize = 40;

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
fn phase_20_does_not_mutate_phase_18_dispatch_sites() {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_rs_files(Path::new("src"), &mut files).expect("walk src/ for .rs files");

    let mut count = 0;
    let mut per_file: Vec<(String, usize)> = Vec::new();
    for path in &files {
        // Skip the events module itself — it defines `dispatch`, not callers.
        if path.file_name().is_some_and(|n| n == "events.rs") {
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
