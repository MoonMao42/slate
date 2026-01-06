/// xtask: Build automation tasks for slate.
/// Provides cargo-based task runner for theme generation and code maintenance.
/// Usage: cargo xtask regen-themes
/// Uses standard Rust patterns: std::env, std::fs, std::process, anyhow::Result
/// See: https://rust-lang.github.io/cargo/guide/build-cache.html
use std::env;
use std::process;

mod parser;
mod transform;
mod writer;

use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("regen-themes") => regen_themes()?,
        _ => {
            eprintln!("Usage: cargo xtask regen-themes");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  regen-themes  Regenerate all theme Rust modules from JSON sources");
            process::exit(1);
        }
    }

    Ok(())
}

/// Regenerate all 18 theme Rust modules from JSON sources in themes/ directory.
/// Flow:
/// 1. Find project root (parent of Cargo.toml with workspace member xtask)
/// 2. Load all .json files from themes/ directory
/// 3. Parse and validate each theme using unified JSON schema
/// 4. Transform to Rust code with WCAG contrast adjustments
/// 5. Write to src/theme/generated/ with @generated headers
/// 6. Format output with rustfmt
/// 7. Emit warnings for contrast ratio issues (non-blocking)
fn regen_themes() -> Result<()> {
    let project_root = find_project_root()?;
    let themes_dir = project_root.join("themes");
    let output_dir = project_root.join("src/theme/generated");

    // Ensure output directory exists
    std::fs::create_dir_all(&output_dir)?;

    // Load all theme JSON files
    let mut theme_entries = Vec::new();
    for entry in std::fs::read_dir(&themes_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "json") {
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let content = std::fs::read_to_string(&path)?;
            let theme = parser::parse_theme_json(&content, &filename)?;
            theme_entries.push((filename, theme));
        }
    }

    // Transform and write each theme
    for (filename, theme) in theme_entries {
        eprintln!("Processing: {}", filename);
        let rust_code = transform::codegen_theme(&theme)?;
        let output_filename = format!("{}.rs", theme.id);
        let output_path = output_dir.join(&output_filename);
        writer::write_generated_file(&output_path, &rust_code)?;
    }

    // Generate mod.rs with re-exports
    writer::generate_mod_rs(&output_dir)?;

    eprintln!("Theme regeneration complete!");
    Ok(())
}

/// Find project root by locating Cargo.toml with [workspace] section.
fn find_project_root() -> Result<std::path::PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        let cargo_toml = current.join("Cargo.toml");

        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml)?;
            if content.contains("[workspace]") {
                return Ok(current);
            }
        }

        if !current.pop() {
            return Err(anyhow::anyhow!(
                "Could not find workspace root (Cargo.toml with [workspace] section)"
            ));
        }
    }
}
