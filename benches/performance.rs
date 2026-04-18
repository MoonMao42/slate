use criterion::{black_box, criterion_group, criterion_main, Criterion};
use slate_cli::adapter::ToolRegistry;
use slate_cli::cli::demo;
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

fn bench_demo_render(c: &mut Criterion) {
    let registry = ThemeRegistry::new().expect("Failed to create theme registry");
    let theme = registry
        .get("catppuccin-mocha")
        .expect("Catppuccin Mocha not found");

    c.bench_function("demo_render_all_blocks", |b| {
        b.iter(|| demo::render_to_string(black_box(&theme.palette)))
    });
}

criterion_group!(benches, bench_apply_theme, bench_demo_render);
criterion_main!(benches);
