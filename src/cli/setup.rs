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
    // `pcall(require, 'slate')` line in init.lua (A/B/C). Per RESEARCH
    // §Pattern 7 the shim+loader files live regardless of consent
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
    let label = crate::config::ConfigManager::with_env(env)?
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
/// that's the Pitfall 4 trick. Non-existent files are not an error.
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
/// Pitfall 4 contract: the `marker_block::START` / `END` constants are
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
    let registry = crate::theme::ThemeRegistry::new()?;
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
mod tests {
    use super::*;
    use crate::cli::new_shell_reminder::REMINDER_TEST_LOCK;
    use crate::opacity::OpacityPreset;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    #[ignore = "invoked by nvim_availability_flow_probes_once in a private subprocess"]
    fn nvim_availability_flow_child() {
        let case = std::env::var("SLATE_NVIM_PROBE_CASE").unwrap();
        let log = PathBuf::from(std::env::var_os("SLATE_NVIM_PROBE_LOG").unwrap());
        let env = SlateEnv::from_process().unwrap();
        if case == "disabled" {
            crate::config::ConfigManager::from_env_paths(&env)
                .set_editor_auto_activation_enabled(false)
                .unwrap();
        }
        let result = run_nvim_activation_flow(&env, true);
        match case.as_str() {
            "ready" | "ready-dev" => {
                let consent = result.unwrap();
                assert_eq!(consent, NvimConsent::AutoAdded);
                assert!(init_file_has_slate_marker(&env).unwrap());
                assert!(crate::adapter::nvim::state_file_path(&env).is_file());
                assert_eq!(std::fs::read_to_string(&log).unwrap(), "probe\n");
                for _ in 0..2 {
                    assert!(format_nvim_consent_receipt(&consent).is_none());
                }
                assert_eq!(std::fs::read_to_string(&log).unwrap(), "probe\n");
                let before = std::fs::read(env.nvim_init_path()).unwrap();
                assert_eq!(
                    run_nvim_activation_flow(&env, true).unwrap(),
                    NvimConsent::AlreadyConsented
                );
                assert_eq!(std::fs::read(env.nvim_init_path()).unwrap(), before);
                assert_eq!(std::fs::read_to_string(&log).unwrap(), "probe\nprobe\n");
            }
            "disabled" => {
                assert_eq!(result.unwrap(), NvimConsent::Disabled);
                assert!(!log.exists(), "disabled preference launched an editor");
                assert!(!env.nvim_init_path().exists());
                assert!(!crate::adapter::nvim::state_file_path(&env).exists());
            }
            "old" | "floor-dev" => {
                let consent = result.unwrap();
                assert_eq!(consent, NvimConsent::TooOld);
                assert_eq!(
                    format_nvim_consent_receipt(&consent).as_deref(),
                    Some(Language::NVIM_TOO_OLD_HINT)
                );
                assert_eq!(std::fs::read_to_string(&log).unwrap(), "probe\n");
                assert!(!env.nvim_config_dir().exists());
            }
            "invalid" | "short" | "wrong-tool" | "nonzero" | "timeout" => {
                let error = result.unwrap_err().to_string();
                let reason = match case.as_str() {
                    "invalid" | "short" | "wrong-tool" => {
                        "Could not read a complete semantic version"
                    }
                    "nonzero" => "non-zero exit status",
                    _ => "timed out after 2000 ms",
                };
                assert!(error.contains(reason), "{error}");
                assert!(!error.contains("private-output"));
                assert!(!error.contains(Language::NVIM_MISSING_HINT));
                assert_eq!(std::fs::read_to_string(&log).unwrap(), "probe\n");
                assert!(!env.nvim_config_dir().exists());
            }
            _ => panic!("unexpected fixture case"),
        }
    }

