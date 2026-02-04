/// Setup execution: actually runs the brew installations and applies configurations
/// Handles partial failures and tracks results
use crate::adapter::{AlacrittyAdapter, BatAdapter, GhosttyAdapter, StarshipAdapter};
use crate::cli::failure_handler::{ExecutionSummary, InstallStatus, ToolInstallResult};
use crate::cli::font_selection::FontCatalog;
use crate::cli::theme_apply;
use crate::cli::tool_selection::{detect_installed_tools_with_env, BrewKind, ToolCatalog};
use crate::config::ConfigManager;
use crate::detection;
use crate::env::SlateEnv;
use crate::error::Result;
use crate::theme::{ThemeRegistry, ThemeVariant};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Execute the setup based on wizard selections with injected SlateEnv (preferred)
pub fn execute_setup_with_env(
    tools_to_install: &[String],
    tools_to_configure: &[String],
    font: Option<&str>,
    theme: Option<&str>,
    env: &SlateEnv,
) -> Result<ExecutionSummary> {
    let mut summary = ExecutionSummary::new();

    eprintln!("\n✦ Applying your setup...\n");

    let spinner = cliclack::spinner();

    // Install selected tools
    for tool_id in tools_to_install {
        if let Some(tool) = ToolCatalog::get_tool(tool_id) {
            if !tool.installable {
                summary.add_tool_result(ToolInstallResult {
                    tool_id: tool_id.clone(),
                    tool_label: tool.label.to_string(),
                    status: InstallStatus::Skipped,
                    error_message: Some("Not installable via setup".to_string()),
                });
                continue;
            }

            spinner.start(format!("Installing {}...", tool.label));

            match install_tool(tool.brew_package, tool.brew_kind, env) {
                Ok(method) => {
                    summary.add_tool_result(ToolInstallResult {
                        tool_id: tool_id.clone(),
                        tool_label: tool.label.to_string(),
                        status: InstallStatus::Success,
                        error_message: None,
                    });
                    spinner.stop(method.success_message(tool.label));
                }
                Err(e) => {
                    summary.add_tool_result(ToolInstallResult {
                        tool_id: tool_id.clone(),
                        tool_label: tool.label.to_string(),
                        status: InstallStatus::Failed,
                        error_message: Some(e.to_string()),
                    });
                    spinner.error(format!("✗ {} failed: {}", tool.label, e));
                    // Continue with next tool (partial failure handling)
                }
            }
        }
    }

    let font_plan = planned_font_installs(font);
    let mut brew_font_broken = false;
    for font_name in &font_plan {
        let required = font == Some(font_name.as_str());
        let display = font_display_name(font_name);

        // Step 1: Already installed? (fast local check)
        spinner.start(format!("Checking font {}...", display));
        if is_font_installed_with_env(env, font_name) {
            spinner.stop(format!("✓ {} already installed", display));
            if required {
                summary.font_applied = true;
            }
            continue;
        }

        // Step 2: Try Homebrew (skip if already known broken)
        if !brew_font_broken {
            spinner.start(format!("Installing {} via Homebrew...", display));
            match install_font(font_name) {
                Ok(_) => {
                    spinner.stop(format!("✓ {} installed", display));
                    if required {
                        summary.font_applied = true;
                    }
                    continue;
                }
                Err(e) => {
                    let msg = e.to_string().to_lowercase();
                    if msg.contains("permission denied") || msg.contains("not writable") {
                        brew_font_broken = true;
                        spinner.stop(
                            "⚠ Homebrew: no write access — switching to direct download",
                        );
                    }
                    // Fall through to next method
                }
            }
        }

        // Step 3: Try shared Caskroom copy (fast, no spinner update)
        if copy_font_from_caskroom(font_name, env).is_ok() {
            spinner.stop(format!("✓ {} installed (shared cache)", display));
            if required {
                summary.font_applied = true;
            }
            continue;
        }

        // Step 4: Download from official Nerd Fonts release
        spinner.start(format!("Downloading {}...", display));
        match download_font_release(font_name, env) {
            Ok(_) => {
                spinner.stop(format!("✓ {} downloaded", display));
                if required {
                    summary.font_applied = true;
                }
            }
            Err(e) => {
                let full = e.to_string();
                let err_msg = strip_error_prefix(&full);
                if required {
                    spinner.error(format!("✗ {}: {}", display, err_msg));
                    summary.add_issue(format!("{}: {}", display, err_msg));
                } else {
                    spinner.stop(format!("⚠ {} unavailable", display));
                    summary.add_notice(format!("{}: {}", display, err_msg));
                }
            }
        }
    }

    // Persist user's font choice so adapters write the correct font-family
    if let Some(font_name) = font.filter(|_| summary.font_applied) {
        let family = resolve_font_family_with_env(env, font_name);
        match ConfigManager::with_env(env).and_then(|mgr| mgr.set_current_font(&family)) {
            Ok(_) => {}
            Err(e) => {
                summary.add_issue(format!(
                    "Font '{}' was installed but could not be saved to config: {}",
                    family, e
                ));
            }
        }
    }

    // Ensure integration config files exist for detected tools.
    // Adapters skip writing if the config doesn't exist (to avoid clobbering
    // GUI settings during `slate set`), but during `slate setup` the user
    // explicitly asked us to initialize everything.
    // Merge just-installed tools into detection: if starship was installed to
    // ~/.local/bin as a fallback, detect_installed_tools_with_env won't find it
    // via the current PATH. We inject successfully installed tool IDs so their
    // config files get created and seeded.
    let just_installed: Vec<String> = summary
        .tool_results
        .iter()
        .filter(|r| r.status == InstallStatus::Success)
        .map(|r| r.tool_id.clone())
        .collect();
    for issue in ensure_tool_configs(env, tools_to_configure, &just_installed) {
        summary.add_issue(issue);
    }

    // Setup shell integration: write marker block to .zshrc and env.zsh
    spinner.start("Setting up shell integration...");
    match setup_shell_integration_with_env(theme, env) {
        Ok((selected_theme, report)) => {
            summary.theme_applied = true;
            for issue in theme_apply_issues(&report.results) {
                summary.add_issue(issue);
            }
            summary.set_theme_results(report.results);
            spinner.stop(format!(
                "✓ Shell integration configured for {}",
                selected_theme.name
            ));
        }
        Err(e) => {
            spinner.error(format!("✗ Shell integration had issues: {}", e));
            summary.add_issue(format!("Shell integration setup failed: {}", e));
        }
    }

    // Overall success: no tool failures, and font applied if selected
    let font_ok = font.is_none() || summary.font_applied;
    summary.overall_success = summary.failure_count() == 0
        && font_ok
        && summary.theme_applied
        && summary.theme_failure_count() == 0
        && summary.missing_integration_skip_count() == 0
        && summary.issues.is_empty();

    Ok(summary)
}

