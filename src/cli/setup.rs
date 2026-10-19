use crate::brand::events::{dispatch, BrandEvent, FailureKind};
use crate::brand::language::Language;
use crate::cli::preflight;
use crate::cli::setup_executor;
use crate::cli::tool_selection::ToolCatalog;
use crate::cli::wizard_core::Wizard;
use crate::cli::wizard_support::wording as tr;
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

/// Validate setup before the executable initializes write locks or sounds.
pub fn validate_entry(quick: bool, only: Option<&str>) -> Result<()> {
    if let Some(tool_id) = only {
        validate_retry_tool(tool_id)?;
    }
    // Reject invisible guided setup before acquiring a lock or probing files.
    // Explicit quick/retry workflows keep their existing consent rules.
    if only.is_none()
        && !quick
        && (!std::io::stdin().is_terminal() || !std::io::stdout().is_terminal())
    {
        return Err(crate::error::SlateError::Internal(
            "Non-interactive setup requires --quick for explicit consent.".to_string(),
        ));
    }
    Ok(())
}

/// Handle `slate setup` command with injected SlateEnv (preferred for testability)
pub fn handle_with_env(
    quick: bool,
    force: bool,
    only: Option<String>,
    env: &SlateEnv,
) -> Result<()> {
    validate_entry(quick, only.as_deref())?;
    let _write_guard = crate::config::ConfigWriteGuard::acquire(env)?;
    // If --only flag is set, handle retry flow
    if let Some(tool_id) = only {
        return handle_retry_only(&tool_id, env);
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
    let mut wizard = Wizard::with_env(env)?;
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

    // Resolve the complete request before preferences, snapshots or installers.
    // Execution consumes this plan instead of resolving the saved theme again.
    let reviewed_installs = wizard.confirmed_install_plan()?;
    let plan = match setup_executor::prepare_setup_with_env(
        &selected_tools,
        &tools_to_configure,
        selected_font,
        selected_theme,
        env,
    )
    .and_then(|plan| {
        plan.with_reviewed_installs(
            reviewed_installs,
            crate::platform::packages::InstallContext::detect(),
        )
    }) {
        Ok(plan) => plan,
        Err(error) => {
            dispatch(BrandEvent::Failure(FailureKind::SetupFailed));
            return Err(error);
        }
    };

    // A failed safety snapshot must stop setup before installing or editing tools.
    let snapshot = snapshot_before_setup(env)?;
    eprintln!(
        "✓ {} ({})",
        tr("已创建恢复点", "Snapshot created"),
        snapshot.id
    );

    if let Err(error) = prepare_setup_state(env, fastfetch_enabled, selected_opacity) {
        return fail_setup_after_snapshot(&snapshot.id, error.to_string(), dispatch);
    }

    // Execute the setup (install tools, apply configurations)
    let mut summary = match setup_executor::execute_prepared_setup(plan) {
        Ok(summary) => summary,
        Err(error) => return fail_setup_after_snapshot(&snapshot.id, error.to_string(), dispatch),
    };

    // nvim install consent prompt.
    // Runs after `execute_prepared_setup` so the current theme is
    // resolved (and stored on disk). `NvimAdapter::setup` is
    // idempotent — writes 18 shims + loader + initial state file.
    // The 3-way consent prompt then asks the user about the ONE
    // `pcall(require, 'slate')` line in init.lua (A/B/C).     // §Pattern 7 the shim+loader files live regardless of consent
    // option C users can still `:colorscheme slate-<id>` manually.
    // `!stdin.is_terminal()` (quick mode on CI / non-tty) → default
    // to option A — silently adding the line is consistent with the
    // "quick = least friction" posture; the later completion-receipt
    // surface advertises `slate config set editor disable` for opt-out.
    let non_interactive = !std::io::stdin().is_terminal();
    let nvim_consent = nvim_after_theme(&mut summary, || {
        run_nvim_activation_flow(env, non_interactive)
    });
    summary.refresh_outcome();

    // Display completion message with visibility guidance
    eprintln!(
        "\n{}",
        summary.format_completion_message_for_terminal(wizard.terminal_profile())
    );

    // surface the nvim flow's outcome inline, below the
    // completion card. Separate from `format_completion_message` so
    // the existing receipt contract is not mutated.
    if let Some(receipt_line) = nvim_consent.as_ref().and_then(format_nvim_consent_receipt) {
        let _ = cliclack::log::info(receipt_line);
    }
    if let Some(timing_line) = format_completion_timing(start_time) {
        eprintln!("{}", timing_line);
    }

    // new-shell reminder sits BETWEEN the receipt card and the
    // demo hint. Only fires when at least one successful adapter declared
    // `requires_new_shell=true` (aggregator). `setup` has no
    // auto / --quiet flags at this surface, so both guards are false.
    // `summary.theme_results` is populated by
    // setup_executor/integration.rs (`summary.set_theme_results(report.results)`)
    // no plumbing change required here.
    if should_emit_new_shell_reminder_after_setup(summary.theme_applied) {
        crate::cli::new_shell_reminder::emit_new_shell_reminder_once(false, false);
    }

    finish_setup_with(&summary, &snapshot.id, dispatch)?;
    preflight_result.acknowledge_after_setup(env);
    Ok(())
}

fn nvim_after_theme(
    summary: &mut crate::cli::failure_handler::ExecutionSummary,
    activate: impl FnOnce() -> Result<NvimConsent>,
) -> Option<NvimConsent> {
    if !summary.theme_applied {
        summary.add_notice(tr(
            "主题或 Shell 设置未完成，已跳过 Neovim 启用。",
            "Neovim activation was skipped because theme/shell setup did not finish.",
        ));
        return None;
    }
    match activate() {
        Ok(consent) => Some(consent),
        Err(error) => {
            summary.add_issue(format!(
                "{}: {error}",
                tr("Neovim 启用未完成", "Neovim activation did not finish")
            ));
            None
        }
    }
}

fn finish_setup_with(
    summary: &crate::cli::failure_handler::ExecutionSummary,
    restore_point_id: &str,
    emit: impl FnOnce(BrandEvent),
) -> Result<()> {
    if summary.is_successful() {
        emit(BrandEvent::SetupComplete);
        Ok(())
    } else {
        fail_setup_after_snapshot(restore_point_id, summary.failure_summary(), emit)
    }
}

fn fail_setup_after_snapshot(
    restore_point_id: &str,
    reason: String,
    emit: impl FnOnce(BrandEvent),
) -> Result<()> {
    emit(BrandEvent::Failure(FailureKind::SetupFailed));
    Err(crate::error::SlateError::SetupIncomplete {
        reason,
        restore_point_id: restore_point_id.to_owned(),
    })
}

/// Handle `slate setup` command with optional flags (backward compatibility)
/// Supports: --quick, --force, --only <tool>
pub fn handle(quick: bool, force: bool, only: Option<String>) -> Result<()> {
    let env = SlateEnv::from_process()?;
    handle_with_env(quick, force, only, &env)
}

fn snapshot_before_setup(env: &SlateEnv) -> Result<crate::config::RestorePoint> {
    let has_baseline = crate::config::list_restore_points_with_env(env)?
        .iter()
        .any(|point| point.is_baseline);
    if !has_baseline {
        return crate::config::begin_restore_point_baseline_with_env(env);
    }
    let label = crate::config::ConfigManager::from_env_paths(env)
        .get_current_theme()?
        .unwrap_or_else(|| "pre-setup".into());
    crate::config::snapshot_current_state_with_env(env, &label)
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
            config_mgr.enable_fastfetch_autorun()?;
        }
        Some(false) => {
            config_mgr.disable_fastfetch_autorun()?;
        }
        None => {} // Don't touch existing setting
    }

    if let Some(opacity) = selected_opacity {
        config_mgr.set_current_opacity_preset(opacity)?;
    }

    Ok(())
}

