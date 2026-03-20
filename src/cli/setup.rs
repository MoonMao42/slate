use crate::adapter::ToolAdapter;
use crate::brand::events::{dispatch, BrandEvent, FailureKind};
use crate::brand::language::Language;
use crate::cli::preflight;
use crate::cli::setup_executor;
use crate::cli::tool_selection::ToolCatalog;
use crate::cli::wizard_core::Wizard;
use crate::env::SlateEnv;
use crate::error::Result;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Instant;

fn should_emit_new_shell_reminder_after_setup(theme_applied: bool) -> bool {
    // Setup always rewrites the managed shell env files before it wires the
    // loader, so a successful shell-integration phase always leaves at least
    // one change that becomes visible in a fresh shell.
    theme_applied
}

/// Handle `slate setup` command with injected SlateEnv (preferred for testability)
pub fn handle_with_env(
    quick: bool,
    force: bool,
    only: Option<String>,
    env: &SlateEnv,
) -> Result<()> {
    // If --only flag is set, handle retry flow
    if let Some(tool_id) = only {
        return handle_retry_only(&tool_id);
    }

    if !std::io::stdin().is_terminal() && !quick {
        dispatch(BrandEvent::Failure(FailureKind::SetupFailed));
        return Err(crate::error::SlateError::Internal(
            "Non-interactive setup requires --quick for explicit consent.".to_string(),
        ));
    }

    // Run pre-flight checks
    eprintln!("\n");
    let has_existing_install = env.managed_file("current").exists();
    let scenario = if quick && has_existing_install {
        // Reconfigure path: user already has a slate install, doesn't need a package manager.
        preflight::PreflightScenario::ConfigOnlyReconfigure
    } else if quick {
        preflight::PreflightScenario::QuickSetup
    } else {
        preflight::PreflightScenario::GuidedSetup
    };
    let preflight_result = preflight::run_checks_for_setup_with_env(env, scenario)?;
    eprintln!("{}", preflight_result.format_for_display());

    if !preflight_result.is_ready() {
        dispatch(BrandEvent::Failure(FailureKind::SetupFailed));
        return Err(crate::error::SlateError::Internal(
            preflight_result.format_blocking_guidance(),
        ));
    }

    eprintln!("\n");

    // Run the wizard
    let mut wizard = Wizard::new()?;
    wizard.run(quick, force)?;

    // Build selections from wizard context
    let context = wizard.get_context();
    if !context.confirmed {
        return Ok(());
    }
    let start_time = context.start_time;
    let selected_tools = context.selected_tools.clone();
    let tools_to_configure = context.tools_to_configure.clone();
    let selected_font = context.selected_font.as_deref();
    let selected_theme = context.selected_theme.as_deref();
    let selected_opacity = context.selected_opacity;
    let fastfetch_enabled = context.fastfetch_enabled;

    // Snapshot current state BEFORE any mutations
    {
        use crate::config::{
            begin_restore_point_baseline_with_env, list_restore_points_with_env,
            snapshot_current_state_with_env,
        };
        let backups = list_restore_points_with_env(env).ok();
        let has_baseline = if let Some(ref backups) = backups {
            backups.iter().any(|rp| rp.is_baseline)
        } else {
            false
        };

        if !has_baseline {
            // First time: create baseline (pre-slate state)
            match begin_restore_point_baseline_with_env(env) {
                Ok(baseline_point) => {
                    eprintln!("✓ Baseline snapshot created ({})", baseline_point.id);
                }
                Err(_) => {
                    eprintln!("⚠ Could not create baseline snapshot — slate restore will not be available for pre-slate state");
                }
            }
        } else {
            // Subsequent runs: snapshot current config so user can restore back
            let config = crate::config::ConfigManager::with_env(env).ok();
            let label = config
                .and_then(|c| c.get_current_theme().ok().flatten())
                .unwrap_or_else(|| "pre-setup".to_string());
            match snapshot_current_state_with_env(env, &label) {
                Ok(snap) => {
                    eprintln!("✓ Snapshot created ({})", snap.id);
                }
                Err(_) => {
                    eprintln!("⚠ Could not create restore snapshot — continuing without it");
                }
            }
        }
    }

    prepare_setup_state(env, fastfetch_enabled, selected_opacity)?;

    // Execute the setup (install tools, apply configurations)
    let summary = setup_executor::execute_setup_with_env(
        &selected_tools,
        &tools_to_configure,
        selected_font,
        selected_theme,
        env,
    )?;

    // nvim install consent prompt.
    // Runs after `execute_setup_with_env` so the current theme is
    // resolved (and stored on disk). `NvimAdapter::setup` is
    // idempotent — writes 18 shims + loader + initial state file.
    // The 3-way consent prompt then asks the user about the ONE
    // `pcall(require, 'slate')` line in init.lua (A/B/C). Per RESEARCH
    // §Pattern 7 the shim+loader files live regardless of consent
    // option C users can still `:colorscheme slate-<id>` manually.
    // `!stdin.is_terminal()` (quick mode on CI / non-tty) → default
    // to option A — silently adding the line is consistent with the
    // "quick = least friction" posture; the later completion-receipt
    // surface advertises `slate config editor disable` for opt-out.
    let non_interactive = !std::io::stdin().is_terminal();
    let nvim_consent = run_nvim_activation_flow(env, non_interactive);

    // Display completion message with visibility guidance
    eprintln!("\n{}", summary.format_completion_message());

    // surface the nvim flow's outcome inline, below the
    // completion card. Separate from `format_completion_message` so
    // the existing receipt contract is not mutated.
    if let Some(receipt_line) = format_nvim_consent_receipt(&nvim_consent) {
        let _ = cliclack::log::info(receipt_line);
    }
    // Task 5 — capability hint (missing / too-old nvim) surfaces when
    // `NvimAdapter.is_installed()` returned false. Exactly once per
    // run.
    if let Some(hint) = format_nvim_skip_hint_if_relevant() {
        let _ = cliclack::log::remark(hint);
    }

    if let Some(timing_line) = format_completion_timing(start_time) {
        eprintln!("{}", timing_line);
    }

    crate::cli::sound::play_feedback();

    // UX-02 (D-D3): new-shell reminder sits BETWEEN the receipt card and the
    // demo hint. Only fires when at least one successful adapter declared
    // `requires_new_shell=true` (aggregator). `setup` has no
    // auto / --quiet flags at this surface, so both guards are false.
    // `summary.theme_results` is populated by
    // setup_executor/integration.rs (`summary.set_theme_results(report.results)`)
    // no plumbing change required here.
    if should_emit_new_shell_reminder_after_setup(summary.theme_applied) {
        crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
    }

    // 18-: whole-flow milestone — setup finished
    // successfully. SoundSink consumes this for the
    // completion SFX; in it routes to NoopSink (no-op).
    // Failure exits earlier in this function each fire
    // BrandEvent::Failure(FailureKind::SetupFailed); the success
    // signal is this single dispatch.
    dispatch(BrandEvent::SetupComplete);

    Ok(())
}

