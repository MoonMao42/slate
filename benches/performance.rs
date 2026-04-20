use criterion::{black_box, criterion_group, criterion_main, Criterion};
use slate_cli::adapter::ToolRegistry;
use slate_cli::cli::picker::preview::starship_fork::fork_starship_prompt;
use slate_cli::theme::ThemeRegistry;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_env() -> (TempDir, PathBuf) {
    let tempdir = TempDir::new().expect("Failed to create temp directory");
    let home = tempdir.path().to_path_buf();
    (tempdir, home)
}

fn bench_apply_theme(c: &mut Criterion) {
    let registry = ThemeRegistry::new().expect("Failed to create theme registry");
    let theme = registry
        .get("catppuccin-mocha")
        .expect("Catppuccin Mocha not found");
    let (_tempdir, _home) = create_test_env();

    c.bench_function("apply_theme_all_adapters", |b| {
        b.iter(|| {
            let adapter_registry = ToolRegistry::default();
            adapter_registry.apply_theme_to_all(black_box(theme))
        })
    });
}

/// Phase 19 · VALIDATION row 13 — end-to-end picker starship-fork
/// latency. Exercises the production path: `starship_bin=None` forces
/// `which::which("starship")` resolution, matching what the Tab
/// full-preview path does in the real picker.
///
/// D-04 envelope: 30–80 ms on a warm cache. The VALIDATION regression
/// alarm fires at 200 ms. Criterion itself does not hard-fail on latency
/// — hard enforcement is deferred to a future `cargo bench` wrapper that
/// parses criterion output (Phase 20 candidate; not in Phase 19 scope).
/// If the host has no `starship` on PATH the iterator times the probe +
/// early-return path (far under the alarm).
fn bench_starship_fork_latency(c: &mut Criterion) {
    let tmp = TempDir::new().expect("bench tempdir");
    let managed_dir = tmp.path().to_path_buf();
    std::fs::create_dir_all(managed_dir.join("starship")).expect("mk starship/");
    let managed_toml = managed_dir.join("starship").join("active.toml");
    // Minimal starship config — if starship is installed, it renders the
    // default prompt; otherwise the probe misses before config is read.
    std::fs::write(&managed_toml, "# starship default\n").expect("write active.toml");

    c.bench_function("picker_starship_fork_latency", |b| {
        b.iter(|| {
            // None → which::which("starship"). Return is intentionally
            // dropped; criterion measures wall clock of the full call.
            let _ = fork_starship_prompt(
                black_box(&managed_toml),
                black_box(&managed_dir),
                black_box(80),
                None,
            );
        });
    });
}

criterion_group!(benches, bench_apply_theme, bench_starship_fork_latency);
criterion_main!(benches);
