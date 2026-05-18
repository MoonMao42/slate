use std::fs;
use std::path::Path;
use std::process::Command;

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

fn parse_sha256_output(output: std::process::Output) -> Option<String> {
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .and_then(|stdout| stdout.split_whitespace().next().map(str::to_string))
}

fn sha256_for(path: &Path) -> Option<String> {
    Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .ok()
        .and_then(parse_sha256_output)
        .or_else(|| {
            Command::new("sha256sum")
                .arg(path)
                .output()
                .ok()
                .and_then(parse_sha256_output)
        })
}

#[test]
fn sfx_sha256sums_match_wav_files() {
    let sums = fs::read_to_string("resources/sfx/SHA256SUMS").expect("SFX checksum file exists");

    for (line_index, line) in sums.lines().enumerate() {
        let mut parts = line.split_whitespace();
        let expected = parts
            .next()
            .unwrap_or_else(|| panic!("line {} is missing a checksum", line_index + 1));
        let filename = parts
            .next()
            .unwrap_or_else(|| panic!("line {} is missing a filename", line_index + 1));
        assert!(
            parts.next().is_none(),
            "line {} should contain only checksum and filename",
            line_index + 1
        );

        let path = Path::new("resources/sfx").join(filename);
        assert!(path.is_file(), "{filename} listed in SHA256SUMS must exist");

        let actual = sha256_for(&path)
            .expect("SFX checksum test needs either `shasum` or `sha256sum` on PATH");
        assert_eq!(
            actual, expected,
            "{filename} checksum must match resources/sfx/SHA256SUMS"
        );
    }
}