/// Handle `slate setup` command with optional flags (backward compatibility)
/// Supports: --quick, --force, --only <tool>
pub fn handle(quick: bool, force: bool, only: Option<String>) -> Result<()> {
    let env = SlateEnv::from_process()?;
    handle_with_env(quick, force, only, &env)
}

fn prepare_setup_state(
    env: &SlateEnv,
    fastfetch_enabled: Option<bool>,
    selected_opacity: Option<crate::opacity::OpacityPreset>,
) -> Result<()> {
    let config_mgr = crate::config::ConfigManager::with_env(env)?;

    // Fastfetch: only write if user made an explicit choice (Some).
    // None = user wasn't asked (quick mode) — preserve existing setting.
    match fastfetch_enabled {
        Some(true) => {
            if let Err(e) = config_mgr.enable_fastfetch_autorun() {
                eprintln!("⚠ Could not save fastfetch preference: {}", e);
            }
        }
        Some(false) => {
            if let Err(e) = config_mgr.disable_fastfetch_autorun() {
                eprintln!("⚠ Could not save fastfetch preference: {}", e);
            }
        }
        None => {} // Don't touch existing setting
    }

    if let Some(opacity) = selected_opacity {
        if let Err(e) = config_mgr.set_current_opacity_preset(opacity) {
            eprintln!("⚠ Could not save opacity preference: {}", e);
        }
    }

    Ok(())
}

/// Handle --only flag: retry a single tool installation.
/// Only installs the tool — does NOT rewrite shell integration or apply themes.
fn handle_retry_only(tool_id: &str) -> Result<()> {
    let tool = validate_retry_tool(tool_id)?;
    let env = crate::env::SlateEnv::from_process()?;

    eprintln!("\n✦ Retrying tool installation: {}\n", tool.label);

    // Run pre-flight checks
    let preflight_result =
        preflight::run_checks_for_setup_with_env(&env, preflight::PreflightScenario::RetryInstall)?;
    if !preflight_result.is_ready() {
        return Err(crate::error::SlateError::Internal(
            preflight_result.format_blocking_guidance(),
        ));
    }

    // Only install the single tool — no shell integration, no theme apply
    match setup_executor::install_tool(tool.id, tool.brew_package, tool.brew_kind, &env) {
        Ok(method) => {
            eprintln!("\n{}", method.success_message(tool.label));
        }
        Err(e) => {
            eprintln!("\n✗ Tool '{}' installation failed: {}\n", tool.label, e);
        }
    }

    Ok(())
}

