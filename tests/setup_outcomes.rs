//! Real configuration-only executor runs in private subprocesses. These do not
//! request tool/font installations, run the interactive wizard or launch Neovim.
use slate_cli::brand::events::{set_sink, BrandEvent, EventSink, FailureKind};
use slate_cli::config::ConfigManager;
use slate_cli::env::SlateEnv;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

#[path = "support/tree.rs"]
mod tree;

#[derive(Default)]
struct SetupEvents(AtomicUsize);

impl EventSink for SetupEvents {
    fn dispatch(&self, event: BrandEvent) {
        if matches!(
            event,
            BrandEvent::SetupComplete | BrandEvent::Failure(FailureKind::SetupFailed)
        ) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
#[ignore = "invoked only in deadline-guarded private-profile subprocesses"]
fn setup_outcome_executor_child() {
    assert!(std::env::var_os("SLATE_HOME").is_some());
    let case = std::env::var("SLATE_SETUP_RESULT_CASE").unwrap();
    let env = SlateEnv::from_process().unwrap();
    let config = ConfigManager::with_env(&env).unwrap();
    config.set_current_theme("nord").unwrap();
    let events = Arc::new(SetupEvents::default());
    assert!(set_sink(events.clone()).is_ok());
    let alacritty = env.xdg_config_home().join("alacritty/alacritty.toml");
    let targets = if case == "adapter" {
        std::fs::create_dir_all(alacritty.parent().unwrap()).unwrap();
        std::fs::write(&alacritty, "[private broken TOML\n").unwrap();
        vec!["alacritty".to_string()]
    } else {
        Vec::new()
    };
    if case == "shellfile" {
        std::fs::create_dir(env.bash_integration_path()).unwrap();
    }
    let before = tree::tree(env.home());
    let result = slate_cli::cli::setup_executor::execute_setup_with_env(
        &[],
        &targets,
        None,
        if case == "theme" {
            Some("unknown-fixture-theme")
        } else if case == "shellfile" {
            Some("catppuccin-mocha")
        } else {
            None
        },
        &env,
    );
    if matches!(case.as_str(), "theme" | "shellfile") {
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains(if case == "theme" {
                "unknown-fixture-theme"
            } else {
                "shell loader"
            }),
            "{error}"
        );
        assert_eq!(events.0.load(Ordering::SeqCst), 0);
        assert_eq!(tree::tree(env.home()), before);
        assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
        return;
    }
    let summary = result.unwrap();
    let expected = case == "complete";
    assert_eq!(summary.is_successful(), expected);
    assert_eq!(summary.overall_success, expected);
    assert_eq!(summary.theme_applied, expected);
    assert!(summary.tool_results.is_empty());
    assert!(!summary.font_requested && !summary.font_available && !summary.font_applied);
    // Whole-setup milestones belong to the handler, after its follow-up steps.
    assert_eq!(events.0.load(Ordering::SeqCst), 0);
    let terminal = slate_cli::detection::TerminalProfile::from_env_vars(Some("ghostty"), None)
        .with_session(env.session().clone());
    let report = summary.format_completion_message_for_terminal(&terminal);
    assert_eq!(report.contains("Setup Complete!"), expected);
    assert!(!report.contains("Already Live"));
    assert!(report.contains("File-Only Session"));
    if !expected {
        assert!(!summary.issues.is_empty());
    }
    if case == "adapter" {
        assert_eq!(
            std::fs::read_to_string(&alacritty).unwrap(),
            "[private broken TOML\n"
        );
    }
    assert_eq!(config.get_current_theme().unwrap().as_deref(), Some("nord"));
}

#[test]
fn setup_outcome_real_executor_distinguishes_complete_and_incomplete_configuration() {
    for case in ["complete", "theme", "adapter", "shellfile"] {
        let td = tempfile::tempdir().unwrap();
        let output = assert_cmd::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("HOME", td.path())
            .env("SLATE_HOME", td.path())
            .env("PATH", td.path().join("bin"))
            .env("SHELL", "/bin/bash")
            .env("TERM_PROGRAM", "kitty")
            .env("NO_COLOR", "1")
            .env("SLATE_SETUP_RESULT_CASE", case)
            .args([
                "--exact",
                "setup_outcome_executor_child",
                "--ignored",
                "--nocapture",
            ])
            .timeout(Duration::from_secs(6))
            .assert()
            .success()
            .get_output()
            .clone();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains("Your terminal is beautiful"));
        assert!(!stderr.contains("Slate updated Kitty cleanly"));
        if case == "complete" {
            assert!(stderr.contains("nord"));
        } else {
            assert!(!stderr.contains("Configuration files updated"));
        }
    }
}
