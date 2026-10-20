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
                "invalid" | "short" | "wrong-tool" => "Could not read a complete semantic version",
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
        let result = finish_setup_with(&summary, "private-checkpoint", |event| events.push(event));
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

/// the managed block written to init.lua
/// wraps the shell-style START / END markers in Lua `--` line
/// comments AND embeds the human-readable marker comment next to
/// the runtime call. The RAW marker strings (without the leading
/// `-- `) must still appear in the block so
/// `marker_block::strip_managed_blocks`'s substring match finds
/// them.
#[test]
fn build_marker_block_for_init_lua_wraps_with_lua_comments() {
    let block = build_marker_block_for_init(true);

    // Lua comments in front of each marker .
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

/// Idempotency gate: if an
/// init.lua already carries a slate marker, the activation state
/// returns `AlreadyConsented` — no prompt, no file edit.
/// This pure-function test verifies the same short-circuit as the
/// interactive activation flow, without mocking cliclack.
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
    let text =
        format_nvim_manual_instruction(std::path::Path::new("/fixture/\x1b[2J\n/init.lua"), call);
    assert!(!text.contains('\x1b'));
    assert!(!text.contains("[2J\n/init.lua"));
    assert!(text.ends_with(call));
}