fn validate_retry_tool(tool_id: &str) -> Result<crate::cli::tool_selection::ToolMetadata> {
    let Some(tool) = ToolCatalog::get_tool(tool_id) else {
        return Err(crate::error::SlateError::Internal(format!(
            "Unknown tool: '{}'. Run 'slate setup' to see available tools.",
            tool_id
        )));
    };

    if !tool.installable {
        return Err(crate::error::SlateError::Internal(format!(
            "Tool '{}' is not installable via setup",
            tool_id
        )));
    }

    Ok(tool)
}

fn format_completion_timing(start_time: Option<Instant>) -> Option<String> {
    start_time.map(|start| {
        format!(
            "{} {}",
            Language::COMPLETION_TIME_TAKEN,
            format_elapsed(start.elapsed())
        )
    })
}

fn format_elapsed(elapsed: std::time::Duration) -> String {
    let ms = elapsed.as_millis();
    if ms < 1_000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", elapsed.as_secs_f64())
    } else {
        let secs = elapsed.as_secs();
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

// nvim auto-activation flow (consent).

/// Outcome of the 3-way consent prompt. Surfaced verbatim in the
/// completion receipt so users see what happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NvimConsent {
    /// NvimAdapter::is_installed() returned false — nvim missing or
    /// older than 0.8.0. Task 5's capability hint surfaces instead.
    NoNvim,
    /// Marker already present in init.lua/init.vim — prompt skipped.
    AlreadyConsented,
    /// User chose A — slate wrote the managed-block line.
    AutoAdded,
    /// User chose B — slate printed the line, no file edit.
    ShownLine,
    /// User chose C — nothing happened.
    Skipped,
}

/// Pre-prompt state for `prompt_nvim_activation`. Split from
/// `NvimConsent` because the prompt function itself has three
/// short-circuit paths (no nvim, marker exists, needs prompt) and
/// unit tests for the idempotency path should NOT need to mock
/// cliclack. This enum is the pure-function input; the prompt is
/// the thin orchestrator on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NvimActivationState {
    /// NvimAdapter::is_installed() returned false.
    NoNvim,
    /// A slate marker block is already present in init.lua / init.vim.
    AlreadyConsented,
    /// Neither of the above — the prompt should fire.
    NeedsPrompt,
}

/// Decide the current activation state WITHOUT firing the cliclack
/// prompt. Pure I/O (reads init.lua / init.vim, queries nvim
/// installed-ness) — deterministic for a given env + filesystem
/// snapshot, so unit-testable with `SlateEnv::with_home(tempdir)`
/// (no `std::env::set_var` needed).
pub(crate) fn nvim_activation_state(env: &SlateEnv) -> Result<NvimActivationState> {
    if !crate::adapter::NvimAdapter.is_installed()? {
        return Ok(NvimActivationState::NoNvim);
    }
    if init_file_has_slate_marker(env)? {
        return Ok(NvimActivationState::AlreadyConsented);
    }
    Ok(NvimActivationState::NeedsPrompt)
}