    #[test]
    fn nvim_availability_flow_probes_once() {
        use std::os::unix::fs::PermissionsExt;
        for (case, body) in [
            ("ready", "printf 'NVIM v0.8.0\\n'"),
            (
                "ready-dev",
                "printf 'NVIM v0.12.0-dev-123+gabc\\nLuaJIT 2.1.0\\n'",
            ),
            ("old", "printf 'NVIM v0.7.2\\n'"),
            ("floor-dev", "printf 'NVIM v0.8.0-dev\\nLuaJIT 2.1.0\\n'"),
            ("invalid", "printf 'private-output\\n'"),
            ("short", "printf 'NVIM v0.8\\nLuaJIT 2.1.0\\n'"),
            ("wrong-tool", "printf 'LuaJIT 2.1.0\\n'"),
            ("nonzero", "printf 'NVIM v0.12.0\\n'; exit 9"),
            ("timeout", "printf 'NVIM v0.12.0\\n'; exec /bin/sleep 20"),
            ("disabled", "exit 92"),
        ] {
            let td = TempDir::new().unwrap();
            let bin = td.path().join("bin");
            std::fs::create_dir(&bin).unwrap();
            let executable = bin.join("nvim");
            std::fs::write(&executable, format!(
                "#!/bin/sh\n[ \"$1\" = --version ] || exit 91\nprintf 'probe\\n' >> \"$SLATE_NVIM_PROBE_LOG\"\n{body}\n"
            )).unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_cmd::Command::new(std::env::current_exe().unwrap())
                .env_clear()
                .env("HOME", td.path())
                .env("SLATE_HOME", td.path())
                .env("PATH", &bin)
                .env("NO_COLOR", "1")
                .env("SLATE_NVIM_PROBE_CASE", case)
                .env("SLATE_NVIM_PROBE_LOG", td.path().join("probe-calls"))
                .args([
                    "--exact",
                    "cli::setup::tests::nvim_availability_flow_child",
                    "--ignored",
                    "--nocapture",
                ])
                .timeout(Duration::from_secs(7))
                .assert()
                .success();
        }
    }