/// Handle --only flag: retry a single tool installation.
/// Only installs the tool — does NOT rewrite shell integration or apply themes.
fn handle_retry_only(tool_id: &str, env: &SlateEnv) -> Result<()> {
    retry_only_with(
        tool_id,
        env,
        |_env| {
            let report = preflight::run_checks_for_retry(tool_id);
            if report.is_ready() {
                Ok(())
            } else {
                Err(crate::error::SlateError::Internal(
                    report.format_blocking_guidance(),
                ))
            }
        },
        |tool, env| {
            setup_executor::install_tool(tool.id, tool.brew_package, tool.brew_kind, env)
                .map(|method| method.success_message(tool.label))
        },
    )
}

/// Shared retry sequence; callbacks let tests exercise failures without package
/// managers, network probes or installers. Production always uses the real pair.
fn retry_only_with(
    tool_id: &str,
    env: &SlateEnv,
    preflight: impl FnOnce(&SlateEnv) -> Result<()>,
    install: impl FnOnce(&crate::cli::tool_selection::ToolMetadata, &SlateEnv) -> Result<String>,
) -> Result<()> {
    let tool = validate_retry_tool(tool_id)?;
    eprintln!(
        "\n✦ {}: {}\n",
        tr("重试安装", "Retrying tool installation"),
        tool.label
    );
    preflight(env)?;
    // Preserve the install error as a failure result (CLI exit 1), not a printed
    // failure followed by success. Only successful installs get a success line.
    let message = install(&tool, env)?;
    eprintln!("\n{}", crate::cli::file_output::terminal_text(&message));
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        eprintln!("{}", tr("本次仅安装工具，未应用配色或启用 Shell 集成。", "This retry only installed the tool; it did not apply colors or enable shell integration."));
    }
    Ok(())
}