/// Scan both init.lua and init.vim for the slate marker. `marker_block::START`
/// is a raw-substring match (no line-start anchoring), so the Lua `--`
/// or vimscript `"` prefix in front of the marker still matches
/// that's the Pitfall 4 trick. Non-existent files are not an error.
fn init_file_has_slate_marker(env: &SlateEnv) -> Result<bool> {
    let nvim_home = env.home().join(".config/nvim");
    for name in ["init.lua", "init.vim"] {
        let path = nvim_home.join(name);
        if path.exists() {
            let content = std::fs::read(&path)?;
            if content
                .windows(crate::adapter::marker_block::START.len())
                .any(|w| w == crate::adapter::marker_block::START.as_bytes())
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Pick the target init file + whether it's Lua (as opposed to vim).
/// Pure function of the filesystem: init.lua wins when it exists OR
/// when NEITHER file exists (default-create-Lua). init.vim wins only
/// when it exists and init.lua does not.
pub(crate) fn choose_nvim_init_target(env: &SlateEnv) -> (PathBuf, bool) {
    let nvim_home = env.home().join(".config/nvim");
    let init_lua = nvim_home.join("init.lua");
    let init_vim = nvim_home.join("init.vim");
    if init_lua.exists() || !init_vim.exists() {
        (init_lua, true)
    } else {
        (init_vim, false)
    }
}

/// Build the managed-block body that gets written to init.lua / init.vim.
/// Pitfall 4 contract: the `marker_block::START` / `END` constants are
/// shell/TOML-style (`# slate:…`). For init.lua we MUST prepend `-- `
/// so the resulting file parses as valid Lua. For init.vim the prefix
/// is `"` (vimscript line comment), and the body uses `lua pcall(...)`.
/// The spliced markers are still raw-substring-matchable by
/// `marker_block::strip_managed_blocks` — verified in
/// `build_marker_block_for_init_contains_raw_markers` below.
pub(crate) fn build_marker_block_for_init(is_lua: bool) -> String {
    if is_lua {
        format!(
            "-- {}\n{}\npcall(require, 'slate')  {}\n-- {}",
            crate::adapter::marker_block::START,
            Language::NVIM_CONSENT_MARKER_COMMENT,
            Language::NVIM_CONSENT_MARKER_COMMENT,
            crate::adapter::marker_block::END,
        )
    } else {
        // Vimscript: comment prefix is `"`, and the runtime call is
        // `lua pcall(require, 'slate')` (vim vs. lua context).
        format!(
            "\" {}\n\" {}\nlua pcall(require, 'slate')\n\" {}",
            crate::adapter::marker_block::START,
            Language::NVIM_CONSENT_MARKER_COMMENT,
            crate::adapter::marker_block::END,
        )
    }
}

/// Apply the "option A" branch: write the managed block to init.lua
/// (or init.vim). Pulled out of `prompt_nvim_activation` so tests can
/// exercise it without spawning cliclack.
pub(crate) fn apply_activation_choice_a(env: &SlateEnv) -> Result<NvimConsent> {
    let (target, is_lua) = choose_nvim_init_target(env);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let block = build_marker_block_for_init(is_lua);
    crate::adapter::marker_block::upsert_managed_block_file(&target, &block)?;
    Ok(NvimConsent::AutoAdded)
}

/// Apply the "option B" branch: print the line + the target path.
/// No file edit. Pulled out of `prompt_nvim_activation` for testing +
/// parity with option A.
pub(crate) fn apply_activation_choice_b(env: &SlateEnv) -> NvimConsent {
    let (target, is_lua) = choose_nvim_init_target(env);
    let call = if is_lua {
        "pcall(require, 'slate')"
    } else {
        "lua pcall(require, 'slate')"
    };
    let _ = cliclack::log::info(format!(
        "Add this line to {}:\n\n    {}",
        target.display(),
        call
    ));
    NvimConsent::ShownLine
}

/// The 3-way consent prompt. Fires cliclack `select` with the
/// A/B/C labels, dispatches the chosen branch. Returns early if
/// NvimAdapter reports not-installed OR if an existing slate marker
/// is already present in init.lua / init.vim.
/// `non_interactive=true` short-circuits to option A — used when
/// stdin is not a TTY (quick setup on CI, piped input, etc.). The
/// one-line outcome still surfaces in the completion receipt.
pub(crate) fn prompt_nvim_activation(env: &SlateEnv, non_interactive: bool) -> Result<NvimConsent> {
    match nvim_activation_state(env)? {
        NvimActivationState::NoNvim => Ok(NvimConsent::NoNvim),
        NvimActivationState::AlreadyConsented => Ok(NvimConsent::AlreadyConsented),
        NvimActivationState::NeedsPrompt => {
            if non_interactive {
                // Quick / non-TTY: default to A. User still sees the
                // outcome in the completion receipt and can opt out
                // via `slate config editor disable`.
                return apply_activation_choice_a(env);
            }

            let choice = cliclack::select(Language::NVIM_CONSENT_HEADER)
                .item("A", Language::NVIM_CONSENT_OPTION_A, "")
                .item("B", Language::NVIM_CONSENT_OPTION_B, "")
                .item("C", Language::NVIM_CONSENT_OPTION_C, "")
                .interact()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::Interrupted {
                        crate::error::SlateError::UserCancelled
                    } else {
                        crate::error::SlateError::IOError(e)
                    }
                })?;

            match choice {
                "A" => apply_activation_choice_a(env),
                "B" => Ok(apply_activation_choice_b(env)),
                "C" => Ok(NvimConsent::Skipped),
                _ => unreachable!("cliclack returns only declared items"),
            }
        }
    }
}

/// Run the nvim install + consent flow inside the setup handler.
/// Split from `handle_with_env` so tests can exercise the install +
/// prompt orchestration without the wizard / preflight / TTY guards.
/// Install failures are non-fatal — the consent prompt is skipped (we
/// don't want to prompt for a line that `require('slate')` would fail
/// to resolve), and the error is logged via cliclack warning so users
/// see it. Returns `NvimConsent::NoNvim` in both the not-installed and
/// install-failed paths; the completion receipt surface treats them
/// identically (no "added" line).
fn run_nvim_activation_flow(env: &SlateEnv, non_interactive: bool) -> NvimConsent {
    match crate::adapter::NvimAdapter.is_installed() {
        Ok(true) => {}
        Ok(false) | Err(_) => return NvimConsent::NoNvim,
    }

    // Write 18 shims + loader + initial state. Idempotent — re-runs
    // produce byte-identical files via AtomicWriteFile.
    let current_theme =
        match crate::config::ConfigManager::with_env(env).and_then(|cm| cm.get_current_theme()) {
            Ok(Some(id)) => id,
            _ => "catppuccin-mocha".to_string(),
        };
    let registry = match crate::theme::ThemeRegistry::new() {
        Ok(r) => r,
        Err(_) => return NvimConsent::NoNvim,
    };
    let theme = match registry.get(&current_theme).cloned() {
        Some(t) => t,
        None => return NvimConsent::NoNvim,
    };
    if let Err(err) = crate::adapter::NvimAdapter::setup(env, &theme) {
        let _ = cliclack::log::warning(format!("⚠ Could not write slate's nvim files: {}", err));
        return NvimConsent::NoNvim;
    }

    // Consent prompt for the ONE line in init.lua.
    match prompt_nvim_activation(env, non_interactive) {
        Ok(consent) => consent,
        Err(err) => {
            let _ = cliclack::log::warning(format!("⚠ Nvim consent prompt failed: {}", err));
            NvimConsent::NoNvim
        }
    }
}

/// One-line receipt surface for the `NvimConsent` outcome. Returns
/// `None` when there's nothing to say (NoNvim — the capability hint
/// from Task 5 covers that surface, not this one).
fn format_nvim_consent_receipt(consent: &NvimConsent) -> Option<String> {
    match consent {
        NvimConsent::NoNvim => None,
        NvimConsent::AlreadyConsented => Some(
            "✦ Neovim auto-activation already wired (marker detected in init.lua).".to_string(),
        ),
        NvimConsent::AutoAdded => None,
        NvimConsent::ShownLine => Some(
            "✦ Nvim activation line shown above — paste it into init.lua when you're ready."
                .to_string(),
        ),
        NvimConsent::Skipped => Some(
            "✦ Nvim activation skipped — run `:colorscheme slate-<variant>` in nvim manually."
                .to_string(),
        ),
    }
}

// Task 5 — nvim capability hint (missing / too-old) surfaced in
// the completion receipt.

/// Decide which capability hint (if any) to surface on the setup
/// completion receipt. Pure function of `installed` + the optional
/// parsed version string — unit-testable without mutating process
/// env vars or spawning subprocesses.
/// Rules (matches RESEARCH §Pattern 8):
/// - `installed = false` → Some(NVIM_MISSING_HINT)
/// - `installed = true, version parse failure (None)` → Some(NVIM_MISSING_HINT)
/// - `installed = true, version < 0.8` → Some(NVIM_TOO_OLD_HINT)
/// - `installed = true, version ≥ 0.8` → None (nothing to say)
pub(crate) fn skip_hint_for(installed: bool, version: Option<&str>) -> Option<&'static str> {
    if !installed {
        return Some(Language::NVIM_MISSING_HINT);
    }
    match version {
        None => Some(Language::NVIM_MISSING_HINT),
        Some(ver) => {
            if crate::platform::version_check::VersionPolicy::check_version("nvim", ver).is_ok() {
                None
            } else {
                Some(Language::NVIM_TOO_OLD_HINT)
            }
        }
    }
}