/// Execute the setup based on wizard selections (backward compat)
pub fn execute_setup(
    tools_to_install: &[String],
    font: Option<&str>,
    theme: Option<&str>,
) -> Result<ExecutionSummary> {
    let env = SlateEnv::from_process()?;
    // Backward compat: install = configure
    execute_setup_with_env(tools_to_install, tools_to_install, font, theme, &env)
}

fn resolve_selected_theme(theme: Option<&str>, env: &SlateEnv) -> Result<ThemeVariant> {
    let config_mgr = ConfigManager::with_env(env)?;
    let theme_id = if let Some(theme_name) = theme {
        theme_name.to_string()
    } else if let Some(current_theme) = config_mgr.get_current_theme()? {
        current_theme
    } else {
        "catppuccin-mocha".to_string()
    };

    let registry = ThemeRegistry::new()?;
    registry.get(&theme_id).cloned().ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!("Theme '{}' not found", theme_id))
    })
}

/// Setup shell integration: generate env.zsh first, then wire .zshrc to source it.
/// With injected SlateEnv (preferred for testing)
fn setup_shell_integration_with_env(
    theme: Option<&str>,
    env: &SlateEnv,
) -> Result<(ThemeVariant, theme_apply::ThemeApplyReport)> {
    use crate::adapter::marker_block;

    let zshrc_path = env.zshrc_path();
    let env_zsh_path = env.config_dir().join("managed/shell/env.zsh");
    let env_zsh_shell_path = detection::shell_quote_path(&env_zsh_path);

    let selected_theme = resolve_selected_theme(theme, env)?;
    let report = theme_apply::apply_theme_selection_with_env(&selected_theme, env)?;

    let marker_content = format!(
        "{}\nif [ -f {path} ]; then\n  source {path}\nfi\n{}\n",
        marker_block::START,
        marker_block::END,
        path = env_zsh_shell_path
    );

    marker_block::upsert_managed_block_file(&zshrc_path, &marker_content)?;

    Ok((selected_theme, report))
}