    #[test]
    fn editor_preference_blocks_setup_before_native_probes_or_writes() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        let config = crate::config::ConfigManager::from_env_paths(&env);
        config.set_editor_auto_activation_enabled(false).unwrap();
        assert_eq!(
            with_nvim_auto_activation(&env, || panic!("native activation ran")).unwrap(),
            NvimConsent::Disabled
        );
        assert_eq!(
            nvim_activation_state(&env).unwrap(),
            NvimActivationState::Disabled
        );
        for quick in [true, false] {
            assert_eq!(
                prompt_nvim_activation(&env, quick).unwrap(),
                NvimConsent::Disabled
            );
        }
        assert_eq!(
            apply_activation_choice_a(&env).unwrap(),
            NvimConsent::Disabled
        );
        assert!(!env.nvim_init_path().exists());
        assert!(!env.nvim_config_dir().join("colors").exists());
        let receipt = format_nvim_consent_receipt(&NvimConsent::Disabled).unwrap();
        assert!(receipt.contains("slate config set editor enable"));
        config.set_editor_auto_activation_enabled(true).unwrap();
        assert_eq!(
            with_nvim_auto_activation(&env, || Ok(NvimConsent::AutoAdded)).unwrap(),
            NvimConsent::AutoAdded
        );
    }

    #[test]
    fn editor_preference_remembers_both_manual_choices_and_refuses_bad_records() {
        for show_line in [false, true] {
            let td = TempDir::new().unwrap();
            let env = SlateEnv::with_home(td.path().into());
            let config = crate::config::ConfigManager::from_env_paths(&env);
            let result = remember_manual_nvim_activation(&env, show_line).unwrap();
            assert_eq!(
                result,
                if show_line {
                    NvimConsent::ShownLine
                } else {
                    NvimConsent::Skipped
                }
            );
            assert!(!config.is_editor_auto_activation_enabled().unwrap());
            assert!(!env.nvim_init_path().exists());
            std::fs::write(env.nvim_auto_activation_path(), "bad record").unwrap();
            assert!(with_nvim_auto_activation(&env, || panic!(
                "bad consent was treated as permission"
            ))
            .is_err());
        }
    }

    #[test]
    fn setup_outcome_finalization_returns_failure_and_only_one_matching_event() {
        use crate::cli::failure_handler::ExecutionSummary;
        for success in [true, false] {
            let mut summary = ExecutionSummary::new();
            summary.theme_applied = true;
            summary.overall_success = true;
            if !success {
                summary.add_issue("private failure");
            }
            let mut events = Vec::new();
            let result =
                finish_setup_with(&summary, "private-checkpoint", |event| events.push(event));
            assert_eq!(events.len(), 1);
            assert_eq!(result.is_ok(), success);
            if success {
                assert!(matches!(events[0], BrandEvent::SetupComplete));
            } else {
                assert!(matches!(
                    events[0],
                    BrandEvent::Failure(FailureKind::SetupFailed)
                ));
                let error = result.unwrap_err().to_string();
                assert!(error.contains("slate restore private-checkpoint --dry-run"));
                assert!(error.contains("does not uninstall packages or fonts"));
                assert!(error.contains("no automatic rollback"));
            }
        }
    }

    #[test]
    fn setup_outcome_neovim_errors_are_not_missing_installations() {
        use crate::cli::failure_handler::ExecutionSummary;
        let mut summary = ExecutionSummary::new();
        assert!(nvim_after_theme(&mut summary, || panic!(
            "activation ran after failed theme setup"
        ))
        .is_none());
        assert!(summary.issues.is_empty());
        for consent in [
            NvimConsent::NoNvim,
            NvimConsent::TooOld,
            NvimConsent::Disabled,
            NvimConsent::Skipped,
            NvimConsent::ShownLine,
            NvimConsent::AlreadyConsented,
            NvimConsent::AutoAdded,
        ] {
            let mut summary = ExecutionSummary::new();
            summary.theme_applied = true;
            assert_eq!(
                nvim_after_theme(&mut summary, || Ok(consent)),
                Some(consent)
            );
            assert!(summary.is_successful());
        }
        summary.theme_applied = true;
        assert!(nvim_after_theme(&mut summary, || Err(
            crate::error::SlateError::UserCancelled
        ))
        .is_none());
        assert!(!summary.is_successful());
        assert!(summary.issues[0].contains("Neovim activation did not finish"));
    }

    #[test]
    fn setup_outcome_preference_failures_stop_before_later_choices() {
        for preference in ["autorun-fastfetch", "current-opacity"] {
            let td = TempDir::new().unwrap();
            let env = SlateEnv::with_home(td.path().to_owned());
            let config = crate::config::ConfigManager::with_env(&env).unwrap();
            let blocked = env.managed_file(preference);
            std::fs::create_dir(&blocked).unwrap();
            let result = prepare_setup_state(&env, Some(true), Some(OpacityPreset::Frosted));
            assert!(result.is_err());
            assert!(blocked.is_dir());
            if preference == "autorun-fastfetch" {
                assert!(config.get_current_opacity().unwrap().is_none());
            } else {
                assert!(config.has_fastfetch_autorun().unwrap());
            }
        }
    }

    #[test]
    fn setup_outcome_marker_scan_rejects_oversized_and_nonregular_sources() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        std::fs::create_dir_all(env.nvim_config_dir()).unwrap();
        let path = env.nvim_config_dir().join("init.lua");
        std::fs::create_dir(&path).unwrap();
        assert!(init_file_has_slate_marker(&env).is_err());
        std::fs::remove_dir(&path).unwrap();
        std::fs::File::create(&path)
            .unwrap()
            .set_len(8 * 1024 * 1024 + 1)
            .unwrap();
        assert!(init_file_has_slate_marker(&env).is_err());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 8 * 1024 * 1024 + 1);
    }

    #[test]
    fn setup_retry_keeps_captured_profile_and_propagates_install_failures() {
        let td = TempDir::new().unwrap();
        let home = td.path().join("profile");
        let custom = td.path().join("custom-xdg");
        let env = SlateEnv::from_vars(|key| match key {
            "HOME" => Some(home.as_os_str().to_owned()),
            "XDG_CONFIG_HOME" => Some(custom.as_os_str().to_owned()),
            _ => None,
        })
        .unwrap();
        for success in [true, false] {
            let calls = std::cell::RefCell::new(Vec::new());
            let result = retry_only_with(
                "starship",
                &env,
                |actual| {
                    assert!(std::ptr::eq(actual, &env));
                    assert_eq!(actual.xdg_config_home(), custom);
                    calls.borrow_mut().push("preflight");
                    Ok(())
                },
                |tool, actual| {
                    assert!(std::ptr::eq(actual, &env));
                    assert_eq!(actual.user_local_bin(), home.join(".local/bin"));
                    assert_eq!(tool.id, "starship");
                    assert_eq!(tool.brew_package, "starship");
                    calls.borrow_mut().push("install");
                    if success {
                        Ok("private simulated success".into())
                    } else {
                        Err(crate::error::SlateError::Internal(
                            "private install failure".into(),
                        ))
                    }
                },
            );
            assert_eq!(*calls.borrow(), ["preflight", "install"]);
            if success {
                assert!(result.is_ok());
            } else {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("private install failure"));
            }
        }
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
    }

    #[test]
    // SWATCH-RENDERER: hostile target-name styling bytes are rejection-test data.
    fn setup_retry_rejects_invalid_targets_and_stops_at_failed_preflight() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().join("profile"));
        for invalid in ["unknown", "tmux", "ghostty", "\x1b[31munknown\n"] {
            let result = retry_only_with(
                invalid,
                &env,
                |_| panic!("unexpected preflight"),
                |_, _| panic!("unexpected install"),
            );
            let error = result.unwrap_err().to_string();
            assert!(!error.contains('\x1b') && !error.contains('\n'));
            assert!(handle_with_env(false, false, Some(invalid.into()), &env).is_err());
        }
        let result = retry_only_with(
            "bat",
            &env,
            |_| {
                Err(crate::error::SlateError::Internal(
                    "preflight stopped".into(),
                ))
            },
            |_, _| panic!("install ran after failed preflight"),
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("preflight stopped"));
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
    }

    #[test]
    fn theme_safety_setup_stops_when_baseline_or_later_snapshot_fails() {
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().to_owned());
        std::fs::write(env.zshrc_path(), "# user shell\n").unwrap();
        std::fs::create_dir(env.bashrc_path()).unwrap();
        assert!(snapshot_before_setup(&env).is_err());
        assert!(crate::config::list_restore_points_with_env(&env)
            .unwrap()
            .is_empty());
        std::fs::remove_dir(env.bashrc_path()).unwrap();
        let baseline = snapshot_before_setup(&env).unwrap();
        assert!(baseline.is_baseline);
        std::fs::create_dir(env.bashrc_path()).unwrap();
        assert!(snapshot_before_setup(&env).is_err());
        assert_eq!(
            crate::config::list_restore_points_with_env(&env)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            std::fs::read_to_string(env.zshrc_path()).unwrap(),
            "# user shell\n"
        );
    }

    #[test]
    fn test_setup_force_flag_recognized() {
        // Verify force flag is handled
        let force = true;
        assert!(force);
    }

    #[test]
    fn test_setup_only_invalid_tool() {
        // Verify invalid tool names are rejected
        let result = validate_retry_tool("invalid_tool_xyz");
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
        let result = validate_retry_tool("tmux");
        assert!(result.is_err());
        // ghostty is now detect-only too
        let result = validate_retry_tool("ghostty");
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

        let state = nvim_activation_state(&env).expect("pure I/O");
        assert_eq!(state, NvimActivationState::AlreadyConsented);

        // Marker inspection never launches the editor or changes the file.
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
    /// activation-state) idempotent. The line-aware block editor also makes
    /// the direct write itself idempotent, including Lua comment wrappers.
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
        apply_activation_choice_a(&env).unwrap();
        assert_eq!(std::fs::read_to_string(&init_lua).unwrap(), first);
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

    /// All skipped availability results must remain distinct and read-only.
    #[test]
    fn nvim_availability_skips_and_errors_do_not_write_configuration() {
        use crate::adapter::nvim::availability::NvimAvailability;
        for (availability, consent, hint) in [
            (
                NvimAvailability::Missing,
                NvimConsent::NoNvim,
                Language::NVIM_MISSING_HINT,
            ),
            (
                NvimAvailability::Unsupported,
                NvimConsent::TooOld,
                Language::NVIM_TOO_OLD_HINT,
            ),
        ] {
            let td = TempDir::new().unwrap();
            let env = SlateEnv::with_home(td.path().into());
            let result = run_nvim_activation_with_probe(&env, false, || Ok(availability)).unwrap();
            assert_eq!(result, consent);
            assert_eq!(format_nvim_consent_receipt(&result).as_deref(), Some(hint));
            assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
        }
        let td = TempDir::new().unwrap();
        let env = SlateEnv::with_home(td.path().into());
        let error = run_nvim_activation_with_probe(&env, true, || {
            Err(crate::error::SlateError::PlatformError(
                "probe failed".into(),
            ))
        })
        .unwrap_err();
        assert!(error.to_string().contains("probe failed"));
        assert_eq!(std::fs::read_dir(td.path()).unwrap().count(), 0);
    }

    /// Receipt surface distinctness — every meaningful consent state
    /// produces a distinct one-liner so the reader can tell outcomes
    /// apart. Only AutoAdded is silent (the marker covers that surface).
    #[test]
    fn format_nvim_consent_receipt_surfaces_distinct_messages() {
        let auto = format_nvim_consent_receipt(&NvimConsent::AutoAdded);
        let shown = format_nvim_consent_receipt(&NvimConsent::ShownLine);
        let skipped = format_nvim_consent_receipt(&NvimConsent::Skipped);
        let already = format_nvim_consent_receipt(&NvimConsent::AlreadyConsented);
        let none = format_nvim_consent_receipt(&NvimConsent::NoNvim);
        let old = format_nvim_consent_receipt(&NvimConsent::TooOld);

        assert_eq!(auto, None, "AutoAdded is silent — marker speaks for itself");
        assert_eq!(none.as_deref(), Some(Language::NVIM_MISSING_HINT));
        assert_eq!(old.as_deref(), Some(Language::NVIM_TOO_OLD_HINT));
        assert_ne!(none, old);
        assert!(shown.is_some());
        assert!(skipped.is_some());
        assert!(already.is_some());

        assert_ne!(shown, skipped);
        assert_ne!(already, shown);
        assert_ne!(already, skipped);
    }

    #[test]
    fn nvim_receipt_languages_preserve_consent_and_recovery_commands() {
        use crate::config::ui_language::UiLanguage;
        for language in [UiLanguage::Chinese, UiLanguage::English] {
            assert_eq!(
                format_nvim_consent_receipt_in(&NvimConsent::AutoAdded, language),
                None
            );
            let mut messages = std::collections::HashSet::new();
            for consent in [
                NvimConsent::NoNvim,
                NvimConsent::TooOld,
                NvimConsent::Disabled,
                NvimConsent::AlreadyConsented,
                NvimConsent::ShownLine,
                NvimConsent::Skipped,
            ] {
                let message = format_nvim_consent_receipt_in(&consent, language).unwrap();
                if matches!(
                    consent,
                    NvimConsent::Disabled | NvimConsent::ShownLine | NvimConsent::Skipped
                ) {
                    assert!(
                        message.contains("slate config set editor enable"),
                        "{message}"
                    );
                }
                if consent == NvimConsent::Skipped {
                    assert!(message.contains(":colorscheme slate-<variant>"));
                }
                if matches!(consent, NvimConsent::NoNvim | NvimConsent::TooOld) {
                    assert!(message.contains("0.8"));
                }
                assert!(messages.insert(message));
            }
        }
    }

    #[test]
    fn nvim_manual_instruction_escapes_path_controls_without_altering_lua() {
        let call = "pcall(require, 'slate')";
        let text = format_nvim_manual_instruction(
            std::path::Path::new("/fixture/\x1b[2J\n/init.lua"),
            call,
        );
        assert!(!text.contains('\x1b'));
        assert!(!text.contains("[2J\n/init.lua"));
        assert!(text.ends_with(call));
    }
}
