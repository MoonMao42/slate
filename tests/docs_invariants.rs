use std::fs;
use std::path::Path;

use slate_cli::theme::ThemeRegistry;

fn count_gallery_rows(readme: &str) -> usize {
    let start = readme
        .find("<!-- THEME-GALLERY-START -->")
        .expect("gallery start marker");
    let end = readme
        .find("<!-- THEME-GALLERY-END -->")
        .expect("gallery end marker");

    readme[start..end]
        .lines()
        .filter(|line| line.starts_with("| ") && line.contains("<svg "))
        .count()
}

#[test]
fn readme_theme_gallery_matches_registry_count() {
    let registry = ThemeRegistry::new().expect("theme registry loads");
    let theme_count = registry.all().len();

    for path in ["README.md", "README.zh-CN.md"] {
        let readme = fs::read_to_string(path).expect("README exists");
        assert_eq!(
            count_gallery_rows(&readme),
            theme_count,
            "{path} gallery row count must match theme registry"
        );
    }
}

#[test]
fn docs_reference_existing_gallery_tooling() {
    assert!(
        Path::new("scripts/render-theme-gallery.sh").is_file(),
        "README gallery comment references scripts/render-theme-gallery.sh"
    );
    assert!(
        Path::new("tests/docs_invariants.rs").is_file(),
        "README gallery note references tests/docs_invariants.rs"
    );
}

#[test]
fn readmes_use_current_install_and_theme_counts() {
    let readme = fs::read_to_string("README.md").expect("README exists");
    let readme_zh = fs::read_to_string("README.zh-CN.md").expect("README.zh-CN exists");

    assert!(readme.contains("brew install MoonMao42/tap/slate-cli"));
    assert!(readme_zh.contains("brew install MoonMao42/tap/slate-cli"));
    assert!(readme.contains("20 Neovim colorschemes"));
    assert!(readme_zh.contains("20 套对应全部主题家族的 Neovim 配色"));
}