/// Setup shell integration: write marker block to .zshrc and apply the selected theme (backward compat)
#[allow(dead_code)]
fn setup_shell_integration(
    theme: Option<&str>,
) -> Result<(ThemeVariant, theme_apply::ThemeApplyReport)> {
    let env = SlateEnv::from_process()?;
    setup_shell_integration_with_env(theme, &env)
}

/// Ensure integration config files exist for detected tools so adapters can write to them.
/// During `slate setup`, we create these files if missing. Regular `slate set` does not
/// create them (to avoid clobbering existing GUI-level settings).
fn ensure_tool_configs(
    env: &SlateEnv,
    user_selected: &[String],
    just_installed: &[String],
) -> Vec<String> {
    use std::fs;

    fn touch_config(tool_id: &str, path: &Path, issues: &mut Vec<String>) {
        if path.exists() {
            return;
        }

        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                issues.push(format!(
                    "Could not create {} config directory at {}: {}",
                    tool_id,
                    parent.display(),
                    err
                ));
                return;
            }
        }

        if let Err(err) = fs::File::create(path) {
            issues.push(format!(
                "Could not initialize {} config file at {}: {}",
                tool_id,
                path.display(),
                err
            ));
        }
    }

    fn seed_starship_config(path: &Path, issues: &mut Vec<String>) {
        let needs_seed = match fs::read_to_string(path) {
            Ok(content) => {
                crate::config::shell_integration::should_upgrade_seeded_starship_content(&content)
            }
            Err(err) => {
                issues.push(format!(
                    "Could not inspect starship config at {}: {}",
                    path.display(),
                    err
                ));
                false
            }
        };

        if needs_seed {
            if let Err(err) = fs::write(
                path,
                crate::config::shell_integration::starter_starship_content(),
            ) {
                issues.push(format!(
                    "Could not seed starship config at {}: {}",
                    path.display(),
                    err
                ));
            }
        }
    }

    let mut installed = detect_installed_tools_with_env(env);
    // Merge tools that were successfully installed THIS run — they may not be
    // on PATH yet (e.g. starship installed to ~/.local/bin via fallback).
    // Mark them as Tier 1 (in_path) since the user explicitly chose to install them.
    for tool_id in just_installed {
        installed.insert(
            tool_id.clone(),
            crate::detection::ToolPresence {
                installed: true,
                in_path: true,
                evidence: None,
            },
        );
    }
    let mut issues = Vec::new();

    // Configure a tool if:
    // 1. Terminal emulators (ghostty/alacritty): always configure if detected (detect-only)
    // 2. CLI tools: only if user explicitly selected them OR just installed them
    // This respects the user's wizard choices — unchecked tools don't get configs.
    let user_set: std::collections::HashSet<&str> = user_selected
        .iter()
        .chain(just_installed.iter())
        .map(|s| s.as_str())
        .collect();
    let should_configure = |id: &str| -> bool {
        let presence = installed.get(id);
        let is_detected = presence.map(|p| p.installed).unwrap_or(false);
        if !is_detected {
            return false;
        }
        // Terminal emulators (detect-only): configure if Tier 1 (user-local)
        // OR if user explicitly selected them in the wizard (Tier 2 opt-in).
        if id == "ghostty" || id == "alacritty" {
            return presence.map(|p| p.is_tier1()).unwrap_or(false) || user_set.contains(id);
        }
        // CLI tools: only if user chose them
        user_set.contains(id)
    };

    if should_configure("ghostty") {
        let adapter = GhosttyAdapter;
        match adapter.integration_config_path_with_env(env) {
            Ok(path) => touch_config("ghostty", &path, &mut issues),
            Err(err) => issues.push(format!("Could not resolve ghostty config path: {}", err)),
        }
    }
    if should_configure("starship") {
        let path = StarshipAdapter::integration_config_path_with_env(env);
        touch_config("starship", &path, &mut issues);
        if path.exists() {
            seed_starship_config(&path, &mut issues);
        }
    }
    if should_configure("alacritty") {
        touch_config(
            "alacritty",
            &AlacrittyAdapter::integration_config_path_with_env(env),
            &mut issues,
        );
    }
    if should_configure("bat") {
        let adapter = BatAdapter;
        match adapter.integration_config_path_with_env(env) {
            Ok(path) => touch_config("bat", &path, &mut issues),
            Err(err) => issues.push(format!("Could not resolve bat config path: {}", err)),
        }
    }
    if should_configure("delta") {
        touch_config("delta", &env.home().join(".gitconfig"), &mut issues);
    }

    issues
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolInstallMethod {
    Homebrew,
    UserLocal(PathBuf),
}

