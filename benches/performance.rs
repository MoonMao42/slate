use criterion::{black_box, criterion_group, criterion_main, Criterion};
use slate_cli::adapter::ToolRegistry;
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
    let theme = registry.get("catppuccin-mocha").expect("Catppuccin Mocha not found");
    let (_tempdir, _home) = create_test_env();
    
    c.bench_function("apply_theme_all_adapters", |b| {
        b.iter(|| {
            let adapter_registry = ToolRegistry::default();
            adapter_registry.apply_theme_to_all(black_box(theme))
        })
    });
}

criterion_group!(benches, bench_apply_theme);
criterion_main!(benches);