/// Side-effect-free input validation, also used before CLI profile/lock/sound IO.
pub fn validate_retry_tool(tool_id: &str) -> Result<crate::cli::tool_selection::ToolMetadata> {
    let Some(tool) = ToolCatalog::get_tool(tool_id) else {
        return Err(crate::error::SlateError::Internal(format!(
            "Unknown tool: '{}'. Run 'slate setup' to see available tools.",
            tool_id.escape_default()
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
    /// No executable was found; retained for the completion hint.
    NoNvim,
    /// The checked executable is older than the supported minimum.
    TooOld,
    /// This profile has explicitly opted out of automatic setup activation.
    Disabled,
    /// Marker already present in init.lua/init.vim — prompt skipped.
    AlreadyConsented,
    /// User chose A — slate wrote the managed-block line.
    AutoAdded,
    /// User chose B — line shown, manual-only preference saved; init unchanged.
    ShownLine,
    /// User chose C — manual-only preference was saved.
    Skipped,
}

/// Pre-prompt state for `prompt_nvim_activation`. Split from
/// `NvimConsent` because the prompt function itself has three
/// states (disabled, marker exists, needs prompt) and
/// unit tests for the idempotency path should NOT need to mock
/// cliclack. This enum is the pure-function input; the prompt is
/// the thin orchestrator on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NvimActivationState {
    Disabled,
    /// A slate marker block is already present in init.lua / init.vim.
    AlreadyConsented,
    /// Neither of the above — the prompt should fire.
    NeedsPrompt,
}

/// Read consent/markers WITHOUT launching an editor or firing the prompt.
/// The activation flow checks availability once before setup; this later stage
/// must not repeat that native probe just to decide whether a hook is present.
pub(crate) fn nvim_activation_state(env: &SlateEnv) -> Result<NvimActivationState> {
    if !crate::config::ConfigManager::from_env_paths(env).is_editor_auto_activation_enabled()? {
        return Ok(NvimActivationState::Disabled);
    }
    if init_file_has_slate_marker(env)? {
        return Ok(NvimActivationState::AlreadyConsented);
    }
    Ok(NvimActivationState::NeedsPrompt)
}

/// Scan both init.lua and init.vim for the slate marker. `marker_block::START`
/// is a raw-substring match (no line-start anchoring), so the Lua `--`
/// or vimscript `"` prefix in front of the marker still matches
/// avoiding syntax errors. Non-existent files are not an error.
fn init_file_has_slate_marker(env: &SlateEnv) -> Result<bool> {
    use crate::config::file_read::{self, Links, MAX_TOOL_CONFIG_BYTES};
    let nvim_home = env.nvim_config_dir();
    for name in ["init.lua", "init.vim"] {
        let path = nvim_home.join(name);
        if let Some(source) =
            file_read::read(&path, MAX_TOOL_CONFIG_BYTES, Links::Follow).map_err(|error| {
                crate::error::SlateError::ConfigReadError(
                    path.display().to_string(),
                    error.to_string(),
                )
            })?
        {
            if source
                .bytes
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
    let target = env.nvim_init_path();
    let is_lua = target.extension().is_some_and(|ext| ext == "lua");
    (target, is_lua)
}

/// Build the managed-block body that gets written to init.lua / init.vim.
/// the `marker_block::START` / `END` constants are
/// shell/TOML-style (`# slate:…`). For init.lua we MUST prepend `-- `
/// so the resulting file parses as valid Lua. For init.vim the prefix
/// is `"` (vimscript line comment), and the body uses `lua pcall(...)`.
/// The marker editor recognizes these full comment lines, removing their
/// wrappers together with the block instead of leaving a comment prefix behind.
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
    if !crate::config::ConfigManager::from_env_paths(env).is_editor_auto_activation_enabled()? {
        return Ok(NvimConsent::Disabled);
    }
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
    let _ = cliclack::log::info(format_nvim_manual_instruction(&target, call));
    NvimConsent::ShownLine
}

fn format_nvim_manual_instruction(target: &std::path::Path, call: &str) -> String {
    format!(
        "{} {}:\n\n    {}",
        tr("将以下内容添加到", "Add this line to"),
        crate::cli::file_output::terminal_text(&target.to_string_lossy()),
        call
    )
}

/// The 3-way consent prompt. Fires cliclack `select` with the
/// A/B/C labels, dispatches the chosen branch. The caller must have checked
/// availability; this stage only checks the preference and existing markers.
/// `non_interactive=true` short-circuits to option A — used when
/// stdin is not a TTY (quick setup on CI, piped input, etc.). The
/// one-line outcome still surfaces in the completion receipt.
pub(crate) fn prompt_nvim_activation(env: &SlateEnv, non_interactive: bool) -> Result<NvimConsent> {
    match nvim_activation_state(env)? {
        NvimActivationState::Disabled => Ok(NvimConsent::Disabled),
        NvimActivationState::AlreadyConsented => Ok(NvimConsent::AlreadyConsented),
        NvimActivationState::NeedsPrompt => {
            if non_interactive {
                // Quick / non-TTY: default to A. User still sees the
                // outcome in the completion receipt and can opt out
                // via `slate config set editor disable`.
                return apply_activation_choice_a(env);
            }

            let choice =
                super::menu::select(tr("启用 Neovim 自动配色？", Language::NVIM_CONSENT_HEADER))
                    .item("A", tr("自动添加配置", Language::NVIM_CONSENT_OPTION_A), "")
                    .item(
                        "B",
                        tr("显示配置内容，手动添加", Language::NVIM_CONSENT_OPTION_B),
                        tr(
                            "以后保持手动启用",
                            "Remember manual activation for future setup",
                        ),
                    )
                    .item(
                        "C",
                        tr("跳过，手动切换配色", Language::NVIM_CONSENT_OPTION_C),
                        tr(
                            "以后保持手动启用",
                            "Remember manual activation for future setup",
                        ),
                    )
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
                "B" => remember_manual_nvim_activation(env, true),
                "C" => remember_manual_nvim_activation(env, false),
                _ => unreachable!("cliclack returns only declared items"),
            }
        }
    }
}

fn remember_manual_nvim_activation(env: &SlateEnv, show_line: bool) -> Result<NvimConsent> {
    crate::config::ConfigManager::from_env_paths(env).set_editor_auto_activation_enabled(false)?;
    Ok(if show_line {
        apply_activation_choice_b(env)
    } else {
        NvimConsent::Skipped
    })
}

/// Run the nvim install + consent flow inside the setup handler.
/// Split from `handle_with_env` so tests can exercise the install +
/// prompt orchestration without the wizard / preflight / TTY guards.
/// Missing/unsupported Neovim is an ordinary skip. Actual state-read, setup or
/// consent failures remain errors so the handler cannot declare full success.
fn run_nvim_activation_flow(env: &SlateEnv, non_interactive: bool) -> Result<NvimConsent> {
    run_nvim_activation_with_probe(env, non_interactive, || {
        crate::adapter::nvim::availability::detect(env)
    })
}

fn run_nvim_activation_with_probe(
    env: &SlateEnv,
    non_interactive: bool,
    probe: impl FnOnce() -> Result<crate::adapter::nvim::availability::NvimAvailability>,
) -> Result<NvimConsent> {
    use crate::adapter::nvim::availability::NvimAvailability;
    with_nvim_auto_activation(env, || match probe()? {
        NvimAvailability::Missing => Ok(NvimConsent::NoNvim),
        NvimAvailability::Unsupported => Ok(NvimConsent::TooOld),
        NvimAvailability::Ready => run_allowed_nvim_activation_flow(env, non_interactive),
    })
}

fn with_nvim_auto_activation(
    env: &SlateEnv,
    activate: impl FnOnce() -> Result<NvimConsent>,
) -> Result<NvimConsent> {
    if !crate::config::ConfigManager::from_env_paths(env).is_editor_auto_activation_enabled()? {
        return Ok(NvimConsent::Disabled);
    }
    activate()
}

fn run_allowed_nvim_activation_flow(env: &SlateEnv, non_interactive: bool) -> Result<NvimConsent> {
    // Write the shims + loader + initial state. Idempotent — re-runs
    // produce byte-identical files via AtomicWriteFile.
    let current_theme = crate::config::ConfigManager::from_env_paths(env)
        .get_current_theme()?
        .unwrap_or_else(|| crate::theme::DEFAULT_THEME_ID.into());
    let registry = crate::theme::ThemeRegistry::with_env(env)?;
    let theme = registry.get(&current_theme).ok_or_else(|| {
        crate::error::SlateError::InvalidThemeData(format!(
            "Cannot activate Neovim with unknown saved theme '{}'",
            current_theme.escape_default()
        ))
    })?;
    crate::adapter::NvimAdapter::setup(env, theme)?;

    // Consent prompt for the ONE line in init.lua.
    prompt_nvim_activation(env, non_interactive)
}

/// Render the retained result without probing the editor again.
fn format_nvim_consent_receipt(consent: &NvimConsent) -> Option<String> {
    format_nvim_consent_receipt_in(consent, crate::cli::ui_language::output_language())
}

fn format_nvim_consent_receipt_in(
    consent: &NvimConsent,
    language: crate::config::ui_language::UiLanguage,
) -> Option<String> {
    if language == crate::config::ui_language::UiLanguage::Chinese {
        return match consent {
            NvimConsent::NoNvim => Some("未检测到 Neovim，已跳过编辑器配色。需要 Neovim 0.8 或更新版本。".into()),
            NvimConsent::TooOld => Some("Neovim 低于 0.8，已跳过编辑器配色；升级后可重新设置。".into()),
            NvimConsent::Disabled => Some("Neovim 自动启用保持关闭；未更改手动加载配置或运行中的编辑器。重新允许：`slate config set editor enable`，然后运行 `slate setup`。".into()),
            NvimConsent::AlreadyConsented => Some("已检测到 Neovim 自动加载配置；未验证运行中的配色。".into()),
            NvimConsent::AutoAdded => None,
            NvimConsent::ShownLine => Some("请将上面的配置添加到指定文件。以后保持手动启用；重新允许自动配置：`slate config set editor enable`。".into()),
            NvimConsent::Skipped => Some("已记住跳过自动启用。可手动运行 `:colorscheme slate-<variant>`；重新允许自动配置：`slate config set editor enable`。".into()),
        };
    }
    match consent {
        NvimConsent::NoNvim => Some(Language::NVIM_MISSING_HINT.into()),
        NvimConsent::TooOld => Some(Language::NVIM_TOO_OLD_HINT.into()),
        NvimConsent::Disabled => Some(
            "Neovim automatic activation remains off for this profile. To allow it again: `slate config set editor enable`, then `slate setup`. Existing manual hooks and running editors are unchanged.".into(),
        ),
        NvimConsent::AlreadyConsented => Some(
            "✦ Neovim auto-activation already wired (marker detected in init.lua/init.vim).".to_string(),
        ),
        NvimConsent::AutoAdded => None,
        NvimConsent::ShownLine => Some(
            "✦ Nvim activation line shown above — paste it into the indicated file when ready. Future setup keeps activation manual until `slate config set editor enable`."
                .to_string(),
        ),
        NvimConsent::Skipped => Some(
            "✦ Nvim activation skipped and remembered — use `:colorscheme slate-<variant>` manually. To allow setup activation again: `slate config set editor enable`."
                .to_string(),
        ),
    }
}

#[cfg(test)]
#[path = "setup/tests.rs"]
mod tests;