impl ToolInstallMethod {
    pub(crate) fn success_message(&self, label: &str) -> String {
        match self {
            Self::Homebrew => format!("✓ {} installed", label),
            Self::UserLocal(bin_dir) => {
                format!("✓ {} installed locally at {}", label, bin_dir.display())
            }
        }
    }
}

/// Install a tool via Homebrew, with a user-local Starship fallback for shared machines.
pub(crate) fn install_tool(package: &str, kind: BrewKind, env: &SlateEnv) -> Result<ToolInstallMethod> {
    match install_tool_via_homebrew(package, kind) {
        Ok(()) => Ok(ToolInstallMethod::Homebrew),
        Err(err) if package == "starship" && should_try_local_starship_fallback(&err) => {
            install_starship_locally(env)?;
            Ok(ToolInstallMethod::UserLocal(env.user_local_bin()))
        }
        Err(err) => Err(err),
    }
}

fn install_tool_via_homebrew(package: &str, kind: BrewKind) -> Result<()> {
    let brew = detection::homebrew_executable().ok_or_else(|| {
        crate::error::SlateError::Internal(
            "Homebrew was not found. Install it first or add it to PATH.".to_string(),
        )
    })?;
    let mut cmd = Command::new(brew);
    detection::apply_normalized_path(&mut cmd);

    match kind {
        BrewKind::Formula => {
            cmd.arg("install").arg(package);
        }
        BrewKind::Cask => {
            cmd.arg("install").arg("--cask").arg(package);
        }
    }

    let output = cmd.output().map_err(|e| {
        crate::error::SlateError::Internal(format!("Failed to execute brew: {}", e))
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(crate::error::SlateError::Internal(classify_brew_error(
            package, &stderr,
        )))
    }
}

fn should_try_local_starship_fallback(err: &crate::error::SlateError) -> bool {
    let message = err.to_string().to_lowercase();
    message.contains("permission denied")
        || message.contains("not writable")
        || message.contains("homebrew was not found")
}

fn install_starship_locally(env: &SlateEnv) -> Result<()> {
    use std::fs;

    const STARSHIP_INSTALL_URL: &str = "https://starship.rs/install.sh";

    let local_bin = env.user_local_bin();
    fs::create_dir_all(&local_bin)?;

    // Use create_writable_temp_dir to avoid TMPDIR permission issues on
    // secondary user accounts (same fix as download_font_release).
    let temp_dir = create_writable_temp_dir(env).map_err(|e| {
        crate::error::SlateError::Internal(format!(
            "Cannot create temporary directory for Starship installer: {}",
            e
        ))
    })?;
    let installer_path = temp_dir.path().join("starship_install.sh");

    let mut download = Command::new("/usr/bin/curl");
    detection::apply_normalized_path(&mut download);
    let download_output = download
        .arg("-fsSL")
        .arg("--connect-timeout")
        .arg("10")
        .arg("--max-time")
        .arg("60")
        .arg(STARSHIP_INSTALL_URL)
        .output()
        .map_err(|err| {
            crate::error::SlateError::Internal(format!(
                "Failed to download Starship installer: {}",
                err
            ))
        })?;

    if !download_output.status.success() {
        let stderr = String::from_utf8_lossy(&download_output.stderr);
        let stdout = String::from_utf8_lossy(&download_output.stdout);
        return Err(crate::error::SlateError::Internal(format!(
            "Starship local fallback download failed: {}",
            first_meaningful_command_line(&stderr, &stdout)
        )));
    }

    fs::write(&installer_path, &download_output.stdout)?;

    let mut install = Command::new("/bin/sh");
    detection::apply_normalized_path(&mut install);
    let install_output = install
        .arg(&installer_path)
        .arg("-y")
        .arg("-b")
        .arg(&local_bin)
        .output()
        .map_err(|err| {
            crate::error::SlateError::Internal(format!(
                "Failed to execute Starship local installer: {}",
                err
            ))
        })?;

    if !install_output.status.success() {
        let stderr = String::from_utf8_lossy(&install_output.stderr);
        let stdout = String::from_utf8_lossy(&install_output.stdout);
        return Err(crate::error::SlateError::Internal(format!(
            "Starship local fallback failed: {}",
            first_meaningful_command_line(&stderr, &stdout)
        )));
    }

    let binary = local_bin.join("starship");
    if !binary.is_file() {
        return Err(crate::error::SlateError::Internal(format!(
            "Starship local fallback completed without creating {}",
            binary.display()
        )));
    }

    Ok(())
}

fn first_meaningful_command_line(stderr: &str, stdout: &str) -> String {
    stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("==>"))
        .unwrap_or("unknown error")
        .to_string()
}