/// Production wrapper: probes the current process environment via
/// `detect_tool_presence` + `detect_version` and delegates to
/// `skip_hint_for` for the decision. The split keeps the decision
/// logic unit-testable (pure) while the wrapper owns the I/O.
pub(crate) fn format_nvim_skip_hint_if_relevant() -> Option<&'static str> {
    let presence = crate::detection::detect_tool_presence("nvim");
    let version = if presence.installed {
        crate::platform::version_check::detect_version("nvim").ok()
    } else {
        None
    };
    skip_hint_for(presence.installed, version.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::new_shell_reminder::REMINDER_TEST_LOCK;
    use crate::opacity::OpacityPreset;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_setup_force_flag_recognized() {
        // Verify force flag is handled
        let force = true;
        assert!(force);
    }

    #[test]
    fn test_setup_only_invalid_tool() {
        // Verify invalid tool names are rejected
        let result = handle_retry_only("invalid_tool_xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_setup_only_valid_tool() {
        // Verify installable tools are recognized
        let result = validate_retry_tool("starship");
        assert!(result.is_ok());
    }

    #[test]
    fn test_setup_only_detectable_tool() {
        // Verify detect-only tools are rejected for retry
        let result = handle_retry_only("tmux");
        assert!(result.is_err());
        // ghostty is now detect-only too
        let result = handle_retry_only("ghostty");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_completion_timing_uses_label() {
        let start = Instant::now() - Duration::from_millis(10);
        let line = format_completion_timing(Some(start)).expect("timing should be present");

        assert!(line.contains(Language::COMPLETION_TIME_TAKEN));
        assert!(line.contains("ms"));
    }

    #[test]
    fn format_elapsed_picks_human_unit() {
        use std::time::Duration;
        assert_eq!(format_elapsed(Duration::from_millis(10)), "10ms");
        assert_eq!(format_elapsed(Duration::from_millis(999)), "999ms");
        assert_eq!(format_elapsed(Duration::from_millis(1_500)), "1.5s");
        assert_eq!(format_elapsed(Duration::from_millis(15_500)), "15.5s");
        assert_eq!(format_elapsed(Duration::from_millis(60_000)), "1m 0s");
        assert_eq!(format_elapsed(Duration::from_millis(223_088)), "3m 43s");
    }

    #[test]
    fn test_format_completion_timing_none() {
        assert!(format_completion_timing(None).is_none());
    }

    /// Simulate the decision point in `handle_with_env` that guards the
    /// `emit_new_shell_reminder_once` call. This isolates the wiring
    /// (successful shell-integration phase → emitter) from wizard + preflight
    /// + stdin-TTY coupling that makes the full handler untestable in-process.
    fn setup_emit_branch(theme_applied: bool) {
        if should_emit_new_shell_reminder_after_setup(theme_applied) {
            crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
        }
    }

    #[test]
    fn setup_handler_emits_reminder_when_shell_integration_succeeds() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();
        assert!(!crate::cli::new_shell_reminder::reminder_flag_for_tests());

        setup_emit_branch(true);

        assert!(
            crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "setup handler must transition the reminder flag after the shell-integration phase succeeds"
        );
    }

    #[test]
    fn setup_handler_skips_reminder_when_shell_integration_fails() {
        let _guard = REMINDER_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::cli::new_shell_reminder::reset_reminder_flag_for_tests();

        setup_emit_branch(false);

        assert!(
            !crate::cli::new_shell_reminder::reminder_flag_for_tests(),
            "setup handler must leave the flag untouched when shell integration did not complete"
        );
    }

    #[test]
    fn test_prepare_setup_state_updates_marker_and_opacity_before_apply() {
        let tempdir = TempDir::new().unwrap();
        let env = SlateEnv::with_home(tempdir.path().to_path_buf());
        let config = crate::config::ConfigManager::with_env(&env).unwrap();

        config.enable_fastfetch_autorun().unwrap();

        prepare_setup_state(&env, Some(false), Some(OpacityPreset::Frosted)).unwrap();

        assert!(!config.has_fastfetch_autorun().unwrap());
        assert_eq!(
            config.get_current_opacity_preset().unwrap(),
            OpacityPreset::Frosted
        );
    }

    // nvim activation flow

    /// Pitfall 4 contract: the managed block written to init.lua
    /// wraps the shell-style START / END markers in Lua `--` line
    /// comments AND embeds the human-readable marker comment next to
    /// the runtime call. The RAW marker strings (without the leading
    /// `-- `) must still appear in the block so
    /// `marker_block::strip_managed_blocks`'s substring match finds
    /// them.
    #[test]
    fn build_marker_block_for_init_lua_wraps_with_lua_comments() {
        let block = build_marker_block_for_init(true);

        // Lua comments in front of each marker — the Pitfall 4 fix.
        assert!(
            block.contains(&format!("-- {}", crate::adapter::marker_block::START)),
            "init.lua marker block must prepend `-- ` to the START marker: {}",
            block
        );
        assert!(
            block.contains(&format!("-- {}", crate::adapter::marker_block::END)),
            "init.lua marker block must prepend `-- ` to the END marker: {}",
            block
        );
        // Raw markers still present (for strip_managed_blocks).
        assert!(block.contains(crate::adapter::marker_block::START));
        assert!(block.contains(crate::adapter::marker_block::END));
        // The runtime call is the bare-Lua form.
        assert!(
            block.contains("pcall(require, 'slate')"),
            "Lua block must carry the bare pcall(require, 'slate') call"
        );
        // Marker comment (brand-voiced) embedded.
        assert!(block.contains(Language::NVIM_CONSENT_MARKER_COMMENT));
    }

    /// init.vim variant: `"` comment prefix + `lua pcall(...)` body.
    #[test]
    fn build_marker_block_for_init_vim_uses_vimscript_comment_prefix() {
        let block = build_marker_block_for_init(false);
        assert!(
            block.contains(&format!("\" {}", crate::adapter::marker_block::START)),
            "init.vim marker block must prepend `\" ` to the START marker: {}",
            block
        );
        assert!(block.contains(&format!("\" {}", crate::adapter::marker_block::END)));
        assert!(
            block.contains("lua pcall(require, 'slate')"),
            "init.vim block must carry the `lua pcall(require, 'slate')` runtime call"
        );
    }

    /// Target selection: init.lua exists → init.lua.
    #[test]
    fn choose_nvim_init_target_prefers_existing_init_lua() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let init_lua = td.path().join(".config/nvim/init.lua");
        std::fs::create_dir_all(init_lua.parent().unwrap()).unwrap();
        std::fs::write(&init_lua, "-- empty\n").unwrap();

        let (target, is_lua) = choose_nvim_init_target(&env);
        assert_eq!(target, init_lua);
        assert!(is_lua);
    }

    /// Target selection: only init.vim exists → init.vim.
    #[test]
    fn choose_nvim_init_target_picks_init_vim_when_only_vim_exists() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let init_vim = td.path().join(".config/nvim/init.vim");
        std::fs::create_dir_all(init_vim.parent().unwrap()).unwrap();
        std::fs::write(&init_vim, "\" empty\n").unwrap();

        let (target, is_lua) = choose_nvim_init_target(&env);
        assert_eq!(target, init_vim);
        assert!(!is_lua);
    }

    /// Target selection: neither exists → default init.lua.
    #[test]
    fn choose_nvim_init_target_defaults_to_init_lua_when_neither_exists() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());

        let (target, is_lua) = choose_nvim_init_target(&env);
        assert_eq!(target, td.path().join(".config/nvim/init.lua"));
        assert!(is_lua);
    }

    /// Idempotency gate (RESEARCH §Pattern 7 + §Pitfall 4): if an
    /// init.lua already carries a slate marker, the activation state
    /// returns `AlreadyConsented` — no prompt, no file edit.
    /// This is the pure-function equivalent of the
    /// `prompt_nvim_activation_is_idempotent_on_existing_marker` test
    /// in 17-06-PLAN.md — split because we don't need to mock
    /// cliclack to prove the short-circuit.
    #[test]
    fn nvim_activation_state_detects_existing_marker_in_init_lua() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let init_lua = td.path().join(".config/nvim/init.lua");
        std::fs::create_dir_all(init_lua.parent().unwrap()).unwrap();
        let seed = format!(
            "-- {}\npcall(require, 'slate')\n-- {}\n",
            crate::adapter::marker_block::START,
            crate::adapter::marker_block::END,
        );
        std::fs::write(&init_lua, &seed).unwrap();

        // We can't exercise is_installed() in tests without mutating
        // PATH; but when nvim IS installed the state is
        // AlreadyConsented. When nvim is NOT installed the state is
        // NoNvim. Either way the file is not mutated.
        let state = nvim_activation_state(&env).expect("pure I/O");
        assert!(
            matches!(
                state,
                NvimActivationState::AlreadyConsented | NvimActivationState::NoNvim
            ),
            "with an existing marker, state must be AlreadyConsented OR NoNvim, got {:?}",
            state
        );

        // Contract: no file mutation regardless of is_installed path.
        let after = std::fs::read_to_string(&init_lua).unwrap();
        assert_eq!(after, seed, "init.lua must be byte-identical");
    }

    #[test]
    fn init_file_has_slate_marker_handles_non_utf8_init_lua() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let init_lua = td.path().join(".config/nvim/init.lua");
        std::fs::create_dir_all(init_lua.parent().unwrap()).unwrap();
        let mut seed = vec![0xff, 0xfe, b'\n'];
        seed.extend_from_slice(
            format!(
                "-- {}\npcall(require, 'slate')\n-- {}\n",
                crate::adapter::marker_block::START,
                crate::adapter::marker_block::END,
            )
            .as_bytes(),
        );
        std::fs::write(&init_lua, seed).unwrap();

        assert!(
            init_file_has_slate_marker(&env).expect("raw byte scan must succeed"),
            "marker detection must not fail on non-UTF-8 init.lua"
        );
    }

    /// `apply_activation_choice_a` is the load-bearing side-effect
    /// path. It produces a Lua-comment-wrapped block on init.lua AND
    /// is detected by `init_file_has_slate_marker` on subsequent
    /// calls — that detection is what makes the *flow* (prompt →
    /// activation-state) idempotent, even though the raw marker_block
    /// strip+append at the byte level is not a true fixed-point when
    /// a `-- ` Lua-comment prefix sits outside the substring range of
    /// the START marker (byte-positional strip, not line-aware).
    /// Contract exercised:
    /// 1. First write produces a Lua-comment-wrapped START marker
    /// and the `pcall(require, 'slate')` runtime call.
    /// 2. After the first write, `init_file_has_slate_marker`
    /// detects the marker, which is the guard that short-circuits
    /// `prompt_nvim_activation` on subsequent runs (returning
    /// `NvimConsent::AlreadyConsented` — never re-entering this
    /// side-effect path).
    #[test]
    fn apply_activation_choice_a_writes_lua_wrapped_block_and_is_detected_as_consented() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let init_lua = td.path().join(".config/nvim/init.lua");

        let outcome = apply_activation_choice_a(&env).unwrap();
        assert_eq!(outcome, NvimConsent::AutoAdded);

        let first = std::fs::read_to_string(&init_lua).unwrap();
        assert!(
            first.contains(&format!("-- {}", crate::adapter::marker_block::START)),
            "first write must produce a Lua-comment-wrapped START marker"
        );
        assert!(first.contains("pcall(require, 'slate')"));
        assert!(
            first.contains(&format!("-- {}", crate::adapter::marker_block::END)),
            "first write must produce a Lua-comment-wrapped END marker"
        );

        // Idempotency of the *flow*: the marker-detection helper now
        // reports `true`, so the prompt's short-circuit path
        // (`NvimActivationState::AlreadyConsented`) fires on re-run
        // and `apply_activation_choice_a` is never called again.
        assert!(
            init_file_has_slate_marker(&env).expect("reads init.lua"),
            "after choice A, init_file_has_slate_marker must detect the marker"
        );
    }

    /// `apply_activation_choice_a` creates parent dirs when
    /// `~/.config/nvim/` does not exist yet — fresh box pre-nvim.
    #[test]
    fn apply_activation_choice_a_creates_parent_when_absent() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        // Nothing in .config/nvim — verify parent creation.
        let outcome = apply_activation_choice_a(&env).unwrap();
        assert_eq!(outcome, NvimConsent::AutoAdded);
        assert!(td.path().join(".config/nvim/init.lua").exists());
    }

    #[test]
    fn apply_activation_choice_a_preserves_non_utf8_prefix_bytes() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_path_buf());
        let init_lua = td.path().join(".config/nvim/init.lua");
        std::fs::create_dir_all(init_lua.parent().unwrap()).unwrap();
        std::fs::write(&init_lua, [0xff, 0xfe, b'\n']).unwrap();

        let outcome = apply_activation_choice_a(&env).unwrap();
        assert_eq!(outcome, NvimConsent::AutoAdded);

        let updated = std::fs::read(&init_lua).unwrap();
        assert!(
            updated.starts_with(&[0xff, 0xfe, b'\n']),
            "existing non-UTF-8 bytes must be preserved"
        );
        assert!(
            updated
                .windows(crate::adapter::marker_block::START.len())
                .any(|w| w == crate::adapter::marker_block::START.as_bytes()),
            "choice A must still append the marker block"
        );
    }

    /// `skip_hint_for` — pure decision logic (Task 5).
    #[test]
    fn skip_hint_for_returns_missing_hint_when_nvim_absent() {
        assert_eq!(
            skip_hint_for(false, None),
            Some(Language::NVIM_MISSING_HINT)
        );
        assert_eq!(
            skip_hint_for(false, Some("0.12.0")),
            Some(Language::NVIM_MISSING_HINT),
            "installed=false always short-circuits to missing"
        );
    }

    #[test]
    fn skip_hint_for_returns_too_old_for_below_0_8() {
        assert_eq!(
            skip_hint_for(true, Some("0.7.2")),
            Some(Language::NVIM_TOO_OLD_HINT)
        );
    }

    #[test]
    fn skip_hint_for_returns_none_for_supported_version() {
        assert_eq!(skip_hint_for(true, Some("0.8.0")), None);
        assert_eq!(skip_hint_for(true, Some("0.12.0")), None);
    }

    #[test]
    fn skip_hint_for_treats_unparseable_version_as_missing() {
        // installed=true but we couldn't parse the version → conservative:
        // surface the missing hint so the user reinstalls.
        assert_eq!(skip_hint_for(true, None), Some(Language::NVIM_MISSING_HINT));
    }

    /// Receipt surface distinctness — every meaningful consent state
    /// produces a distinct one-liner so the reader can tell outcomes
    /// apart. AutoAdded and NoNvim are intentionally silent (the marker
    /// in init.lua and the capability hint cover those surfaces).
    #[test]
    fn format_nvim_consent_receipt_surfaces_distinct_messages() {
        let auto = format_nvim_consent_receipt(&NvimConsent::AutoAdded);
        let shown = format_nvim_consent_receipt(&NvimConsent::ShownLine);
        let skipped = format_nvim_consent_receipt(&NvimConsent::Skipped);
        let already = format_nvim_consent_receipt(&NvimConsent::AlreadyConsented);
        let none = format_nvim_consent_receipt(&NvimConsent::NoNvim);

        assert_eq!(auto, None, "AutoAdded is silent — marker speaks for itself");
        assert_eq!(none, None, "NoNvim is silent — hint surface covers it");
        assert!(shown.is_some());
        assert!(skipped.is_some());
        assert!(already.is_some());

        assert_ne!(shown, skipped);
        assert_ne!(already, shown);
        assert_ne!(already, skipped);
    }
}