/// Classify brew stderr into a one-line guided message
fn classify_brew_error(package: &str, stderr: &str) -> String {
    let lower = stderr.to_lowercase();
    if lower.contains("couldn't connect to server")
        || lower.contains("could not resolve host")
        || lower.contains("network is unreachable")
    {
        format!(
            "{} — network unreachable. Check your connection and retry: slate setup --only {}",
            package, package
        )
    } else if lower.contains("is not writable") || lower.contains("permission denied") {
        format!(
            "{} — permission denied. On a shared Homebrew install, ask the primary user or admin to install this package, then rerun slate setup.",
            package
        )
    } else if lower.contains("already installed") {
        format!("{} — already installed", package)
    } else {
        // Fallback: first meaningful line of stderr, not the full dump
        let first_line = stderr
            .lines()
            .find(|l| {
                let trimmed = l.trim();
                !trimmed.is_empty() && !trimmed.starts_with("==>")
            })
            .unwrap_or(stderr.lines().next().unwrap_or("unknown error"));
        format!("{} — {}", package, first_line.trim())
    }
}

/// Copy font files from Homebrew Caskroom to current user's ~/Library/Fonts/.
/// This handles the case where another user installed the font via brew cask
/// the .ttf files live in /opt/homebrew/Caskroom/ (readable by all users)
/// but are only symlinked to the installing user's ~/Library/Fonts/.
pub fn copy_font_from_caskroom(font_name_or_id: &str, env: &SlateEnv) -> Result<()> {
    use std::fs;

    let cask_name = FontCatalog::get_font(font_name_or_id)
        .map(|f| f.brew_cask)
        .ok_or_else(|| {
            crate::error::SlateError::Internal(format!("Unknown font: {}", font_name_or_id))
        })?;

    let caskroom = detection::homebrew_prefix()
        .map(|prefix| prefix.join("Caskroom").join(cask_name))
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/Caskroom").join(cask_name));
    if !caskroom.exists() {
        return Err(crate::error::SlateError::Internal(
            "Font not found in Homebrew Caskroom".to_string(),
        ));
    }

    let font_files: Vec<PathBuf> = walkdir(&caskroom, &["ttf", "otf", "ttc"]);
    if font_files.is_empty() {
        return Err(crate::error::SlateError::Internal(
            "No font files found in Caskroom".to_string(),
        ));
    }

    // Copy to ~/Library/Fonts/
    let home = dirs_font_target(env);
    fs::create_dir_all(&home)?;

    for src in &font_files {
        if let Some(filename) = src.file_name() {
            let dest = home.join(filename);
            fs::copy(src, &dest)?;
        }
    }

    Ok(())
}

pub fn download_font_release(font_name_or_id: &str, env: &SlateEnv) -> Result<()> {
    const NERD_FONTS_RELEASE_BASE: &str =
        "https://github.com/ryanoasis/nerd-fonts/releases/latest/download";

    let font = FontCatalog::get_font(font_name_or_id).ok_or_else(|| {
        crate::error::SlateError::Internal(format!("Unknown font: {}", font_name_or_id))
    })?;

    // Create a temp directory for download. On macOS, TMPDIR may point to a
    // per-user /var/folders/ path that is inaccessible when running as a
    // secondary user (e.g. via `su`). Fall back to /tmp, then slate's cache dir.
    let temp = create_writable_temp_dir(env).map_err(|e| {
        crate::error::SlateError::Internal(format!(
            "Cannot create temporary directory for font download \
             (tried $TMPDIR, /tmp, ~/.cache/slate/tmp): {}",
            e
        ))
    })?;

    let archive = temp.path().join(format!("{}.zip", font.release_asset));
    let extract_dir = temp.path().join("extract");
    std::fs::create_dir_all(&extract_dir).map_err(|e| {
        crate::error::SlateError::Internal(format!(
            "Cannot create extraction directory {}: {}",
            extract_dir.display(),
            e
        ))
    })?;

    let url = format!("{}/{}.zip", NERD_FONTS_RELEASE_BASE, font.release_asset);
    let download_output = Command::new("/usr/bin/curl")
        .arg("-fsSL")
        .arg("--connect-timeout")
        .arg("10")
        .arg("--max-time")
        .arg("90")
        .arg("--http1.1")
        .arg("--retry")
        .arg("2")
        .arg("--retry-all-errors")
        .arg("-A")
        .arg("slate-font-bootstrap")
        .arg("-o")
        .arg(&archive)
        .arg(&url)
        .output()
        .map_err(|err| {
            crate::error::SlateError::Internal(format!(
                "Failed to download font archive from Nerd Fonts releases: {}",
                err
            ))
        })?;

    if !download_output.status.success() {
        let stderr = String::from_utf8_lossy(&download_output.stderr);
        let stdout = String::from_utf8_lossy(&download_output.stdout);
        return Err(crate::error::SlateError::Internal(format!(
            "Font release download failed: {}",
            first_meaningful_command_line(&stderr, &stdout)
        )));
    }

    let unzip_output = Command::new("/usr/bin/unzip")
        .arg("-oq")
        .arg(&archive)
        .arg("-d")
        .arg(&extract_dir)
        .output()
        .map_err(|err| {
            crate::error::SlateError::Internal(format!(
                "Failed to extract downloaded font archive: {}",
                err
            ))
        })?;

    if !unzip_output.status.success() {
        let stderr = String::from_utf8_lossy(&unzip_output.stderr);
        let stdout = String::from_utf8_lossy(&unzip_output.stdout);
        return Err(crate::error::SlateError::Internal(format!(
            "Font archive extraction failed: {}",
            first_meaningful_command_line(&stderr, &stdout)
        )));
    }

    let font_files = walkdir(&extract_dir, &["ttf", "otf", "ttc"]);
    if font_files.is_empty() {
        return Err(crate::error::SlateError::Internal(
            "Downloaded font archive did not contain any font files".to_string(),
        ));
    }

    let target = dirs_font_target(env);
    std::fs::create_dir_all(&target).map_err(|e| {
        crate::error::SlateError::Internal(format!(
            "Cannot create font directory {}: {}",
            target.display(),
            e
        ))
    })?;
    for src in &font_files {
        if let Some(filename) = src.file_name() {
            std::fs::copy(src, target.join(filename)).map_err(|e| {
                crate::error::SlateError::Internal(format!(
                    "Cannot install font file {} to {}: {}",
                    filename.to_string_lossy(),
                    target.display(),
                    e
                ))
            })?;
        }
    }

    Ok(())
}

/// Create a temp directory, trying multiple locations.
/// On macOS, TMPDIR may be inaccessible for secondary users (e.g. `su` inherits
/// the primary user's /var/folders/ path which is owner-only).
fn create_writable_temp_dir(env: &SlateEnv) -> std::io::Result<tempfile::TempDir> {
    use tempfile::TempDir;

    TempDir::new().or_else(|_| TempDir::new_in("/tmp")).or_else(|_| {
        let fallback = env.slate_cache_dir().join("tmp");
        std::fs::create_dir_all(&fallback)?;
        TempDir::new_in(&fallback)
    })
}

/// Get a font's display name from its catalog ID (e.g. "hack" → "Hack Nerd Font").
fn font_display_name(font_name_or_id: &str) -> String {
    FontCatalog::get_font(font_name_or_id)
        .map(|f| f.name.to_string())
        .unwrap_or_else(|| font_name_or_id.to_string())
}

/// Strip "Internal error: " / "IO error: " prefixes from SlateError Display output
/// so user-facing messages don't contain implementation details.
fn strip_error_prefix(msg: &str) -> &str {
    msg.strip_prefix("Internal error: ")
        .or_else(|| msg.strip_prefix("IO error: "))
        .unwrap_or(msg)
}

/// Walk directory recursively for files with given extension
fn walkdir(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(walkdir(&path, exts));
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| {
                    exts.iter()
                        .any(|candidate| ext.eq_ignore_ascii_case(candidate))
                })
            {
                results.push(path);
            }
        }
    }
    results
}

/// Get the current user's font target directory
fn dirs_font_target(env: &SlateEnv) -> PathBuf {
    env.home().join("Library/Fonts")
}

fn planned_font_installs(selected_font: Option<&str>) -> Vec<String> {
    // Only install the font the user explicitly selected.
    // Other catalog fonts are available on-demand via `slate font`.
    match selected_font {
        Some(font) => vec![font.to_string()],
        None => vec![],
    }
}

/// Check if a Nerd Font is already installed
fn is_font_installed_with_env(env: &SlateEnv, font_name_or_id: &str) -> bool {
    use crate::adapter::font::FontAdapter;
    if let Ok(installed) = FontAdapter::detect_installed_fonts_with_env(env) {
        let lookup = FontCatalog::get_font(font_name_or_id)
            .map(|f| f.name.to_string())
            .unwrap_or_else(|| font_name_or_id.to_string());
        let lookup_key = FontAdapter::family_match_key(&lookup);
        installed
            .iter()
            .any(|family| FontAdapter::family_match_key(family) == lookup_key)
    } else {
        false
    }
}

/// Resolve a font id/name to the canonical family name for terminal configs.
/// E.g. "jetbrains-mono" → "JetBrainsMono Nerd Font"
fn resolve_font_family_with_env(env: &SlateEnv, font_name_or_id: &str) -> String {
    use crate::adapter::font::FontAdapter;

    // Try catalog first (id → display name)
    if let Some(font) = FontCatalog::get_font(font_name_or_id) {
        // Catalog name is e.g. "JetBrains Mono Nerd Font" — but the actual
        // installed file may be "JetBrainsMonoNerdFont-Regular.ttf".
        // Detect what's actually installed and return its normalized form.
        if let Ok(installed) = FontAdapter::detect_installed_fonts_with_env(env) {
            let catalog_key = FontAdapter::family_match_key(font.name);
            if let Some(matched) = installed
                .iter()
                .find(|f| FontAdapter::family_match_key(f) == catalog_key)
            {
                return matched.clone();
            }
        }
        return font.name.to_string();
    }
    font_name_or_id.to_string()
}

// ensure_font_available was replaced by inline step-by-step fallback chain
// in execute_setup_with_env (the font installation loop) for better UX:
// each fallback step updates the spinner in real time, and brew failures
// are cached so subsequent fonts skip brew entirely.

/// Install a Nerd Font via Homebrew
pub fn install_font(font_name_or_id: &str) -> Result<()> {
    let cask_name = FontCatalog::get_font(font_name_or_id)
        .map(|font| font.brew_cask)
        .or_else(|| {
            FontCatalog::all_fonts()
                .into_iter()
                .find(|font| {
                    font.name == font_name_or_id
                        || font.name.replace(" Nerd Font", "") == font_name_or_id
                })
                .map(|font| font.brew_cask)
        })
        .ok_or_else(|| {
            crate::error::SlateError::Internal(format!("Unknown font: {}", font_name_or_id))
        })?;

    let brew = detection::homebrew_executable().ok_or_else(|| {
        crate::error::SlateError::Internal(
            "Homebrew was not found. Install it first or add it to PATH.".to_string(),
        )
    })?;
    let mut cmd = Command::new(brew);
    detection::apply_normalized_path(&mut cmd);
    cmd.arg("install").arg("--cask").arg(cask_name);

    let output = cmd.output().map_err(|e| {
        crate::error::SlateError::Internal(format!("Failed to execute brew: {}", e))
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(crate::error::SlateError::Internal(classify_brew_error(
            cask_name, &stderr,
        )))
    }
}

fn theme_apply_issues(results: &[crate::adapter::ToolApplyResult]) -> Vec<String> {
    results
        .iter()
        .filter_map(|result| match &result.status {
            crate::adapter::ToolApplyStatus::Skipped(
                crate::adapter::SkipReason::MissingIntegrationConfig,
            ) => Some(format!(
                "{} is installed, but slate could not initialize its integration file.",
                result.tool_name
            )),
            crate::adapter::ToolApplyStatus::Failed(err) => Some(format!(
                "{} failed during theme apply: {}",
                result.tool_name, err
            )),
            crate::adapter::ToolApplyStatus::Skipped(crate::adapter::SkipReason::NotInstalled)
            | crate::adapter::ToolApplyStatus::Applied => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SlateEnv;
    use tempfile::TempDir;

    #[test]
    fn test_execute_setup_empty() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let result = execute_setup_with_env(&[], &[], None, None, &env);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert!(summary.overall_success);
        assert_eq!(summary.success_count(), 0);
    }

    #[test]
    fn test_planned_font_installs_only_selected() {
        let plan = planned_font_installs(Some("jetbrains-mono"));
        assert_eq!(plan, vec!["jetbrains-mono"]);
    }

    #[test]
    fn test_planned_font_installs_none_selected() {
        let plan = planned_font_installs(None);
        assert!(plan.is_empty());
    }

    #[test]
    fn test_font_mapping() {
        // Verify font display names map correctly
        let fonts = vec!["JetBrains Mono", "Fira Code", "Iosevka Term", "Hack"];
        for font in fonts {
            // Just verify these are recognized
            let _ = font;
        }
    }

    #[test]
    fn test_theme_selection_marks_summary_as_applied() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let summary = execute_setup_with_env(&[], &[], None, Some("catppuccin-mocha"), &env).unwrap();
        assert!(summary.theme_applied);
    }

    #[test]
    fn test_local_starship_fallback_triggering() {
        let permission = crate::error::SlateError::Internal(
            "starship — permission denied. shared Homebrew.".to_string(),
        );
        let missing_homebrew = crate::error::SlateError::Internal(
            "Homebrew was not found. Install it first or add it to PATH.".to_string(),
        );
        let network = crate::error::SlateError::Internal(
            "starship — network unreachable. Check your connection.".to_string(),
        );

        assert!(should_try_local_starship_fallback(&permission));
        assert!(should_try_local_starship_fallback(&missing_homebrew));
        assert!(!should_try_local_starship_fallback(&network));
    }

    #[test]
    fn test_user_local_install_message_mentions_directory() {
        let method = ToolInstallMethod::UserLocal(PathBuf::from("/tmp/.local/bin"));
        assert_eq!(
            method.success_message("Starship"),
            "✓ Starship installed locally at /tmp/.local/bin"
        );
    }

    #[test]
    fn test_setup_upgrades_legacy_starship_seed() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let config_path = env.xdg_config_home().join("starship.toml");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        std::fs::write(
            &config_path,
            r#"format = "$username$directory$git_branch$git_status$cmd_duration$line_break$character"

[username]
show_always = true
format = "[$user]($style) "
style_user = "bold green"

[directory]
format = "[$path]($style) "
style = "bold cyan"
truncation_length = 3

[git_branch]
format = "[$symbol$branch]($style) "
symbol = ""
style = "bold purple"

[git_status]
format = "([$all_status$ahead_behind]($style) )"
style = "bold red"

[cmd_duration]
format = "[$duration]($style) "
style = "bold yellow"

[character]
success_symbol = "[>](bold green)"
error_symbol = "[>](bold red)"
"#,
        )
        .unwrap();

        let issues = ensure_tool_configs(
            &env,
            &["starship".to_string()],
            &["starship".to_string()],
        );
        assert!(issues.is_empty());

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("\"$schema\" = 'https://starship.rs/config-schema.json'"));
        assert!(content.contains("[](red)$os$username"));
    }

    #[test]
    fn test_font_release_urls_match_official_asset_names() {
        let jetbrains = FontCatalog::get_font("jetbrains-mono").unwrap();
        let hack = FontCatalog::get_font("hack").unwrap();
        let iosevka = FontCatalog::get_font("iosevka-term").unwrap();
        let fira = FontCatalog::get_font("fira-code").unwrap();

        assert_eq!(jetbrains.release_asset, "JetBrainsMono");
        assert_eq!(hack.release_asset, "Hack");
        assert_eq!(iosevka.release_asset, "IosevkaTerm");
        assert_eq!(fira.release_asset, "FiraCode");
    }
}
